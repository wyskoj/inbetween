//! The live preview: the GPU renderer drawing straight into a texture egui shows, or, with no
//! GPU, the CPU renderer feeding an ordinary egui texture.

use crate::imaging::to_color_image;
use eframe::egui_wgpu::{self, RenderState};
use eframe::wgpu;
use egui::{TextureHandle, TextureId, TextureOptions};
use inbetween_engine::Prepared;
use inbetween_render_gpu::{GpuContext, GpuRenderer};
use std::sync::Arc;

/// The GPU objects the preview shares with eframe, so a frame the renderer draws is already
/// in a texture egui can paint.
#[derive(Clone)]
pub struct GpuHandle {
    pub ctx: GpuContext,
    pub renderer: Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
}

impl GpuHandle {
    pub fn from_render_state(state: &RenderState) -> Self {
        let info = state.adapter.get_info();
        Self {
            ctx: GpuContext::from_parts(
                state.device.clone(),
                state.queue.clone(),
                info.name,
                format!("{:?}", info.backend),
            ),
            renderer: Arc::clone(&state.renderer),
        }
    }
}

/// What a renderer was built from. When it changes the renderer is rebuilt; otherwise a changed
/// sequence is handed over in place.
#[derive(Clone, Copy, PartialEq, Eq)]
struct SourceKey {
    image1: usize,
    image2: usize,
    output: (u32, u32),
}

struct GpuPreview {
    renderer: GpuRenderer,
    key: SourceKey,
    texture: TextureId,
    revision: u64,
}

#[derive(Default)]
pub struct Preview {
    gpu: Option<GpuPreview>,
    cpu: Option<TextureHandle>,
    drawn: Option<(u64, u32)>,
    /// Why the current frame could not be drawn (the last good one stays on screen).
    pub error: Option<String>,
    gpu_failed: bool,
}

fn key_of(prepared: &Prepared) -> SourceKey {
    SourceKey {
        image1: Arc::as_ptr(&prepared.image1) as usize,
        image2: prepared
            .image2
            .as_ref()
            .map_or(0, |i| Arc::as_ptr(i) as usize),
        output: prepared.output,
    }
}

impl Preview {
    /// The texture to paint, if a frame has been drawn.
    pub fn texture_id(&self) -> Option<TextureId> {
        match (&self.gpu, &self.cpu) {
            (Some(g), _) => Some(g.texture),
            (None, Some(c)) => Some(c.id()),
            _ => None,
        }
    }

    pub fn backend_name(&self) -> &'static str {
        if self.gpu.is_some() {
            "GPU"
        } else {
            "CPU"
        }
    }

    /// Forget everything (no project, or the project was replaced).
    pub fn clear(&mut self, handle: Option<&GpuHandle>) {
        self.release_gpu(handle);
        self.cpu = None;
        self.drawn = None;
        self.error = None;
    }

    fn release_gpu(&mut self, handle: Option<&GpuHandle>) {
        if let (Some(old), Some(h)) = (self.gpu.take(), handle) {
            h.renderer.write().free_texture(&old.texture);
        }
    }

    /// Bring the preview up to date for `revision` and `frame`. Cheap when nothing changed.
    pub fn refresh(
        &mut self,
        ctx: &egui::Context,
        handle: Option<&GpuHandle>,
        prepared: &Prepared,
        revision: u64,
        frame: u32,
    ) {
        if self.drawn == Some((revision, frame)) {
            return;
        }
        self.drawn = Some((revision, frame));
        // Effects, the camera and the master fade are drawn by the CPU renderer only (D34, D36),
        // so a sequence with any of them on previews on the CPU even when a GPU is available.
        // A caption (sequence or movie) is drawn on the CPU as well.
        let gpu = handle.filter(|_| {
            !self.gpu_failed
                && !prepared.sequence.needs_post_pass()
                && prepared.movie_caption.is_none()
        });
        let outcome = match gpu {
            Some(h) => self.refresh_gpu(h, prepared, revision, frame),
            None => {
                self.release_gpu(handle);
                self.refresh_cpu(ctx, prepared, frame)
            }
        };
        self.error = outcome.err();
    }

    /// Show a finished frame (a layer composite, drawn by the CPU) for `revision` and `frame`.
    /// `render` is only called when that frame is not already on screen.
    pub fn refresh_composite(
        &mut self,
        ctx: &egui::Context,
        handle: Option<&GpuHandle>,
        revision: u64,
        frame: u32,
        render: impl FnOnce() -> Result<inbetween_raster::Image, String>,
    ) {
        // A frame number far outside any real movie marks "a composite", so switching between
        // the sequence preview and the composite always redraws.
        const COMPOSITE: u32 = 1 << 31;
        let key = (revision, frame | COMPOSITE);
        if self.drawn == Some(key) {
            return;
        }
        self.drawn = Some(key);
        self.release_gpu(handle);
        let outcome = render().map(|image| {
            let color = to_color_image(&image, ctx.input(|i| i.max_texture_side));
            match &mut self.cpu {
                Some(handle) => handle.set(color, TextureOptions::LINEAR),
                None => self.cpu = Some(ctx.load_texture("preview", color, TextureOptions::LINEAR)),
            }
        });
        self.error = outcome.err();
    }

    fn refresh_gpu(
        &mut self,
        handle: &GpuHandle,
        prepared: &Prepared,
        revision: u64,
        frame: u32,
    ) -> Result<(), String> {
        let key = key_of(prepared);
        let stale = self.gpu.as_ref().is_none_or(|g| g.key != key);
        if stale {
            self.release_gpu(Some(handle));
            let renderer = GpuRenderer::new(
                &handle.ctx,
                &prepared.sequence,
                prepared.output,
                &prepared.image1,
                prepared.image2.as_deref(),
            );
            match renderer {
                Ok(renderer) => {
                    let view = renderer.preview_view_srgb();
                    let texture = handle.renderer.write().register_native_texture(
                        handle.ctx.device(),
                        &view,
                        wgpu::FilterMode::Linear,
                    );
                    self.gpu = Some(GpuPreview {
                        renderer,
                        key,
                        texture,
                        revision,
                    });
                }
                Err(e) => {
                    // Typically a dot problem (two dots on one spot) or an image the GPU
                    // refuses; an oversized image falls back to the CPU for good.
                    if matches!(e, inbetween_render_gpu::GpuError::TooLarge { .. }) {
                        self.gpu_failed = true;
                    }
                    return Err(e.to_string());
                }
            }
        }
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(());
        };
        if gpu.revision != revision {
            gpu.renderer
                .set_sequence(&prepared.sequence)
                .map_err(|e| e.to_string())?;
            gpu.revision = revision;
        }
        gpu.renderer
            .draw_preview_frame(frame)
            .map_err(|e| e.to_string())
    }

    fn refresh_cpu(
        &mut self,
        ctx: &egui::Context,
        prepared: &Prepared,
        frame: u32,
    ) -> Result<(), String> {
        let renderer = prepared.renderer().map_err(|e| e.to_string())?;
        let mut image = renderer.render_frame(frame).map_err(|e| e.to_string())?;
        if let Some(layer) = &prepared.movie_caption {
            inbetween_render_cpu::caption::composite(&mut image, layer);
        }
        let color = to_color_image(&image, ctx.input(|i| i.max_texture_side));
        match &mut self.cpu {
            Some(handle) => handle.set(color, TextureOptions::LINEAR),
            None => self.cpu = Some(ctx.load_texture("preview", color, TextureOptions::LINEAR)),
        }
        Ok(())
    }
}
