mod hotkeys;
#[cfg(target_os = "macos")]
#[path = "mac.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui_glow::glow;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::{GlSurface, NotCurrentGlContext};
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowId, WindowLevel};

use crate::app::{App, FrameInput, LIBRARY_SIZE, Wants};
use crate::command::Command;
use crate::control;
use crate::placement::{self, Drag, Rect, Size};
use crate::player::{self, GlLookup, Player};
use crate::plex::worker::Wake;
use crate::shell::dimension;
use crate::store;

const HEARTBEAT: Duration = Duration::from_millis(80);
const DRAG_BEAT: Duration = Duration::from_millis(12);

struct Graphics {
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
}

struct Running {
    app: App,
    input: egui_winit::State,
    graphics: Graphics,
    window: Window,
}

struct Desktop {
    wake: Wake,
    commands: Option<Receiver<Command>>,
    running: Option<Running>,
    rect: Option<Rect>,
    drag: Option<(Drag, (f32, f32))>,
    cursor_over: bool,
    click_through: Option<bool>,
    opacity: Option<f32>,
    takes_keys: Option<bool>,
    visible: Option<bool>,
    repaint_at: Option<Instant>,
    next_beat: Instant,
    failure: Option<String>,
    quit: bool,
}

fn window_attributes() -> winit::window::WindowAttributes {
    let attributes = Window::default_attributes()
        .with_title("sth-plex-pip")
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_active(false)
        .with_visible(false)
        .with_inner_size(LogicalSize::new(LIBRARY_SIZE.width, LIBRARY_SIZE.height));
    #[cfg(target_os = "windows")]
    let attributes = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes.with_skip_taskbar(true)
    };
    attributes
}

fn open(event_loop: &ActiveEventLoop, wake: Wake, commands: Receiver<Command>) -> Result<Running, String> {
    let fail = |error: &dyn std::fmt::Display| format!("The window could not open: {error}");
    let template = ConfigTemplateBuilder::new().with_depth_size(0).with_stencil_size(0);
    let (window, config) = DisplayBuilder::new()
        .with_window_attributes(Some(window_attributes()))
        .build(event_loop, template, |mut configs| configs.next().expect("glutin offers at least one config"))
        .map_err(|error| fail(&error))?;
    let window = match window {
        Some(window) => window,
        None => {
            glutin_winit::finalize_window(event_loop, window_attributes(), &config).map_err(|error| fail(&error))?
        }
    };
    let handle = window.window_handle().map_err(|error| fail(&error))?.as_raw();
    let display = config.display();
    let size = window.inner_size();
    let surface_attributes =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(handle, dimension(size.width), dimension(size.height));
    let surface =
        unsafe { display.create_window_surface(&config, &surface_attributes) }.map_err(|error| fail(&error))?;
    let context_attributes = ContextAttributesBuilder::new().build(Some(handle));
    let context = unsafe { display.create_context(&config, &context_attributes) }
        .map_err(|error| fail(&error))?
        .make_current(&surface)
        .map_err(|error| fail(&error))?;
    let _ = surface.set_swap_interval(&context, SwapInterval::DontWait);
    let gl = Arc::new(unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) });
    let player = Player::new(GlLookup { display }, player::proc_address, None, wake.clone())
        .map_err(|error| format!("mpv could not start: {error}"))?;
    let app = App::new(gl, player, commands, wake)?;
    let input = egui_winit::State::new(
        app.context(),
        egui::ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
        None,
    );
    window.set_visible(true);
    platform::prepare(&window);
    Ok(Running { window, graphics: Graphics { context, surface }, input, app })
}

impl Running {
    fn scale(&self) -> f64 {
        self.window.scale_factor()
    }

    fn monitor(&self) -> Option<winit::monitor::MonitorHandle> {
        self.window.current_monitor().or_else(|| self.window.primary_monitor())
    }

    fn screen(&self) -> (Size, (f32, f32)) {
        self.monitor()
            .map(|monitor| {
                let size = monitor.size().to_logical::<f32>(monitor.scale_factor());
                let origin = monitor.position().to_logical::<f32>(monitor.scale_factor());
                (Size { width: size.width, height: size.height }, (origin.x, origin.y))
            })
            .unwrap_or((Size { width: 1920.0, height: 1080.0 }, (0.0, 0.0)))
    }

    fn fit(&self, size: PhysicalSize<u32>) {
        self.graphics.surface.resize(&self.graphics.context, dimension(size.width), dimension(size.height));
    }

    fn put(&self, rect: Rect) {
        let (_, origin) = self.screen();
        let _ = self.window.request_inner_size(LogicalSize::new(rect.width, rect.height));
        self.window.set_outer_position(LogicalPosition::new(origin.0 + rect.x, origin.1 + rect.y));
    }

    fn resting(&self, wants: &Wants) -> Rect {
        placement::resting(self.screen().0, wants.size, wants.corner)
    }
}

impl Desktop {
    fn place(&mut self, rect: Rect) {
        if self.rect == Some(rect) {
            return;
        }
        self.rect = Some(rect);
        if let Some(running) = &self.running {
            running.put(rect);
            running.window.request_redraw();
        }
    }

    fn set_click_through(&mut self, through: bool) {
        if self.click_through == Some(through) {
            return;
        }
        self.click_through = Some(through);
        self.opacity = None;
        self.takes_keys = None;
        if let Some(running) = &self.running {
            let _ = running.window.set_cursor_hittest(!through);
        }
    }

    fn set_visible(&mut self, visible: bool) {
        if self.visible == Some(visible) {
            return;
        }
        self.visible = Some(visible);
        if let Some(running) = &self.running {
            running.window.set_visible(visible);
        }
    }

    fn set_takes_keys(&mut self, takes: bool) {
        if self.takes_keys == Some(takes) {
            return;
        }
        self.takes_keys = Some(takes);
        if let Some(running) = &self.running {
            platform::set_takes_keys(&running.window, takes);
        }
    }

    fn set_opacity(&mut self, opacity: f32) {
        if self.opacity == Some(opacity) {
            return;
        }
        self.opacity = Some(opacity);
        if let Some(running) = &self.running {
            platform::set_opacity(&running.window, opacity);
        }
    }

    fn begin_drag(&mut self, wants: &Wants) {
        let (Some(gesture), None, Some(from), Some(running)) = (wants.gesture, self.drag, self.rect, &self.running)
        else {
            return;
        };
        let Some(cursor) = platform::cursor(running.scale()) else { return };
        self.drag = Some((Drag { gesture, from, corner: wants.corner, aspect: wants.aspect }, cursor));
    }

    fn apply(&mut self, wants: Wants) {
        self.quit = self.quit || wants.quit;
        self.set_visible(wants.look.visible);
        self.set_click_through(!wants.look.takes_clicks);
        self.set_takes_keys(wants.keyboard);
        self.set_opacity(wants.look.opacity);
        self.repaint_at = wants.repaint_after.map(|delay| Instant::now() + delay);
        self.begin_drag(&wants);
        let resting = self.running.as_ref().map(|running| running.resting(&wants));
        if let (None, Some(rect)) = (self.drag, resting) {
            self.place(rect);
        }
    }

    fn draw(&mut self) {
        let hovered = self.cursor_over;
        let Some(running) = self.running.as_mut() else { return };
        let size = running.window.inner_size();
        let input = FrameInput {
            raw: running.input.take_egui_input(&running.window),
            pixels: [size.width, size.height],
            hovered,
            modifier_held: platform::modifier_held(),
            window_fades: true,
        };
        let wants = running.app.frame(input);
        let _ = running.graphics.surface.swap_buffers(&running.graphics.context);
        self.apply(wants);
    }

    fn drop_drag(&mut self) {
        let (Some((drag, _)), Some(rect)) = (self.drag.take(), self.rect) else { return };
        let Some(running) = self.running.as_mut() else { return };
        let (screen, _) = running.screen();
        running.app.placed(drag.dropped_in(rect, screen), rect.width);
        running.window.request_redraw();
    }

    fn follow_drag(&mut self, cursor: Option<(f32, f32)>) {
        let (Some((drag, start)), Some(cursor)) = (self.drag, cursor) else { return };
        if !platform::primary_button_down() {
            self.drop_drag();
            return;
        }
        let Some(screen) = self.running.as_ref().map(|running| running.screen().0) else { return };
        self.place(drag.after(cursor.0 - start.0, cursor.1 - start.1, screen));
    }

    fn heartbeat(&mut self) {
        let Some(running) = &self.running else { return };
        let cursor = platform::cursor(running.scale());
        let (_, origin) = running.screen();
        let over = match (cursor, self.rect) {
            (Some((x, y)), Some(rect)) => rect.contains(x - origin.0, y - origin.1),
            _ => false,
        };
        let due = self.repaint_at.is_some_and(|at| at <= Instant::now());
        if over || over != self.cursor_over || due {
            running.window.request_redraw();
        }
        self.cursor_over = over;
        if due {
            self.repaint_at = None;
        }
        self.follow_drag(cursor);
    }
}

impl ApplicationHandler for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(commands) = self.commands.take() else { return };
        match open(event_loop, self.wake.clone(), commands) {
            Ok(running) => self.running = Some(running),
            Err(message) => {
                self.failure = Some(message);
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        if let Some(running) = &self.running {
            running.window.request_redraw();
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(running) = self.running.as_mut() else { return };
        let _ = running.input.on_window_event(&running.window, &event);
        match event {
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CloseRequested => self.quit = true,
            WindowEvent::Resized(size) => {
                running.fit(size);
                running.window.request_redraw();
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                running.window.request_redraw();
                self.drop_drag();
            }
            _ => running.window.request_redraw(),
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.running = None;
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit {
            event_loop.exit();
            return;
        }
        if Instant::now() >= self.next_beat {
            self.heartbeat();
            self.next_beat = Instant::now() + if self.drag.is_some() { DRAG_BEAT } else { HEARTBEAT };
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_beat));
    }
}

fn event_loop() -> Result<EventLoop<()>, String> {
    let mut builder = EventLoop::<()>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    builder.build().map_err(|error| format!("The event loop could not start: {error}"))
}

pub fn run() -> Result<(), String> {
    let event_loop = event_loop()?;
    let proxy = event_loop.create_proxy();
    let wake: Wake = Arc::new(move || {
        let _ = proxy.send_event(());
    });
    let (sender, commands) = channel::<Command>();
    control::listen(sender.clone(), wake.clone()).map_err(|error| format!("control socket: {error}"))?;
    let _hotkeys = hotkeys::register(&store::settings().hotkeys, sender, wake.clone());
    let mut desktop = Desktop {
        wake,
        commands: Some(commands),
        running: None,
        rect: None,
        drag: None,
        cursor_over: false,
        click_through: None,
        opacity: None,
        takes_keys: None,
        visible: None,
        repaint_at: None,
        next_beat: Instant::now(),
        failure: None,
        quit: false,
    };
    event_loop
        .run_app(&mut desktop)
        .map_err(|error| format!("The event loop failed: {error}"))
        .and_then(|_| desktop.failure.take().map_or(Ok(()), Err))
}
