
use tokio::{
    runtime::Handle,
    sync::mpsc::Sender,
    task::{JoinError, block_in_place},
};

use wayland_client::{
    Connection, DispatchError, EventQueue, QueueHandle,
    globals::{GlobalList, registry_queue_init},
    protocol::wl_surface::WlSurface,
    protocol::{
        wl_keyboard::{self, WlKeyboard},
        wl_pointer::{self, WlPointer},
        wl_surface,
    },
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        },
    },
};

use crate::{font::Vector, state::Message};

pub enum DisplayMessage {
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
    pub display_sender: Sender<DisplayMessage>,
    pub state_sender: Sender<Message>,
}

impl Display {
    pub async fn new(
        height: u32,
        display_sender: Sender<DisplayMessage>,
        state_sender: Sender<Message>,
    ) -> (Self, EventQueue<Self>) {
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
        /*display_sender
                    .send(DisplayMessage::Configure { width: 100, height })
                    .await
                    .expect("To be able to send message for configuring rendering");
        */
        (
            Display {
                display_sender,
                state_sender,
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
    pub fn run_event_loop(
        mut self,
        mut event_queue: EventQueue<Self>,
    ) -> Result<(), EventLoopError> {
        log::info!("Starting poll for events");
        loop {
            self.layer.commit();
            event_queue.blocking_dispatch(&mut self)?;
            /*
                        poll_fn(|cx| {
                            log::info!("Checking for polling");
                            event_queue.poll_dispatch_pending(cx, &mut self)
                        })
                        .await?;
            */

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
    TokioError(JoinError),
}

impl From<DispatchError> for EventLoopError {
    fn from(value: DispatchError) -> Self {
        Self::EventQueueDispathError(value)
    }
}

impl From<JoinError> for EventLoopError {
    fn from(value: JoinError) -> Self {
        Self::TokioError(value)
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
        let display_sender = self.display_sender.clone();
        Handle::current().spawn(async move {
            display_sender
                .send(DisplayMessage::Configure {
                    width: new_width,
                    height: new_height,
                })
                .await
        });
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
        if let Some((width, height)) = output_info.logical_size {
            self.width = width as u32;
            self.layer.set_size(self.width, self.height);
            self.layer.set_exclusive_zone(self.height as i32);
            let display_sender = self.display_sender.clone();
            Handle::current().spawn(async move {
                log::info!("New Output message being sent");
                display_sender
                    .send(DisplayMessage::Configure {
                        width: width as u32,
                        height: height as u32,
                    })
                    .await
            });
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
            dbg!(&self.pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_some() {
            println!("Unset keyboard capability");
            self.keyboard.take().unwrap().release();
        }

        if capability == Capability::Pointer && self.pointer.is_some() {
            println!("Unset pointer capability");
            self.pointer.take().unwrap().release();
        }
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
        _qh: &QueueHandle<Self>,
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
                    block_in_place(|| {
                        self.state_sender.blocking_send(Message::PointerPress {
                            pos: Vector {
                                x: event.position.0 as f32,
                                y: event.position.1 as f32,
                            },
                        })
                    })
                    .expect("To be able to send a state message when mouse is clicked");
                }
                Release { button, .. } => {
                    log::info!("Release {:x} @ {:?}", button, event.position);
                    block_in_place(|| {
                        self.state_sender.blocking_send(Message::PointerRelease {
                            pos: Vector {
                                x: event.position.0 as f32,
                                y: event.position.1 as f32,
                            },
                        })
                    })
                    .expect("To be able to send a state message when mouse is released");
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

delegate_compositor!(Display);
delegate_output!(Display);

delegate_seat!(Display);
delegate_keyboard!(Display);
delegate_pointer!(Display);

delegate_layer!(Display);

delegate_registry!(Display);

impl ProvidesRegistryState for Display {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
