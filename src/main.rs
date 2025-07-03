pub mod layer;
pub mod state;
pub mod sway;

use layer::Wgpu;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::ptr::NonNull;
use wayland_client::{Connection, Proxy, globals::registry_queue_init};

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

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let (conn, globals, mut event_queue) = Wgpu::connect();
    let mut wgpu = Wgpu::new(conn, globals, &mut event_queue).await;
    // We don't draw immediately, the configure will notify us when to first draw.
    loop {
        event_queue.blocking_dispatch(&mut wgpu).unwrap();

        if wgpu.exit {
            println!("exiting example");
            break;
        }
    }

    // On exit we must destroy the surface before the window is destroyed.
    drop(wgpu.surface);
    drop(wgpu.layer);
}
