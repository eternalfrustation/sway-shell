use itertools::Itertools;
use std::mem;

use std::{borrow::Cow, ptr::NonNull, sync::Arc};

use ab_glyph::Font;
use bytemuck::Zeroable;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use tokio::{
    runtime::Handle,
    sync::{RwLock, mpsc::Receiver},
};
use wayland_client::{Proxy, protocol::wl_surface::WlSurface};
use wgpu::{AddressMode, DeviceDescriptor, FilterMode, SamplerDescriptor};
use wgpu::{Buffer, BufferDescriptor, IndexFormat, PresentMode, RenderPipeline, util::DeviceExt};

use crate::font::{FontContainer, GlyphOffLen};
use crate::layer::DisplayMessage;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

#[repr(C)]
// This is so we can store this in a buffer
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobalTransformUniform {
    scale: [f32; 2],
    translate: [f32; 2],
}

impl GlobalTransformUniform {
    fn new() -> Self {
        Self {
            scale: [1., 1.],
            translate: [0., 0.],
        }
    }
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    pub position: [f32; 2],
    pub scale: [f32; 2],
    pub bg: u32,
    pub fg: u32,
    pub lines_off: GlyphOffLen,
    pub quadratic_off: GlyphOffLen,
    pub cubic_off: GlyphOffLen,
}

impl Instance {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Instance>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We'll have to reassemble the mat4 in the shader.
                wgpu::VertexAttribute {
                    offset: 0,
                    // While our vertex shader uses locations 0 and 1
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Unorm8x4,
                },
                wgpu::VertexAttribute {
                    offset: 20,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Unorm8x4,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Uint32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Uint32x2,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Uint32x2,
                },
            ],
        }
    }
}

pub struct Renderer {
    pub width: u32,
    pub height: u32,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub render_pipeline: RenderPipeline,
    pub square_vb: Buffer,
    pub square_ib: Buffer,
    pub square_num_vertices: u32,
    pub global_transform_uniform_buffer: Buffer,
    pub pipeline_bind_group: wgpu::BindGroup,
    pub instance_buffer: Buffer,
    pub font_lines_points_buffer: Buffer,
    pub font_quadratic_points_buffer: Buffer,
    pub font_cubic_points_buffer: Buffer,
    pub font_sdf: FontContainer,
}

#[derive(Debug)]
pub enum Renderable {
    Text {
        text: String,
        fg: u32,
        bg: u32,
    },
    Space(f32),
    Box {
        fg: u32,
        bg: u32,
        width: f32,
        height: f32,
        skip: f32,
    },
}

pub struct RenderState {
    pub left: Vec<Renderable>,
    pub right: Vec<Renderable>,
    pub center: Vec<Renderable>,
}

const SQUARE: &[Vertex] = &[
    Vertex {
        position: [0., 1.],
        tex_coords: [0., 0.],
    },
    Vertex {
        position: [0., -1.],
        tex_coords: [0., 1.],
    },
    Vertex {
        position: [1., -1.],
        tex_coords: [1., 1.],
    },
    Vertex {
        position: [1., 1.],
        tex_coords: [1., 0.],
    },
];

const SQUARE_INDICES: &[u16] = &[0, 1, 3, 3, 1, 2];

impl Renderer {
    pub async fn new(
        wayland_conn: &wayland_client::Connection,
        wayland_surface: &WlSurface,
        width: u32,
        height: u32,
    ) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(wayland_conn.backend().display_ptr() as *mut _)
                .expect("Wayland display pointer to be not null"),
        ));

        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(wayland_surface.id().as_ptr() as *mut _)
                .expect("Wayland surface pointer to be not null"),
        ));
        let surface = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle,
                    raw_window_handle,
                })
                .unwrap()
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("Failed to find suitable adapter");

        let device_descriptor = DeviceDescriptor {
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&device_descriptor)
            .await
            .expect("Failed to request device");

        // Loading the font
        // Need to write custom code for this part
        let font_container = FontContainer::new(
            "|QWERTYUIOPASDFGHJKLZXCVBNMqwertyuiopasdfghjklzxcvbnm1234567890[];',./<>?:\"{}+_)(*&^%$#@!~󱞁`= ",
        );
        // Load the shaders from disk
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let global_transform_uniform = GlobalTransformUniform::new();
        let global_transform_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Global Transform Buffer"),
                contents: bytemuck::cast_slice(&[global_transform_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let pipeline_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
            label: Some("total_bind_group_layout"),
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Font Sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            lod_min_clamp: 1.,
            lod_max_clamp: 1.,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        let font_lines_points_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Font Lines texture"),
                contents: &[0; 1024 * 1024],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        let font_quadratic_points_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Font Quad texture"),
                contents: &[0; 1024 * 1024],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        let font_cubic_points_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Font Cubic texture"),
                contents: &[0; 1024 * 1024],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        let pipeline_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &pipeline_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: global_transform_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: font_lines_points_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: font_quadratic_points_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: font_cubic_points_buffer.as_entire_binding(),
                },
            ],
            label: Some("pipeline_bind_group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&pipeline_layout],
            push_constant_ranges: &[],
        });

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc(), Instance::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(swapchain_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let square_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Square Vertex Buffer"),
            contents: bytemuck::cast_slice(SQUARE),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let square_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Square Index Buffer"),
            contents: bytemuck::cast_slice(SQUARE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // You can now only create 128 squares
        let instance_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Instance Buffer"),
            size: 1024 * mem::size_of::<Instance>() as u64,
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::VERTEX.union(wgpu::BufferUsages::COPY_DST),
        });

        Self {
            font_lines_points_buffer,
            font_quadratic_points_buffer,
            font_cubic_points_buffer,
            font_sdf: font_container,
            width,
            height,
            adapter,
            device,
            queue,
            surface,
            render_pipeline,
            square_vb,
            square_ib,
            instance_buffer,
            square_num_vertices: SQUARE_INDICES.len() as u32,
            global_transform_uniform_buffer,
            pipeline_bind_group,
        }
    }

    fn update_font(&self) {
        self.queue.write_buffer(
            &self.font_lines_points_buffer,
            0,
            bytemuck::cast_slice(&self.font_sdf.linear_points_buffer),
        );
        self.queue.write_buffer(
            &self.font_quadratic_points_buffer,
            0,
            bytemuck::cast_slice(&self.font_sdf.quadratic_points_buffer),
        );
        self.queue.write_buffer(
            &self.font_cubic_points_buffer,
            0,
            bytemuck::cast_slice(&self.font_sdf.cubic_points_buffer),
        );
    }

    fn to_renderable(
        &mut self,
        renderables: &Vec<Renderable>,
        initial_skip: f32,
    ) -> (Vec<Instance>, f32) {
        let mut instances = Vec::new();
        let mut skip = initial_skip;
        for item in renderables.into_iter() {
            match item {
                Renderable::Text { text, fg, bg } => {
                    let id = match text
                        .chars()
                        .map(|c| self.font_sdf.font_arc.glyph_id(c))
                        .next()
                    {
                        Some(id) => id,
                        None => continue,
                    };

                    let glyph_info = match self.font_sdf.load_char_with_id(id) {
                        Some(x) => x,
                        None => {
                            skip += self.font_sdf.font_arc.h_advance_unscaled(id)
                                / self.font_sdf.units_per_em;
                            continue;
                        }
                    };
                    instances.push(Instance {
                        position: [skip + glyph_info.offset.x, -0.5 + glyph_info.offset.y],
                        scale: [glyph_info.dimensions.x, -glyph_info.dimensions.y],
                        fg: *fg,
                        bg: *bg,
                        lines_off: glyph_info.line_off,
                        quadratic_off: glyph_info.bez2_off,
                        cubic_off: glyph_info.bez3_off,
                    });
                    skip += glyph_info.advance;

                    for (prev_id, id) in
                        Vec::from_iter(text.chars().map(|c| self.font_sdf.font_arc.glyph_id(c)))
                            .into_iter()
                            .tuple_windows()
                    {
                        skip -= self.font_sdf.font_arc.kern_unscaled(prev_id, id);
                        let glyph_info = match self.font_sdf.load_char_with_id(id) {
                            Some(x) => {
                                self.update_font();
                                x
                            }
                            None => {
                                skip += self.font_sdf.font_arc.h_advance_unscaled(id)
                                    / self.font_sdf.units_per_em;
                                continue;
                            }
                        };
                        instances.push(Instance {
                            position: [skip + glyph_info.offset.x, -0.5 + glyph_info.offset.y],
                            scale: [glyph_info.dimensions.x, -glyph_info.dimensions.y],
                            fg: *fg,
                            bg: *bg,
                            lines_off: glyph_info.line_off,
                            quadratic_off: glyph_info.bez2_off,
                            cubic_off: glyph_info.bez3_off,
                        });
                        skip += glyph_info.advance;
                    }
                }
                Renderable::Space(space) => {
                    skip += space;
                }
                Renderable::Box {
                    fg,
                    bg,
                    width,
                    height,
                    skip: off,
                } => {
                    instances.push(Instance {
                        position: [skip, 0.],
                        scale: [*width, *height],
                        fg: *fg,
                        bg: *bg,
                        lines_off: GlyphOffLen::zeroed(),
                        quadratic_off: GlyphOffLen::zeroed(),
                        cubic_off: GlyphOffLen::zeroed(),
                    });
                    skip += off
                }
            }
        }
        (instances, skip)
    }

    fn draw_frame(&mut self, state: &RenderState) {
        let surface = &self.surface;
        let device = &self.device.clone();
        let queue = &self.queue.clone();

        // Wait for GPU to do stuff, so that get_current_texture doesn't timeout
        surface.configure(
            device,
            &surface
                .get_default_config(&self.adapter, self.width, self.height)
                .expect("To be able to get default config for the surface"),
        );

        let surface_texture = surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let (mut instances, left_skip) = self.to_renderable(&state.left, 0.0);

        let (center_instances, center_skip) = self.to_renderable(&state.center, left_skip);

        let width = center_skip - left_skip;
        let bar_width = self.width as f32 / self.height as f32;
        for instance in center_instances.into_iter() {
            instances.push(Instance {
                position: [
                    instance.position[0] - left_skip + bar_width / 2. - width / 2.,
                    instance.position[1],
                ],
                ..instance
            });
        }

        let (right_instances, right_skip) = self.to_renderable(&state.right, center_skip);

        let width = right_skip - center_skip;


        for instance in right_instances.into_iter() {
            instances.push(Instance {
                position: [
                    instance.position[0] - center_skip + bar_width - width,
                    instance.position[1],
                ],
                ..instance
            });
        }


        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(instances.as_slice()),
        );

        self.update_font();

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
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
            renderpass.set_bind_group(0, &self.pipeline_bind_group, &[]);
            renderpass.set_pipeline(&self.render_pipeline);
            renderpass.set_vertex_buffer(0, self.square_vb.slice(..));
            renderpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            renderpass.set_index_buffer(self.square_ib.slice(..), IndexFormat::Uint16);
            renderpass.draw_indexed(0..self.square_num_vertices, 0, 0..(instances.len() as u32));
        }

        // Submit the command in the queue to execute
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.queue.write_buffer(
            &self.global_transform_uniform_buffer,
            0,
            bytemuck::bytes_of(&GlobalTransformUniform {
                scale: [2.0 * self.height as f32 / self.width as f32, 1.],
                translate: [-1., 0.],
            }),
        );
        let mut config = self
            .surface
            .get_default_config(&self.adapter, self.width, self.height)
            .expect("To be able to get the default config from a surface");
        config.desired_maximum_frame_latency = 1;
        // Change this back to Mailbox
        config.present_mode = PresentMode::Fifo;
        self.surface.configure(&self.device, &config);
        self.queue.submit([]);
    }

    pub async fn run_event_loop(
        self,
        mut display_receiver: Receiver<DisplayMessage>,
        mut render_receiver: Receiver<RenderState>,
    ) {
        let renderer = Arc::new(RwLock::new(self));
        let handle = Handle::current();
        let renderer1 = Arc::clone(&renderer);
        let display_handle = handle.spawn(async move {
            while let Some(message) = display_receiver.recv().await {
                match message {
                    DisplayMessage::Configure { width, height } => {
                        renderer1.write().await.resize(width, height);
                    }
                }
            }
        });

        let render_handle = handle.spawn(async move {
            while let Some(state) = render_receiver.recv().await {
                renderer.write().await.draw_frame(&state);
            }
        });
        display_handle
            .await
            .expect("No error happending when reading display messages");
        render_handle
            .await
            .expect("No error happending when reading render messages");
    }
}
