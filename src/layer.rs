use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::ptr::NonNull;
use wayland_client::{
    Connection, EventQueue, Proxy,
    globals::{GlobalList, registry_queue_init},
};

use futures::{
    SinkExt, Stream, StreamExt,
    channel::mpsc::{self, SendError, Sender},
    stream,
};
use mpd::Status;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{SeatHandler, SeatState},
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
        },
    },
};
use swayipc::{Event, EventType, Node, Rect, WorkspaceChange};
use tokio::runtime::Runtime;

#[derive(Debug)]
pub struct Wgpu {
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    pub exit: bool,
    pub width: u32,
    pub height: u32,
    pub layer: LayerSurface,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
}

impl Wgpu {
    pub fn connect() -> (Connection, GlobalList, EventQueue<Wgpu>) {
        let wayland_conn =
            Connection::connect_to_env().expect("To be able to connect to the compositor");
        let (globals, event_queue) = registry_queue_init(&wayland_conn)
            .expect("To be able to initialize the registry queue from the compositor");
        (wayland_conn, globals, event_queue)
    }
    pub async fn new(
        wayland_conn: Connection,
        globals: GlobalList,
        event_queue: &mut EventQueue<Wgpu>,
    ) -> Self {
        let qh = event_queue.handle();
        let compositor =
            CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("layer shell is not available");

        let wayland_surface = compositor.create_surface(&qh);

        let compositor = CompositorState::bind(&globals, &qh)
            .expect("wl_compositor is not available, whatever that means");
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        // Create the raw window handle for the surface.
        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(wayland_conn.backend().display_ptr() as *mut _)
                .expect("Wayland display pointer to be not null"),
        ));

        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(wayland_surface.id().as_ptr() as *mut _)
                .expect("Wayland surface pointer to be not null"),
        ));

        let layer = layer_shell.create_layer_surface(
            &qh,
            wayland_surface,
            Layer::Top,
            Some("sway-shell"),
            None,
        );

        layer.set_anchor(Anchor::TOP);
        let surface = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle,
                    raw_window_handle,
                })
                .unwrap()
        };

        // Pick a supported adapter
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

        Wgpu {
            compositor,
            layer_shell,
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),

            exit: false,
            width: 256 * 4,
            height: 256,
            layer,
            device,
            surface,
            adapter,
            queue,
        }
    }
}

impl LayerShellHandler for Wgpu {
    fn closed(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        layer: &LayerSurface,
    ) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        layer: &LayerSurface,
        configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        serial: u32,
    ) {
        let (new_width, new_height) = configure.new_size;
        self.width = new_width;
        self.height = new_height;
        layer.set_size(self.width, self.height);

        let adapter = &self.adapter;
        let surface = &self.surface;
        let device = &self.device;
        let queue = &self.queue;

        let cap = surface.get_capabilities(&adapter);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: cap.formats[0],
            view_formats: vec![cap.formats[0]],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.width,
            height: self.height,
            desired_maximum_frame_latency: 2,
            // Wayland is inherently a mailbox system.
            present_mode: wgpu::PresentMode::Mailbox,
        };

        surface.configure(&self.device, &surface_config);

        // We don't plan to render much in this example, just clear the surface.
        let surface_texture = surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Submit the command in the queue to execute
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
        layer.commit();
    }
}

impl CompositorHandler for Wgpu {
    fn scale_factor_changed(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        new_factor: i32,
    ) {
        log::info!("Wgpu::scale_factor_changed");
    }

    fn transform_changed(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        new_transform: wayland_client::protocol::wl_output::Transform,
    ) {
        log::info!("Wgpu::transform_changed");
    }

    fn frame(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        time: u32,
    ) {
        log::info!("Wgpu::frame");

        let adapter = &self.adapter;
        let surface = &self.surface;
        let device = &self.device;
        let queue = &self.queue;

        let cap = surface.get_capabilities(&adapter);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: cap.formats[0],
            view_formats: vec![cap.formats[0]],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.width,
            height: self.height,
            desired_maximum_frame_latency: 2,
            // Wayland is inherently a mailbox system.
            present_mode: wgpu::PresentMode::Mailbox,
        };

        surface.configure(&self.device, &surface_config);

        // We don't plan to render much in this example, just clear the surface.
        let surface_texture = surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Submit the command in the queue to execute
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
        self.layer.commit();
    }

    fn surface_enter(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        output: &wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::surface_enter");
    }

    fn surface_leave(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        output: &wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::surface_leave");
    }
}

impl OutputHandler for Wgpu {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        let output_info = self
            .output_state
            .info(&output)
            .expect("To be able to get the info of the output from current output state");
        if let Some((width, height)) = output_info.logical_size {
            self.width = width as u32;
            self.height = 50;
            self.layer.set_size(self.width, self.height);
            self.layer.set_exclusive_zone(self.height as i32);
        }

        self.layer.commit();
    }

    fn update_output(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::update_output");
    }

    fn output_destroyed(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::output_destroyed");
    }
}

impl SeatHandler for Wgpu {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
        log::info!("Wgpu::new_seat");
    }

    fn new_capability(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        log::info!("Wgpu::new_capability");
    }

    fn remove_capability(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        log::info!("Wgpu::remove_capability");
    }

    fn remove_seat(
        &mut self,
        conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
        log::info!("Wgpu::remove_seat");
    }
}

delegate_compositor!(Wgpu);
delegate_output!(Wgpu);

delegate_seat!(Wgpu);
delegate_layer!(Wgpu);

delegate_registry!(Wgpu);

impl ProvidesRegistryState for Wgpu {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

