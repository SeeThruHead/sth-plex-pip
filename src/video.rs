use std::sync::Arc;

use egui_glow::Painter;
use egui_glow::glow::{self, HasContext};

struct Surface {
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
    size: [i32; 2],
}

pub struct VideoTarget {
    gl: Arc<glow::Context>,
    surface: Option<Surface>,
    texture_id: Option<egui::TextureId>,
}

fn surface(gl: &glow::Context, size: [i32; 2]) -> Option<Surface> {
    unsafe {
        let texture = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            size[0],
            size[1],
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        let framebuffer = gl.create_framebuffer().ok()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        Some(Surface { texture, framebuffer, size })
    }
}

impl VideoTarget {
    pub fn new(gl: Arc<glow::Context>) -> Self {
        Self { gl, surface: None, texture_id: None }
    }

    fn fits(&self, size: [i32; 2]) -> bool {
        self.surface.as_ref().is_some_and(|surface| surface.size == size)
    }

    fn release(&mut self) {
        if let Some(old) = self.surface.take() {
            unsafe {
                self.gl.delete_framebuffer(old.framebuffer);
            }
        }
    }

    fn register(&mut self, painter: &mut Painter, texture: glow::Texture) -> egui::TextureId {
        match self.texture_id {
            Some(id) => {
                painter.replace_native_texture(id, texture);
                id
            }
            None => painter.register_native_texture(texture),
        }
    }

    pub fn fit(&mut self, painter: &mut Painter, size: [i32; 2]) {
        if self.fits(size) || size[0] <= 0 || size[1] <= 0 {
            return;
        }
        self.release();
        self.surface = surface(&self.gl, size);
        self.texture_id =
            self.surface.as_ref().map(|surface| surface.texture).map(|texture| self.register(painter, texture));
    }

    pub fn framebuffer(&self) -> Option<(i32, [i32; 2])> {
        self.surface.as_ref().map(|surface| (surface.framebuffer.0.get() as i32, surface.size))
    }

    pub fn texture(&self) -> Option<egui::TextureId> {
        self.texture_id
    }
}
