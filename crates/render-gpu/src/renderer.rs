use crate::vertices::build_vertices;
use crate::{GpuContext, GpuError, Vertex};
use inbetween_doc::Sequence;
use inbetween_geom::Affine;
use inbetween_morph::{fit_transform, transparent_outside, FramePlan, Mix, MixMode, Planner};
use inbetween_raster::Image;
use wgpu::util::DeviceExt;

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
/// The 8-bit target a preview shows. egui expects sRGB-encoded premultiplied values in a plain
/// `Rgba8Unorm` texture, which is exactly what the shader writes (the working space is sRGB).
pub const PREVIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BYTES_PER_PIXEL: u32 = 16;
const FLAG_TRANSPARENT_1: u32 = 1;
const FLAG_TRANSPARENT_2: u32 = 2;
const FLAG_HAS_2: u32 = 4;

/// Must match `struct Params` in `shader.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    out_size: [f32; 2],
    dims1: [f32; 2],
    dims2: [f32; 2],
    w1: f32,
    w2: f32,
    bright1: f32,
    bright2: f32,
    /// 0 adds the weighted samples, 1 lays Image 2 over Image 1.
    mode: u32,
    flags: u32,
}

/// Renders the frames of one sequence on the GPU. Build it once (this uploads the source
/// images), then draw any number of frames.
pub struct GpuRenderer {
    ctx: GpuContext,
    planner: Planner,
    size: (u32, u32),
    fit1: Affine,
    fit2: Affine,
    base_params: Params,
    pipeline: wgpu::RenderPipeline,
    preview_pipeline: wgpu::RenderPipeline,
    src1_size: (u32, u32),
    src2_size: Option<(u32, u32)>,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: u64,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    preview: wgpu::Texture,
    preview_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_row_bytes: u32,
}

fn upload(ctx: &GpuContext, image: &Image, label: &str) -> wgpu::Texture {
    ctx.device.create_texture_with_data(
        &ctx.queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        bytemuck::cast_slice(image.as_slice()),
    )
}

impl GpuRenderer {
    /// `output` is the frame size in pixels. `image2` may be `None` only for morph types that
    /// never show Image 2 (such as Warp 1). Sources are fitted exactly as the CPU renderer
    /// fits them.
    pub fn new(
        ctx: &GpuContext,
        seq: &Sequence,
        output: (u32, u32),
        image1: &Image,
        image2: Option<&Image>,
    ) -> Result<Self, GpuError> {
        let (width, height) = output;
        Image::new(width, height)?; // rejects zero and absurd sizes before touching the GPU
        let max = ctx.max_texture_side();
        let too_large = |w: u32, h: u32| {
            (w > max || h > max).then_some(GpuError::TooLarge {
                width: w,
                height: h,
                max,
            })
        };
        if let Some(e) = too_large(width, height) {
            return Err(e);
        }
        for img in std::iter::once(image1).chain(image2) {
            if let Some(e) = too_large(img.width(), img.height()) {
                return Err(e);
            }
        }
        if image2.is_none() && seq.morph.uses_image2() {
            return Err(GpuError::MissingImage2);
        }

        let planner = Planner::new(seq, width, height)?;
        let fit1 = fit_transform(image1.width(), image1.height(), width, height, seq.fit)?;
        let fit2 = match image2 {
            Some(i) => fit_transform(i.width(), i.height(), width, height, seq.fit)?,
            None => Affine::IDENTITY,
        };
        let transparent = transparent_outside(seq.fit);

        let dummy;
        let source2 = match image2 {
            Some(i) => i,
            None => {
                dummy = Image::new(1, 1)?;
                &dummy
            }
        };
        let tex1 = upload(ctx, image1, "image 1");
        let tex2 = upload(ctx, source2, "image 2");
        let view1 = tex1.create_view(&wgpu::TextureViewDescriptor::default());
        let view2 = tex2.create_view(&wgpu::TextureViewDescriptor::default());

        let mut flags = 0;
        if transparent {
            flags |= FLAG_TRANSPARENT_1 | FLAG_TRANSPARENT_2;
        }
        if image2.is_some() {
            flags |= FLAG_HAS_2;
        }
        let base_params = Params {
            out_size: [width as f32, height as f32],
            dims1: [image1.width() as f32, image1.height() as f32],
            dims2: [source2.width() as f32, source2.height() as f32],
            w1: 1.0,
            w2: 0.0,
            bright1: 1.0,
            bright2: 1.0,
            mode: 0,
            flags,
        };
        let device = &ctx.device;
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&base_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("inbetween layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1),
                texture_entry(2),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("inbetween bindings"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view1),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view2),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("inbetween shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("inbetween pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let make_pipeline = |format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("inbetween pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Triangles may wind either way (folded meshes), so never cull.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
        };
        let pipeline = make_pipeline(TARGET_FORMAT);
        let preview_pipeline = make_pipeline(PREVIEW_FORMAT);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let preview = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: PREVIEW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            // egui decodes the textures it shows as sRGB, so it needs this second view.
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let preview_view = preview.create_view(&wgpu::TextureViewDescriptor::default());
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row_bytes = (width * BYTES_PER_PIXEL).div_ceil(align) * align;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vertex_capacity = 1024 * std::mem::size_of::<Vertex>() as u64;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertices"),
            size: vertex_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            ctx: ctx.clone(),
            planner,
            size: output,
            fit1,
            fit2,
            base_params,
            pipeline,
            preview_pipeline,
            src1_size: (image1.width(), image1.height()),
            src2_size: image2.map(|i| (i.width(), i.height())),
            bind_group,
            params_buffer,
            vertex_buffer,
            vertex_capacity,
            target,
            target_view,
            preview,
            preview_view,
            readback,
            padded_row_bytes,
        })
    }

    /// Every frame of the sequence, hold frames included.
    pub fn frame_count(&self) -> u32 {
        self.planner.total_frames()
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The texture the last [`GpuRenderer::draw`] rendered into (`Rgba32Float`, premultiplied).
    /// A preview can sample it directly without a round trip through the CPU.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.target
    }

    /// Render at morph time `t` into [`GpuRenderer::texture`]. Returns once the work is
    /// submitted; call [`GpuRenderer::wait`] before reading the texture on the CPU.
    pub fn draw(&mut self, t: f64) -> Result<(), GpuError> {
        let plan = self.planner.plan(t)?;
        self.submit(&plan, false)
    }

    /// Like [`GpuRenderer::draw`] for frame `index`, counting hold frames (clamped to the last).
    pub fn draw_frame(&mut self, index: u32) -> Result<(), GpuError> {
        let plan = self.planner.plan_frame(index)?;
        self.submit(&plan, false)
    }

    /// Like [`GpuRenderer::draw`], but into the 8-bit [`GpuRenderer::preview_texture`] that a
    /// UI can show without any conversion. About the same cost; no readback.
    pub fn draw_preview(&mut self, t: f64) -> Result<(), GpuError> {
        let plan = self.planner.plan(t)?;
        self.submit(&plan, true)
    }

    /// Like [`GpuRenderer::draw_preview`] for frame `index`, counting hold frames.
    pub fn draw_preview_frame(&mut self, index: u32) -> Result<(), GpuError> {
        let plan = self.planner.plan_frame(index)?;
        self.submit(&plan, true)
    }

    /// The 8-bit texture [`GpuRenderer::draw_preview`] renders into (`Rgba8Unorm`, holding
    /// sRGB-encoded premultiplied colour, which is how egui reads its textures).
    pub fn preview_texture(&self) -> &wgpu::Texture {
        &self.preview
    }

    /// A view of [`GpuRenderer::preview_texture`] that reads the stored bytes as sRGB. egui
    /// (like most UI toolkits) expects to decode sRGB textures to linear while sampling and to
    /// re-encode on output; with this view the colours come out exactly as the source images.
    pub fn preview_view_srgb(&self) -> wgpu::TextureView {
        self.preview.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        })
    }

    /// Use a changed sequence (moved dots, a new frame count or fit mode) without uploading
    /// the source images again. The sources must be the ones this renderer was built with;
    /// build a new renderer when the images or the output size change.
    pub fn set_sequence(&mut self, seq: &Sequence) -> Result<(), GpuError> {
        if self.src2_size.is_none() && seq.morph.uses_image2() {
            return Err(GpuError::MissingImage2);
        }
        let (width, height) = self.size;
        let planner = Planner::new(seq, width, height)?;
        let fit1 = fit_transform(self.src1_size.0, self.src1_size.1, width, height, seq.fit)?;
        let fit2 = match self.src2_size {
            Some((w, h)) => fit_transform(w, h, width, height, seq.fit)?,
            None => Affine::IDENTITY,
        };
        let mut flags = self.base_params.flags & !(FLAG_TRANSPARENT_1 | FLAG_TRANSPARENT_2);
        if transparent_outside(seq.fit) {
            flags |= FLAG_TRANSPARENT_1 | FLAG_TRANSPARENT_2;
        }
        self.planner = planner;
        self.fit1 = fit1;
        self.fit2 = fit2;
        self.base_params.flags = flags;
        Ok(())
    }

    fn submit(&mut self, plan: &FramePlan, preview: bool) -> Result<(), GpuError> {
        let vertices = build_vertices(plan, &self.fit1, &self.fit2, self.size.0, self.size.1);
        let bytes: &[u8] = bytemuck::cast_slice(&vertices);

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;
        if bytes.len() as u64 > self.vertex_capacity {
            self.vertex_capacity = (bytes.len() as u64).next_power_of_two();
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vertices"),
                size: self.vertex_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytes);
        let Mix {
            mode,
            w1,
            w2,
            bright1,
            bright2,
        } = plan.mix;
        let params = Params {
            w1,
            w2,
            bright1,
            bright2,
            mode: match mode {
                MixMode::Add => 0,
                MixMode::Over => 1,
            },
            ..self.base_params
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: if preview {
                        &self.preview_view
                    } else {
                        &self.target_view
                    },
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(if preview {
                &self.preview_pipeline
            } else {
                &self.pipeline
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..bytes.len() as u64));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Block until everything submitted so far has finished.
    pub fn wait(&self) {
        self.ctx.device.poll(wgpu::Maintain::Wait);
    }

    /// Draw frame `index`, counting hold frames (clamped to the last frame), and read it back.
    pub fn render_frame(&mut self, index: u32) -> Result<Image, GpuError> {
        self.draw_frame(index)?;
        self.read_back()
    }

    /// Draw at morph time `t` and read the result back into a CPU image.
    pub fn render_t(&mut self, t: f64) -> Result<Image, GpuError> {
        self.draw(t)?;
        self.read_back()
    }

    fn read_back(&self) -> Result<Image, GpuError> {
        let (width, height) = self.size;
        let device = &self.ctx.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("readback"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.ctx.queue.submit([encoder.finish()]);

        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;

        let mut image = Image::new(width, height)?;
        {
            let mapped = slice.get_mapped_range();
            let row_floats = width as usize * 4;
            let dst = image.as_mut_slice();
            for (y, row) in dst.chunks_exact_mut(row_floats).enumerate() {
                let start = y * self.padded_row_bytes as usize;
                let src = mapped
                    .get(start..start + row_floats * 4)
                    .ok_or_else(|| GpuError::Readback("short buffer".into()))?;
                let floats: &[f32] =
                    bytemuck::try_cast_slice(src).map_err(|e| GpuError::Readback(e.to_string()))?;
                row.copy_from_slice(floats);
            }
        }
        self.readback.unmap();
        Ok(image)
    }
}
