use std::{
    borrow::Cow,
    ptr::NonNull,
    sync::Arc,
};

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use tokio::{
    runtime::Handle,
    sync::{
        RwLock,
        mpsc::{Receiver, channel},
    },
};
use wayland_client::{Proxy, protocol::wl_surface::WlSurface};
use wgpu::{
    Buffer, IndexFormat, PresentMode, RenderPipeline, util::DeviceExt,
};

use crate::{layer::DisplayMessage, state::State};

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
}

impl Instance {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
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
                    // While our vertex shader only uses locations 0, and 1 now, in later tutorials, we'll
                    // be using 2, 3, and 4, for Vertex. We'll start at slot 5, not conflict with them later
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[derive(Debug)]
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
    pub global_transform_bind_group: wgpu::BindGroup,
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

        let (device, queue) = adapter
            .request_device(&Default::default())
            .await
            .expect("Failed to request device");

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
        let global_transform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("camera_bind_group_layout"),
            });

        let global_transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &global_transform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: global_transform_uniform_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&global_transform_layout],
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
        Self {
            width,
            height,
            adapter,
            device,
            queue,
            surface,
            render_pipeline,
            square_vb,
            square_ib,
            square_num_vertices: SQUARE_INDICES.len() as u32,
            global_transform_uniform_buffer,
            global_transform_bind_group,
        }
    }

    fn draw_frame(&self, state: &State) {
        if state.workspaces.len() == 0 {
            return;
        }
        let surface = &self.surface;
        let device = &self.device;
        let queue = &self.queue;
        let surface_texture = surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let instance_data = state
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, _w)| Instance {
                position: [i as f32 * 2., 0.],
                scale: [1., 1.],
            })
            .collect::<Vec<Instance>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(instance_data.as_slice()),
            usage: wgpu::BufferUsages::VERTEX,
        });

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
            renderpass.set_bind_group(0, &self.global_transform_bind_group, &[]);
            renderpass.set_pipeline(&self.render_pipeline);
            renderpass.set_vertex_buffer(0, self.square_vb.slice(..));
            renderpass.set_vertex_buffer(1, instance_buffer.slice(..));
            renderpass.set_index_buffer(self.square_ib.slice(..), IndexFormat::Uint16);
            renderpass.draw_indexed(
                0..self.square_num_vertices,
                0,
                0..(instance_data.len() as u32),
            );
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
        config.present_mode = PresentMode::Mailbox;
        self.surface.configure(&self.device, &config);
        self.queue.submit([]);
    }

    pub async fn run_event_loop(
        self,
        mut display_receiver: Receiver<DisplayMessage>,
        mut render_receiver: Receiver<State>,
    ) {
        let renderer = Arc::new(RwLock::new(self));
        let handle = Handle::current();
        let (sender, mut _receiver) = channel(1);
        let renderer1 = Arc::clone(&renderer);
        let display_handle = handle.spawn(async move {
            while let Some(message) = display_receiver.recv().await {
                match message {
                    DisplayMessage::CanDraw => {
                        sender
                            .send(())
                            .await
                            .expect("To be able to send a message that we can draw");
                    }
                    DisplayMessage::Configure { width, height } => {
                        renderer1.write().await.resize(width, height);
                        sender
                            .send(())
                            .await
                            .expect("To be able to send a message that we can draw");
                    }
                }
            }
        });

        let render_handle = handle.spawn(async move {
            while let Some(state) = render_receiver.recv().await {
                log::info!("Received signal that drawing is requested");
                log::info!("Ignoring signal that we can draw");
                renderer.read().await.draw_frame(&state);
                log::info!("Drew the frame");
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
