use std::sync::Arc;

use tokio::{
    sync::mpsc::Sender,
    task::block_in_place,
};

use wayland_client::{
    Connection, Dispatch, DispatchError, EventQueue, Proxy, QueueHandle,
    backend::ObjectData,
    globals::GlobalListContents,
    globals::{GlobalList, registry_queue_init},
    protocol::{
        wl_callback::WlCallback, wl_compositor::WlCompositor, wl_output::WlOutput,
        wl_registry::WlRegistry, wl_seat::WlSeat, wl_surface::WlSurface,
    },
    protocol::{
        wl_keyboard::{self, WlKeyboard},
        wl_pointer::{self, WlPointer},
        wl_surface,
    },
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, SurfaceData},
    globals::GlobalData,
    output::{OutputData, OutputHandler, OutputState},
    reexports::{
        protocols::{
            wp::cursor_shape::v1::client::{
                wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
                wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
            },
            xdg::xdg_output::zv1::client::{
                zxdg_output_manager_v1::ZxdgOutputManagerV1, zxdg_output_v1::ZxdgOutputV1,
            },
        },
        protocols_wlr::layer_shell::v1::client::{
            zwlr_layer_shell_v1::ZwlrLayerShellV1, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        },
    },
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    seat::{
        Capability, SeatData, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardData, KeyboardHandler, Keysym, Modifiers},
        pointer::{
            PointerData, PointerEvent, PointerEventKind, PointerHandler,
            cursor_shape::CursorShapeManager,
        },
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceData,
        },
    },
};


pub enum DisplayMessage {
    CanDraw,
    Configure { width: u32, height: u32 },
}

#[derive(Debug)]
pub struct Display {
    pub wayland_conn: Connection,
    pub wayland_surface: WlSurface,
    pub globals: GlobalList,
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    pub exit: bool,
    pub width: u32,
    pub height: u32,
    pub layer: LayerSurface,
    pub keyboard: Option<WlKeyboard>,
    pub pointer: Option<WlPointer>,
    pub sender: Sender<DisplayMessage>,
}

impl Display {
    pub fn new(height: u32, sender: Sender<DisplayMessage>) -> (Self, EventQueue<Self>) {
        let wayland_conn =
            Connection::connect_to_env().expect("To be able to connect to the compositor");
        let (globals, event_queue) = registry_queue_init(&wayland_conn)
            .expect("To be able to initialize the registry queue from the compositor");
        let qh = event_queue.handle();
        let compositor =
            CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("layer shell is not available");

        let wayland_surface = compositor.create_surface(&qh);

        let compositor = CompositorState::bind(&globals, &qh)
            .expect("wl_compositor is not available, whatever that means");

        // NOTE: This surface cloning might not be fine
        let layer = layer_shell.create_layer_surface(
            &qh,
            wayland_surface.clone(),
            Layer::Top,
            Some("sway-shell"),
            None,
        );

        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);

        layer.set_anchor(Anchor::TOP.union(Anchor::LEFT).union(Anchor::RIGHT));
        layer.set_size(0, height);

        (
            Display {
                sender,
                wayland_surface,
                wayland_conn,
                compositor,
                layer_shell,
                registry_state: RegistryState::new(&globals),
                seat_state: SeatState::new(&globals, &qh),
                output_state: OutputState::new(&globals, &qh),
                exit: false,
                width: 256 * 4,
                height,
                layer,
                keyboard: None,
                pointer: None,
                globals,
            },
            event_queue,
        )
    }

    /// Actual rendering happens in CompositorHandler::frame
    pub async fn run_event_loop(
        mut self,
        mut event_queue: EventQueue<Self>,
    ) -> Result<(), EventLoopError> {
        loop {
            event_queue.blocking_dispatch(&mut self)?;
            self.layer.commit();

            if self.exit {
                log::info!("exiting example");
                break;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum EventLoopError {
    EventQueueDispathError(DispatchError),
}

impl From<DispatchError> for EventLoopError {
    fn from(value: DispatchError) -> Self {
        Self::EventQueueDispathError(value)
    }
}

impl LayerShellHandler for Display {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (new_width, new_height) = configure.new_size;
        self.width = new_width;
        self.height = new_height;
        block_in_place(|| {
            self.sender.blocking_send(DisplayMessage::Configure {
                width: self.width,
                height: self.height,
            })
        })
        .expect("To be able to send a display message when configuration is requested");
        layer.set_size(self.width, self.height);
    }
}

impl CompositorHandler for Display {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        log::info!("Wgpu::scale_factor_changed");
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _new_transform: wayland_client::protocol::wl_output::Transform,
    ) {
        log::info!("Wgpu::transform_changed");
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _time: u32,
    ) {
        log::info!("Wgpu::frame");
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _output: &wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::surface_enter");
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _output: &wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::surface_leave");
    }
}

impl OutputHandler for Display {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        let output_info = self
            .output_state
            .info(&output)
            .expect("To be able to get the info of the output from current output state");
        if let Some((width, _)) = output_info.logical_size {
            self.width = width as u32;
            self.layer.set_size(self.width, self.height);
            self.layer.set_exclusive_zone(self.height as i32);
            block_in_place(|| {
                self.sender.blocking_send(DisplayMessage::Configure {
                    width: self.width,
                    height: self.height,
                })
            })
            .expect("To be able to send a display message when new output is created");
        }
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::update_output");
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        log::info!("Wgpu::output_destroyed");
    }
}

impl SeatHandler for Display {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
        log::info!("Wgpu::new_seat");
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        log::info!("Wgpu::new_capability");

        if capability == Capability::Keyboard && self.keyboard.is_none() {
            log::info!("Set keyboard capability");
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to create keyboard");
            self.keyboard = Some(keyboard);
        }

        if capability == Capability::Pointer && self.pointer.is_none() {
            log::info!("Set pointer capability");
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
        _capability: smithay_client_toolkit::seat::Capability,
    ) {
        log::info!("Wgpu::remove_capability");
    }

    fn remove_seat(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
        log::info!("Wgpu::remove_seat");
    }
}

impl PointerHandler for Display {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            // Ignore events for other surfaces
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                Enter { .. } => {
                    log::info!("Pointer entered @{:?}", event.position);
                }
                Leave { .. } => {
                    log::info!("Pointer left");
                }
                Motion { .. } => {}
                Press { button, .. } => {
                    log::info!("Press {:x} @ {:?}", button, event.position);
                    self.layer
                        .wl_surface()
                        .frame(qh, self.layer.wl_surface().clone());
                }
                Release { button, .. } => {
                    log::info!("Release {:x} @ {:?}", button, event.position);
                }
                Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    log::info!("Scroll H:{horizontal:?}, V:{vertical:?}");
                }
            }
        }
    }
}

impl KeyboardHandler for Display {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        keysyms: &[Keysym],
    ) {
        if self.layer.wl_surface() == surface {
            log::info!("Keyboard focus on window with pressed syms: {keysyms:?}");
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.layer.wl_surface() == surface {
            log::info!("Release keyboard focus on window");
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        log::info!("Key press: {event:?}");
        // press 'esc' to exit
        if event.keysym == Keysym::Escape {
            self.exit = true;
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        log::info!("Key release: {event:?}");
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _layout: u32,
    ) {
        log::info!("Update modifiers: {modifiers:?}");
    }
}

// All the dispatch handler macros, inlined
impl Dispatch<WlCompositor, GlobalData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlCompositor,
        event: <WlCompositor as Proxy>::Event,
        data: &GlobalData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <CompositorState as Dispatch<WlCompositor, GlobalData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <CompositorState as Dispatch<WlCompositor, GlobalData, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}
impl Dispatch<WlCallback, WlSurface> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlCallback,
        event: <WlCallback as Proxy>::Event,
        data: &WlSurface,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <CompositorState as Dispatch<WlCallback, WlSurface, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <CompositorState as Dispatch<WlCallback, WlSurface, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}
impl Dispatch<WlSurface, SurfaceData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlSurface,
        event: <WlSurface as Proxy>::Event,
        data: &SurfaceData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <CompositorState as Dispatch<WlSurface, SurfaceData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <CompositorState as Dispatch<WlSurface, SurfaceData, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}
impl Dispatch<WlOutput, OutputData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        data: &OutputData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <OutputState as Dispatch<WlOutput, OutputData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <OutputState as Dispatch<WlOutput, OutputData, Self>>::event_created_child(opcode, qhandle)
    }
}
impl Dispatch<ZxdgOutputManagerV1, GlobalData> for Display {
    fn event(
        state: &mut Self,
        proxy: &ZxdgOutputManagerV1,
        event: <ZxdgOutputManagerV1 as Proxy>::Event,
        data: &GlobalData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <OutputState as Dispatch<ZxdgOutputManagerV1, GlobalData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <OutputState as Dispatch<ZxdgOutputManagerV1, GlobalData, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}
impl Dispatch<ZxdgOutputV1, OutputData> for Display {
    fn event(
        state: &mut Self,
        proxy: &ZxdgOutputV1,
        event: <ZxdgOutputV1 as Proxy>::Event,
        data: &OutputData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <OutputState as Dispatch<ZxdgOutputV1, OutputData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <OutputState as Dispatch<ZxdgOutputV1, OutputData, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}

impl Dispatch<WlSeat, SeatData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlSeat,
        event: <WlSeat as Proxy>::Event,
        data: &SeatData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <SeatState as Dispatch<WlSeat, SeatData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <SeatState as Dispatch<WlSeat, SeatData, Self>>::event_created_child(opcode, qhandle)
    }
}

impl Dispatch<WlKeyboard, KeyboardData<Display>> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlKeyboard,
        event: <WlKeyboard as Proxy>::Event,
        data: &KeyboardData<Display>,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <SeatState as Dispatch<WlKeyboard, KeyboardData<Display>, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <SeatState as Dispatch<WlKeyboard, KeyboardData<Display>, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}
impl Dispatch<WpCursorShapeManagerV1, GlobalData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WpCursorShapeManagerV1,
        event: <WpCursorShapeManagerV1 as Proxy>::Event,
        data: &GlobalData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <CursorShapeManager as Dispatch<WpCursorShapeManagerV1, GlobalData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <CursorShapeManager as Dispatch<WpCursorShapeManagerV1,GlobalData,Self>>::event_created_child(opcode,qhandle)
    }
}
impl Dispatch<WpCursorShapeDeviceV1, GlobalData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WpCursorShapeDeviceV1,
        event: <WpCursorShapeDeviceV1 as Proxy>::Event,
        data: &GlobalData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <CursorShapeManager as Dispatch<WpCursorShapeDeviceV1, GlobalData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <CursorShapeManager as Dispatch<WpCursorShapeDeviceV1,GlobalData,Self>>::event_created_child(opcode,qhandle)
    }
}
impl Dispatch<WlPointer, PointerData> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlPointer,
        event: <WlPointer as Proxy>::Event,
        data: &PointerData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <SeatState as Dispatch<WlPointer, PointerData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <SeatState as Dispatch<WlPointer, PointerData, Self>>::event_created_child(opcode, qhandle)
    }
}

impl Dispatch<ZwlrLayerShellV1, GlobalData> for Display {
    fn event(
        state: &mut Self,
        proxy: &ZwlrLayerShellV1,
        event: <ZwlrLayerShellV1 as Proxy>::Event,
        data: &GlobalData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <LayerShell as Dispatch<ZwlrLayerShellV1, GlobalData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <LayerShell as Dispatch<ZwlrLayerShellV1, GlobalData, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}
impl Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData> for Display {
    fn event(
        state: &mut Self,
        proxy: &ZwlrLayerSurfaceV1,
        event: <ZwlrLayerSurfaceV1 as Proxy>::Event,
        data: &LayerSurfaceData,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <LayerShell as Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <LayerShell as Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Display {
    fn event(
        state: &mut Self,
        proxy: &WlRegistry,
        event: <WlRegistry as Proxy>::Event,
        data: &GlobalListContents,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        <RegistryState as Dispatch<WlRegistry, GlobalListContents, Self>>::event(
            state, proxy, event, data, conn, qhandle,
        )
    }
    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        <RegistryState as Dispatch<WlRegistry, GlobalListContents, Self>>::event_created_child(
            opcode, qhandle,
        )
    }
}

impl ProvidesRegistryState for Display {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    fn runtime_add_global(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        name: u32,
        interface: &str,
        version: u32,
    ) {
        <OutputState as RegistryHandler<Self>>::new_global(
            self, conn, qh, name, interface, version,
        );
        <SeatState as RegistryHandler<Self>>::new_global(self, conn, qh, name, interface, version);
    }

    fn runtime_remove_global(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        name: u32,
        interface: &str,
    ) {
        <OutputState as RegistryHandler<Self>>::remove_global(self, conn, qh, name, interface);
        <SeatState as RegistryHandler<Self>>::remove_global(self, conn, qh, name, interface);
    }
}
