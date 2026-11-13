use std::ffi::c_void;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::sync::Arc;

use egui_glow::glow;
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::prelude::{GlSurface, NotCurrentGlContext};
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};

use crate::player::GlLookup;

pub struct Graphics {
    display: Display,
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    pub gl: Arc<glow::Context>,
}

fn dimension(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}

pub fn open(
    wayland_display: NonNull<c_void>,
    wayland_surface: NonNull<c_void>,
    pixels: [u32; 2],
) -> Result<Graphics, String> {
    let fail = |error: glutin::error::Error| format!("OpenGL could not start: {error}");
    let raw_display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(wayland_display));
    let raw_window = RawWindowHandle::Wayland(WaylandWindowHandle::new(wayland_surface));
    let display = unsafe { Display::new(raw_display, DisplayApiPreference::Egl) }.map_err(fail)?;
    let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_transparency(true).build();
    let config = unsafe { display.find_configs(template) }
        .map_err(fail)?
        .min_by_key(|config| config.num_samples())
        .ok_or("OpenGL has no pixel format with transparency")?;
    let surface_attributes =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window, dimension(pixels[0]), dimension(pixels[1]));
    let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }.map_err(fail)?;
    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window));
    let context = unsafe { display.create_context(&config, &context_attributes) }
        .map_err(fail)?
        .make_current(&surface)
        .map_err(fail)?;
    let _ = surface.set_swap_interval(&context, SwapInterval::DontWait);
    let gl = unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) };
    Ok(Graphics { display, context, surface, gl: Arc::new(gl) })
}

impl Graphics {
    pub fn lookup(&self) -> GlLookup {
        GlLookup { display: self.display.clone() }
    }

    pub fn resize(&self, pixels: [u32; 2]) {
        self.surface.resize(&self.context, dimension(pixels[0]), dimension(pixels[1]));
    }

    pub fn present(&self) {
        let _ = self.surface.swap_buffers(&self.context);
    }
}
