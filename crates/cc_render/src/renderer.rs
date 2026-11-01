use crate::{SceneMesh, Vertex, view_proj};
use cc_core::CameraState;
use wgpu::util::DeviceExt;

// egui samples native textures as gamma-encoded values, so the shader encodes sRGB itself.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
const BACKGROUND: wgpu::Color = wgpu::Color { r: 0.84, g: 0.88, b: 0.93, a: 1.0 };

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    /// x: 1 for unlit faces.
    style: [f32; 4],
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve_texture: wgpu::Texture,
    resolve: wgpu::TextureView,
}

struct Buffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    transparent: wgpu::Buffer,
    transparent_count: u32,
    edges: wgpu::Buffer,
    edge_count: u32,
}

/// Draws a [`SceneMesh`] into an offscreen, multisampled texture.
pub struct Renderer3D {
    faces: wgpu::RenderPipeline,
    /// Blended faces that do not write depth (transparent materials).
    blended: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    buffers: Option<Buffers>,
    targets: Option<Targets>,
    /// Camera and size of the frame in the targets, while it is current:
    /// rendering it again is skipped.
    drawn: Option<(CameraState, [u32; 2])>,
    /// The mesh's faces are drawn without light.
    unlit: bool,
}

impl Renderer3D {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cc_render shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cc_render uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cc_render layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cc_render uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cc_render uniforms"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });

        let attributes =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        };
        let pipeline =
            |label: &str, topology, fs: &str, bias: wgpu::DepthBiasState, blend: Option<wgpu::BlendState>| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(vertex_layout.clone())],
                    },
                    primitive: wgpu::PrimitiveState { topology, cull_mode: None, ..Default::default() },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(blend.is_none()),
                        depth_compare: Some(wgpu::CompareFunction::LessEqual),
                        stencil: Default::default(),
                        bias,
                    }),
                    multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: COLOR_FORMAT,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        // Push faces slightly back so coplanar edges win the depth test.
        let face_bias = wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 };
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let faces = pipeline("cc_render faces", tri, "fs_face", face_bias, None);
        let blended = pipeline("cc_render blended", tri, "fs_face", face_bias, Some(wgpu::BlendState::ALPHA_BLENDING));
        let edges = pipeline("cc_render edges", wgpu::PrimitiveTopology::LineList, "fs_edge", Default::default(), None);
        Self { faces, blended, edges, uniforms, bind_group, buffers: None, targets: None, drawn: None, unlit: false }
    }

    pub fn upload(&mut self, device: &wgpu::Device, mesh: &SceneMesh) {
        self.drawn = None;
        self.unlit = mesh.unlit;
        if mesh.indices.is_empty() && mesh.transparent_indices.is_empty() && mesh.edges.is_empty() {
            self.buffers = None;
            return;
        }
        // A buffer beyond the device's limit is a validation error, which
        // wgpu treats as fatal: draw nothing rather than abort the app.
        let largest = [
            std::mem::size_of_val(mesh.vertices.as_slice()),
            std::mem::size_of_val(mesh.edges.as_slice()),
            std::mem::size_of_val(mesh.indices.as_slice()),
            std::mem::size_of_val(mesh.transparent_indices.as_slice()),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        if !fits(largest, device.limits().max_buffer_size) {
            self.buffers = None;
            return;
        }
        let init = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage })
        };
        // Zero-length buffers are invalid; pad with a dummy vertex.
        let pad = [Vertex::plain([0.0; 3], [0.0; 3], [0.0; 4])];
        let verts: &[Vertex] = if mesh.vertices.is_empty() { &pad } else { &mesh.vertices };
        let edges: &[Vertex] = if mesh.edges.is_empty() { &pad } else { &mesh.edges };
        let indices: &[u32] = if mesh.indices.is_empty() { &[0, 0, 0] } else { &mesh.indices };
        let transparent: &[u32] =
            if mesh.transparent_indices.is_empty() { &[0, 0, 0] } else { &mesh.transparent_indices };
        self.buffers = Some(Buffers {
            vertices: init("cc_render vertices", bytemuck::cast_slice(verts), wgpu::BufferUsages::VERTEX),
            indices: init("cc_render indices", bytemuck::cast_slice(indices), wgpu::BufferUsages::INDEX),
            index_count: mesh.indices.len() as u32,
            transparent: init("cc_render transparent", bytemuck::cast_slice(transparent), wgpu::BufferUsages::INDEX),
            transparent_count: mesh.transparent_indices.len() as u32,
            edges: init("cc_render edges", bytemuck::cast_slice(edges), wgpu::BufferUsages::VERTEX),
            edge_count: mesh.edges.len() as u32,
        });
    }

    /// (Re)create the render targets when the size changes; returns whether they were recreated.
    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) -> bool {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        self.targets = Some(Self::create_targets(device, size));
        true
    }

    fn create_targets(device: &wgpu::Device, size: [u32; 2]) -> Targets {
        let texture = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let view = |t: wgpu::Texture| t.create_view(&Default::default());
        let resolve_texture = texture(
            "cc_render resolve",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        Targets {
            size,
            msaa: view(texture("cc_render msaa", COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT)),
            depth: view(texture("cc_render depth", DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT)),
            resolve: resolve_texture.create_view(&Default::default()),
            resolve_texture,
        }
    }

    /// Render a frame (unless the last one, with the same mesh, camera and
    /// size, is still in the targets). Returns the output view and whether it
    /// was recreated (callers must then re-register it with egui); `None` if
    /// there are no targets.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        camera: &CameraState,
    ) -> Option<(&wgpu::TextureView, bool)> {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        let recreated = self.ensure_targets(device, size);
        if !recreated && self.drawn == Some((*camera, size)) {
            return self.targets.as_ref().map(|t| (&t.resolve, false));
        }
        self.drawn = Some((*camera, size));
        let aspect = size[0] as f32 / size[1] as f32;
        let fwd = camera.forward().as_vec3();
        // Sun from behind the viewer's left shoulder, high up.
        let light = (glam::Vec3::new(-fwd.x + 0.4, -fwd.y + 0.3, 1.6)).normalize();
        let u = Uniforms {
            view_proj: view_proj(camera, aspect).to_cols_array_2d(),
            light_dir: [light.x, light.y, light.z, 0.0],
            style: [if self.unlit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&u));

        let targets = self.targets.as_ref()?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cc_render") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cc_render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa,
                    depth_slice: None,
                    resolve_target: Some(&targets.resolve),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(BACKGROUND), store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(b) = &self.buffers {
                pass.set_bind_group(0, &self.bind_group, &[]);
                if b.index_count > 0 {
                    pass.set_pipeline(&self.faces);
                    pass.set_vertex_buffer(0, b.vertices.slice(..));
                    pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..b.index_count, 0, 0..1);
                }
                if b.edge_count > 0 {
                    pass.set_pipeline(&self.edges);
                    pass.set_vertex_buffer(0, b.edges.slice(..));
                    pass.draw(0..b.edge_count, 0..1);
                }
                // Transparent faces last, over everything opaque.
                if b.transparent_count > 0 {
                    pass.set_pipeline(&self.blended);
                    pass.set_vertex_buffer(0, b.vertices.slice(..));
                    pass.set_index_buffer(b.transparent.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..b.transparent_count, 0, 0..1);
                }
            }
        }
        queue.submit([encoder.finish()]);
        Some((&targets.resolve, recreated))
    }

    /// Render one frame of `size` pixels and read it back as tightly packed
    /// RGBA bytes (already sRGB encoded), e.g. for thumbnails.
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        camera: &CameraState,
    ) -> Option<Vec<u8>> {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        self.render(device, queue, size, camera)?;
        let targets = self.targets.as_ref()?;
        let row = size[0] * 4;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cc_render readback"),
            size: u64::from(padded) * u64::from(size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cc_render readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.resolve_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut rgba = Vec::with_capacity((row * size[1]) as usize);
        for y in 0..size[1] as usize {
            let start = y * padded as usize;
            rgba.extend_from_slice(&data[start..start + row as usize]);
        }
        drop(data);
        buffer.unmap();
        Some(rgba)
    }
}

/// Whether a buffer of `bytes` is within the device's `limit`, and its
/// element counts fit the `u32` draw ranges.
fn fits(bytes: usize, limit: u64) -> bool {
    u64::try_from(bytes).is_ok_and(|b| b <= limit) && bytes / std::mem::size_of::<u32>() <= u32::MAX as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshes_beyond_the_buffer_limit_are_not_uploaded() {
        let limit = wgpu::Limits::default().max_buffer_size;
        assert!(fits(0, limit) && fits(1 << 20, limit));
        assert!(!fits(usize::try_from(limit).unwrap_or(usize::MAX).saturating_add(1), limit));
    }
}

#[cfg(test)]
mod shader_tests {
    /// The shader parses and validates (as wgpu does when the pipelines are
    /// made): derivatives in uniform control flow, attributes and bindings.
    #[test]
    fn the_shader_validates() {
        let module = naga::front::wgsl::parse_str(include_str!("shader.wgsl")).unwrap();
        let mut validator =
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty());
        validator.validate(&module).unwrap();
        for entry in ["vs_main", "fs_face", "fs_edge"] {
            assert!(module.entry_points.iter().any(|e| e.name == entry), "{entry}");
        }
    }
}
