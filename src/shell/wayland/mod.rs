mod graphics;
mod hyprland;
mod input;

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::ping::make_ping;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_keyboard, wl_output, wl_pointer, wl_region, wl_seat, wl_surface,
};
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use smithay_client_toolkit::reexports::protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers};
use smithay_client_toolkit::seat::pointer::{
    CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::relative_pointer::{
    RelativeMotionEvent, RelativePointerHandler, RelativePointerState,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};

use crate::app::{App, FrameInput, Gesture, Wants};
use crate::command::Command;
use crate::control;
use crate::placement::{self, Rect, Size};
use crate::player::Player;
use crate::plex::worker::Wake;

use graphics::Graphics;
use hyprland::CursorProbe;

const NAMESPACE: &str = "sth-plex-pip";
const HEARTBEAT: Duration = Duration::from_millis(80);
const FIRST_SIZE: Size = Size { width: 560.0, height: 420.0 };

struct Drag {
    gesture: Gesture,
    from: Rect,
    travelled: (f32, f32),
}

struct Overlay {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    relative_pointers: RelativePointerState,
    shm: Shm,
    connection: Connection,
    queue: QueueHandle<Overlay>,
    layer: LayerSurface,
    output: Option<wl_output::WlOutput>,
    pointer: Option<ThemedPointer>,
    relative_pointer: Option<ZwpRelativePointerV1>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    graphics: Option<Graphics>,
    app: Option<App>,
    commands: Option<Receiver<Command>>,
    wake: Wake,
    probe: Option<CursorProbe>,
    events: Vec<egui::Event>,
    held: egui::Modifiers,
    pointer_inside: bool,
    cursor_over: bool,
    rect: Rect,
    scale: i32,
    drag: Option<Drag>,
    wants: Option<Wants>,
    click_through: Option<bool>,
    keyboard_on: Option<bool>,
    dirty: bool,
    repaint_at: Option<Instant>,
    started: Instant,
    failure: Option<String>,
    exit: bool,
}

impl Overlay {
    fn output_info(&self) -> Option<smithay_client_toolkit::output::OutputInfo> {
        self.output
            .as_ref()
            .or(self.outputs.outputs().next().as_ref())
            .and_then(|output| self.outputs.info(output))
    }

    fn screen(&self) -> Size {
        self.output_info()
            .and_then(|info| info.logical_size)
            .map(|(width, height)| Size { width: width as f32, height: height as f32 })
            .unwrap_or(Size { width: 1920.0, height: 1080.0 })
    }

    fn screen_origin(&self) -> (f32, f32) {
        self.output_info()
            .and_then(|info| info.logical_position)
            .map(|(x, y)| (x as f32, y as f32))
            .unwrap_or((0.0, 0.0))
    }

    fn pixels(&self) -> [u32; 2] {
        let scale = self.scale.max(1) as f32;
        [(self.rect.width * scale) as u32, (self.rect.height * scale) as u32]
    }

    fn place(&mut self, rect: Rect) {
        if rect == self.rect {
            return;
        }
        let resized = rect.size() != self.rect.size();
        self.rect = rect;
        self.layer.set_margin(rect.y as i32, 0, 0, rect.x as i32);
        if resized {
            self.layer.set_size(rect.width as u32, rect.height as u32);
            self.fit_graphics();
        }
        self.layer.commit();
        self.dirty = true;
    }

    fn fit_graphics(&mut self) {
        let pixels = self.pixels();
        if let Some(graphics) = &self.graphics {
            graphics.resize(pixels);
        }
    }

    fn start(&mut self) -> Result<(), String> {
        let display = NonNull::new(self.connection.backend().display_ptr() as *mut c_void)
            .ok_or("no Wayland display")?;
        let surface = NonNull::new(self.layer.wl_surface().id().as_ptr() as *mut c_void)
            .ok_or("no Wayland surface")?;
        let graphics = graphics::open(display, surface, self.pixels())?;
        let player = Player::new(
            graphics.lookup(),
            graphics::proc_address,
            Some(display.as_ptr() as *const c_void),
            self.wake.clone(),
        )
        .map_err(|error| format!("mpv could not start: {error}"))?;
        let commands = self.commands.take().ok_or("already started")?;
        self.app = Some(App::new(graphics.gl.clone(), player, commands, self.wake.clone())?);
        self.graphics = Some(graphics);
        Ok(())
    }

    fn set_click_through(&mut self, through: bool) {
        if self.click_through == Some(through) {
            return;
        }
        self.click_through = Some(through);
        if !through {
            self.layer.wl_surface().set_input_region(None);
            return;
        }
        let region = self.compositor.wl_compositor().create_region(&self.queue, ());
        self.layer.wl_surface().set_input_region(Some(&region));
        region.destroy();
        self.pointer_inside = false;
    }

    fn set_keyboard(&mut self, on: bool) {
        if self.keyboard_on == Some(on) {
            return;
        }
        self.keyboard_on = Some(on);
        let interactivity = if on { KeyboardInteractivity::OnDemand } else { KeyboardInteractivity::None };
        self.layer.set_keyboard_interactivity(interactivity);
    }

    fn resting_rect(&self, wants: &Wants) -> Rect {
        let screen = self.screen();
        let width = placement::fitted_width(screen, wants.size.width);
        let height = wants.size.height * width / wants.size.width;
        placement::in_corner(screen, Size { width, height: height.round() }, wants.corner)
    }

    fn apply(&mut self, wants: Wants) {
        self.exit = self.exit || wants.quit;
        self.set_click_through(!wants.look.takes_clicks);
        self.set_keyboard(wants.keyboard);
        self.repaint_at = wants.repaint_after.map(|delay| Instant::now() + delay);
        if let (Some(gesture), None) = (wants.gesture, &self.drag) {
            self.drag = Some(Drag { gesture, from: self.rect, travelled: (0.0, 0.0) });
        }
        if self.drag.is_none() {
            self.place(self.resting_rect(&wants));
        }
        self.wants = Some(wants);
        self.layer.commit();
    }

    fn raw_input(&mut self) -> egui::RawInput {
        let size = egui::vec2(self.rect.width, self.rect.height);
        let viewport = egui::ViewportInfo {
            native_pixels_per_point: Some(self.scale.max(1) as f32),
            ..Default::default()
        };
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            viewports: [(egui::ViewportId::ROOT, viewport)].into_iter().collect(),
            time: Some(self.started.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        }
    }

    fn draw(&mut self) {
        self.dirty = false;
        if self.app.is_none() {
            return;
        }
        let input = FrameInput {
            raw: self.raw_input(),
            pixels: self.pixels(),
            hovered: self.pointer_inside || self.cursor_over,
            modifier_held: false,
        };
        let wants = self.app.as_mut().map(|app| app.frame(input));
        if let Some(graphics) = &self.graphics {
            graphics.present();
        }
        wants.into_iter().for_each(|wants| self.apply(wants));
    }

    fn cursor_is_over(&self) -> bool {
        let Some((x, y)) = self.probe.as_ref().and_then(CursorProbe::cursor) else { return false };
        let (left, top) = self.screen_origin();
        self.rect.contains(x - left, y - top)
    }

    fn heartbeat(&mut self) {
        let over = self.click_through == Some(true) && self.cursor_is_over();
        if over != self.cursor_over {
            self.cursor_over = over;
            self.dirty = true;
        }
        if self.repaint_at.is_some_and(|at| at <= Instant::now()) {
            self.repaint_at = None;
            self.dirty = true;
        }
    }

    fn dragged(&mut self, dx: f32, dy: f32) {
        let (Some(drag), Some(wants)) = (self.drag.as_mut(), self.wants) else { return };
        drag.travelled = (drag.travelled.0 + dx, drag.travelled.1 + dy);
        let (x, y) = drag.travelled;
        let rect = match drag.gesture {
            Gesture::Move => drag.from.moved(x, y),
            Gesture::Resize => placement::resized(drag.from, wants.corner, x, y, wants.aspect, self.screen()),
        };
        self.place(rect);
    }

    fn dropped(&mut self) {
        let Some(drag) = self.drag.take() else { return };
        let corner = match (drag.gesture, self.wants) {
            (Gesture::Resize, Some(wants)) => wants.corner,
            _ => placement::nearest_corner(self.screen(), self.rect),
        };
        let width = self.rect.width;
        if let Some(app) = self.app.as_mut() {
            app.placed(corner, width);
        }
        self.dirty = true;
    }
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, factor: i32) {
        self.scale = factor.max(1);
        surface.set_buffer_scale(self.scale);
        self.fit_graphics();
        self.dirty = true;
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, output: &wl_output::WlOutput) {
        self.output = Some(output.clone());
        self.dirty = true;
    }

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.dirty = true;
    }

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Overlay {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
        if self.app.is_none() {
            self.failure = self.start().err();
            self.exit = self.failure.is_some();
        }
        self.dirty = true;
    }
}

impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, queue: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(queue, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            let cursor_surface = self.compositor.create_surface(queue);
            self.pointer = self
                .seats
                .get_pointer_with_theme_and_data(queue, &seat, self.shm.wl_shm(), cursor_surface, ThemeSpec::System, ())
                .ok();
            self.relative_pointer = self
                .pointer
                .as_ref()
                .and_then(|pointer| self.relative_pointers.get_relative_pointer(pointer.pointer(), queue).ok());
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard {
            self.keyboard.take().iter().for_each(wl_keyboard::WlKeyboard::release);
        }
        if capability == Capability::Pointer {
            self.relative_pointer.take().iter().for_each(ZwpRelativePointerV1::destroy);
            self.pointer = None;
            self.pointer_inside = false;
            self.drag = None;
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Overlay {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.events.extend(input::key_down(&event, self.held, false));
        self.dirty = true;
    }

    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.events.extend(input::key_down(&event, self.held, true));
        self.dirty = true;
    }

    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.events.extend(input::key_up(&event, self.held));
        self.dirty = true;
    }

    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, modifiers: Modifiers, _: RawModifiers, _: u32) {
        self.held = input::modifiers(modifiers);
    }
}

impl Overlay {
    fn pointed(&mut self, connection: &Connection, event: &PointerEvent) {
        match event.kind {
            PointerEventKind::Enter { .. } => {
                self.pointer_inside = true;
                self.pointer.iter().for_each(|pointer| {
                    let _ = pointer.set_cursor(connection, CursorIcon::Default);
                });
            }
            PointerEventKind::Leave { .. } => self.pointer_inside = false,
            _ => {}
        }
        if self.drag.is_some() && input::is_primary_release(event) {
            self.dropped();
        }
        let moving = self.drag.is_some() && matches!(event.kind, PointerEventKind::Motion { .. });
        if !moving {
            self.events.extend(input::pointer(event, self.held));
        }
        self.dirty = true;
    }
}

impl PointerHandler for Overlay {
    fn pointer_frame(&mut self, connection: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        let surface = self.layer.wl_surface().clone();
        events
            .iter()
            .filter(|event| event.surface == surface)
            .for_each(|event| self.pointed(connection, event));
    }
}

impl RelativePointerHandler for Overlay {
    fn relative_pointer_motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &ZwpRelativePointerV1, _: &wl_pointer::WlPointer, event: RelativeMotionEvent) {
        self.dragged(event.delta.0 as f32, event.delta.1 as f32);
    }
}

impl ShmHandler for Overlay {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<wl_region::WlRegion, ()> for Overlay {
    fn event(_: &mut Self, _: &wl_region::WlRegion, _: wl_region::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_registry!(Overlay);
smithay_client_toolkit::delegate_dispatch2!(Overlay);

pub fn run() -> Result<(), String> {
    let wayland = |error: &dyn std::fmt::Display| format!("Wayland: {error}");
    let connection = Connection::connect_to_env().map_err(|error| wayland(&error))?;
    let (globals, event_queue) = registry_queue_init::<Overlay>(&connection).map_err(|error| wayland(&error))?;
    let queue = event_queue.handle();
    let mut event_loop: EventLoop<Overlay> = EventLoop::try_new().map_err(|error| wayland(&error))?;
    let handle = event_loop.handle();
    WaylandSource::new(connection.clone(), event_queue)
        .insert(handle.clone())
        .map_err(|error| wayland(&error))?;

    let compositor = CompositorState::bind(&globals, &queue).map_err(|error| wayland(&error))?;
    let layer_shell = LayerShell::bind(&globals, &queue).map_err(|_| {
        "This compositor has no layer-shell, which the overlay needs (GNOME has none).".to_string()
    })?;
    let shm = Shm::bind(&globals, &queue).map_err(|error| wayland(&error))?;

    let (ping, pings) = make_ping().map_err(|error| wayland(&error))?;
    let wake: Wake = Arc::new(move || ping.ping());
    handle
        .insert_source(pings, |_, _, overlay| overlay.dirty = true)
        .map_err(|error| wayland(&error))?;
    handle
        .insert_source(Timer::from_duration(HEARTBEAT), |_, _, overlay| {
            overlay.heartbeat();
            TimeoutAction::ToDuration(HEARTBEAT)
        })
        .map_err(|error| wayland(&error))?;

    let (sender, commands) = channel::<Command>();
    control::listen(sender, wake.clone()).map_err(|error| format!("control socket: {error}"))?;

    let surface = compositor.create_surface(&queue);
    let layer = layer_shell.create_layer_surface(&queue, surface, Layer::Overlay, Some(NAMESPACE), None);
    let rect = Rect { x: 0.0, y: 0.0, width: FIRST_SIZE.width, height: FIRST_SIZE.height };
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(rect.width as u32, rect.height as u32);
    layer.commit();

    let mut overlay = Overlay {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &queue),
        outputs: OutputState::new(&globals, &queue),
        relative_pointers: RelativePointerState::bind(&globals, &queue),
        compositor,
        shm,
        connection,
        layer,
        queue,
        output: None,
        pointer: None,
        relative_pointer: None,
        keyboard: None,
        graphics: None,
        app: None,
        commands: Some(commands),
        wake,
        probe: CursorProbe::find(),
        events: vec![],
        held: egui::Modifiers::default(),
        pointer_inside: false,
        cursor_over: false,
        rect,
        scale: 1,
        drag: None,
        wants: None,
        click_through: None,
        keyboard_on: None,
        dirty: false,
        repaint_at: None,
        started: Instant::now(),
        failure: None,
        exit: false,
    };

    let signal = event_loop.get_signal();
    event_loop
        .run(None, &mut overlay, |overlay| {
            if overlay.dirty {
                overlay.draw();
            }
            if overlay.exit {
                signal.stop();
            }
        })
        .map_err(|error| wayland(&error))
        .and_then(|_| overlay.failure.take().map_or(Ok(()), Err))
}
