//! Per-frame image rendering pipeline for the GUI backend.

use std::collections::HashMap;
use std::rc::Weak;

use crate::prelude::Vec2;
use crate::render::image::ImageSource;

use super::pipeline::{bytes_of, slice_bytes};

const SHADER_SRC: &str = r#"
struct ImgUniforms {
    viewport_px: vec2<f32>,
    cell_px: vec2<f32>,
    pad_px: vec2<f32>,
    _pad: vec2<f32>,
}
@group(0) @binding(0) var<uniform> u: ImgUniforms;
@group(0) @binding(1) var img_sampler: sampler;
@group(1) @binding(0) var img_texture: texture_2d<f32>;

struct ImgIn {
    @location(0) cell_xy: vec2<u32>,
    @location(1) span_wh: vec2<u32>,
    @location(2) uv_rect: vec4<f32>,
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(inst: ImgIn, @builtin(vertex_index) vid: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 0.0),
    );
    let corner = corners[vid];
    let origin = vec2<f32>(inst.cell_xy) * u.cell_px;
    let size = vec2<f32>(inst.span_wh) * u.cell_px;
    var pos = origin + corner * size + u.pad_px;
    var clip = (pos / u.viewport_px) * 2.0 - vec2<f32>(1.0, 1.0);
    clip.y = -clip.y;
    var out: VsOut;
    out.pos = vec4<f32>(clip, 0.0, 1.0);
    out.uv = mix(inst.uv_rect.xy, inst.uv_rect.zw, corner);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(img_texture, img_sampler, in.uv);
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct ImgUniforms {
    viewport_px: [f32; 2],
    cell_px: [f32; 2],
    pad_px: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ImageInstance {
    pub cell_xy: [u16; 2],
    pub span_wh: [u16; 2],
    pub uv_rect: [f32; 4],
    pub source_key: *const (),
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuInstance {
    cell_xy: [u16; 2],
    span_wh: [u16; 2],
    uv_rect: [f32; 4],
}

struct CachedTexture {
    bind_group: wgpu::BindGroup,
    weak: Weak<crate::render::image::source::SourceInner>,
}

const IMG_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Uint16x2,
    1 => Uint16x2,
    2 => Float32x4,
];

pub(super) struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    group0: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,

    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instances: Vec<ImageInstance>,
    groups: Vec<(*const (), u32, u32)>,

    textures: HashMap<*const (), CachedTexture>,
}

impl ImagePipeline {
    /// Creates an [`ImagePipeline`] for the given `surface_format`.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image uniforms"),
            size: std::mem::size_of::<ImgUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let group0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image group0 bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<ImgUniforms>() as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image texture bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let group0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image group0"),
            layout: &group0_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image pipeline layout"),
            bind_group_layouts: &[&group0_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &IMG_ATTRS,
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_capacity = 16usize;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image instance buffer"),
            size: (instance_capacity * std::mem::size_of::<GpuInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            uniform_buffer,
            group0,
            texture_layout,
            instance_buffer,
            instance_capacity,
            instances: Vec::new(),
            groups: Vec::new(),
            textures: HashMap::new(),
        }
    }

    /// Clears per-frame instance and group lists.
    pub fn begin_frame(&mut self) {
        self.instances.clear();
        self.groups.clear();
    }

    /// Queues one image quad for the current frame.
    pub fn push(&mut self, instance: ImageInstance) {
        self.instances.push(instance);
    }

    /// Returns the cache key for `source`, uploading its pixels if not already cached.
    pub fn ensure_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &ImageSource,
    ) -> Option<*const ()> {
        let key = source.identity();
        if let Some(cached) = self.textures.get(&key) {
            if cached.weak.strong_count() > 0 {
                return Some(key);
            }
            self.textures.remove(&key);
        }
        let dims = source.get_pixel_dims();
        let rgba = source.get_rgba()?;
        let bind_group = upload_rgba(device, queue, &self.texture_layout, dims, &rgba)?;
        let weak = std::rc::Rc::downgrade(&source.inner);
        self.textures.insert(key, CachedTexture { bind_group, weak });
        Some(key)
    }

    /// Uploads instance data and uniforms for the current frame.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: (f32, f32),
        cell_px: (f32, f32),
        pad_px: (f32, f32),
    ) {
        if self.instances.is_empty() {
            return;
        }

        let u = ImgUniforms {
            viewport_px: [viewport.0, viewport.1],
            cell_px: [cell_px.0, cell_px.1],
            pad_px: [pad_px.0, pad_px.1],
            _pad: [0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytes_of(&u));

        self.instances
            .sort_by(|a, b| (a.source_key as usize).cmp(&(b.source_key as usize)));

        self.groups.clear();
        let mut i = 0u32;
        while (i as usize) < self.instances.len() {
            let key = self.instances[i as usize].source_key;
            let mut j = i + 1;
            while (j as usize) < self.instances.len()
                && self.instances[j as usize].source_key == key
            {
                j += 1;
            }
            self.groups.push((key, i, j));
            i = j;
        }

        let n = self.instances.len();
        if n > self.instance_capacity {
            self.instance_capacity = n.next_power_of_two().max(16);
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image instance buffer"),
                size: (self.instance_capacity * std::mem::size_of::<GpuInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let mut gpu: Vec<GpuInstance> = Vec::with_capacity(n);
        for inst in &self.instances {
            gpu.push(GpuInstance {
                cell_xy: inst.cell_xy,
                span_wh: inst.span_wh,
                uv_rect: inst.uv_rect,
            });
        }
        queue.write_buffer(&self.instance_buffer, 0, slice_bytes(&gpu));
    }

    /// Records draw calls for all image instances prepared this frame.
    pub fn record_draws<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.instances.is_empty() || self.groups.is_empty() {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.group0, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        for (key, start, end) in &self.groups {
            let Some(cached) = self.textures.get(key) else {
                continue;
            };
            render_pass.set_bind_group(1, &cached.bind_group, &[]);
            render_pass.draw(0..6, *start..*end);
        }
    }

    /// Removes cached textures whose source has been dropped.
    pub fn evict_dead(&mut self) {
        self.textures.retain(|_, v| v.weak.strong_count() > 0);
    }
}

fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_layout: &wgpu::BindGroupLayout,
    dims: Vec2<u32>,
    rgba: &[u8],
) -> Option<wgpu::BindGroup> {
    if dims.x == 0 || dims.y == 0 {
        return None;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("image source"),
        size: wgpu::Extent3d {
            width: dims.x,
            height: dims.y,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(dims.x * 4),
            rows_per_image: Some(dims.y),
        },
        wgpu::Extent3d {
            width: dims.x,
            height: dims.y,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("image source bg"),
        layout: texture_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    Some(bind_group)
}
