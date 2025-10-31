#![feature(sort_floats)]
#![feature(iter_array_chunks)]

pub mod font;
pub mod layer;
pub mod mpd;
pub mod renderer;
pub mod state;
pub mod sway;
pub mod network;
pub mod netlink;
pub mod audio;

use layer::Display;
use mpd::mpd_subscription;
use renderer::Renderer;
use std::sync::Arc;
use tokio::sync::mpsc::channel;

use tokio::runtime::Runtime;
use tokio_stream::{StreamExt, StreamMap};

use state::State;
use sway::sway_subscription;

use crate::{audio::audio_subscription, network::network_subscription};

fn main() {
    pretty_env_logger::init();
    let rt = Arc::new(Runtime::new().expect("To be able to initalize a tokio runtime"));

    let mut streams = StreamMap::new();

    let state = State::new();
    let (render_sender, render_receiver) = channel(1);
    let (state_sender, state_receiver) = channel(1);
    let state_stream = tokio_stream::wrappers::ReceiverStream::new(state_receiver);
    streams.insert("sway", sway_subscription(rt.handle().clone()));
    streams.insert("mpd", mpd_subscription(rt.handle().clone()));
    streams.insert("network", network_subscription(rt.handle().clone()));
    streams.insert("audio", audio_subscription(rt.handle().clone()));
    streams.insert("display", state_stream);
    let (display_sender, display_receiver) = channel(1);
    // Currently using the merge method, ideally would use a StreamMap
    let state_event_loop_handle =
        rt.spawn(state.run_event_loop(streams.map(|(_, v)| v), render_sender));
    // IDK how else to do this
    const HEIGHT: u32 = 20;
    let (display, event_queue) = rt.block_on(Display::new(HEIGHT, display_sender, state_sender));
    let wayland_conn = display.wayland_conn.clone();
    let wayland_surface = display.wayland_surface.clone();

    let renderer_event_loop_handle = rt.spawn(async move {
        let renderer = Renderer::new(&wayland_conn, &wayland_surface, 100, HEIGHT).await;
        renderer
            .run_event_loop(display_receiver, render_receiver)
            .await;
    });

    let display_event_loop_handle = rt.spawn_blocking(|| {
        display
            .run_event_loop(event_queue)
            .expect("To never exit the event loop");
    });

    rt.block_on(async {
        state_event_loop_handle
            .await
            .expect("Never erroring out in the state event loop");
        renderer_event_loop_handle
            .await
            .expect("Never erroring out in the renderer event loop");
        display_event_loop_handle
            .await
            .expect("Never erroring out in the display event loop");
    });
}
