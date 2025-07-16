#![feature(sort_floats)]
#![feature(iter_array_chunks)]

pub mod layer;
pub mod renderer;
pub mod state;
pub mod sway;
pub mod font;

use layer::Display;
use renderer::Renderer;
use std::sync::Arc;
use tokio::sync::mpsc::channel;

use tokio::runtime::Runtime;

use state::State;
use sway::sway_subscription;

fn main() {
    pretty_env_logger::init();
    let rt = Arc::new(Runtime::new().expect("To be able to initalize a tokio runtime"));

    let state = State::new();
    let sway_stream = sway_subscription(rt.clone());
    let (render_sender, render_receiver) = channel(1);
    let (display_sender, display_receiver) = channel(1);
    let state_event_loop_handle = rt.spawn(state.run_event_loop(sway_stream, render_sender));
    // IDK how else to do this
    const HEIGHT: u32 = 20;
    let (display, event_queue) = Display::new(HEIGHT, display_sender);
    let wayland_conn = display.wayland_conn.clone();
    let wayland_surface = display.wayland_surface.clone();

    let renderer_event_loop_handle = rt.spawn(async move {
        let renderer = Renderer::new(&wayland_conn, &wayland_surface, 100, HEIGHT).await;
        renderer
            .run_event_loop(display_receiver, render_receiver)
            .await;
    });

    let display_event_loop_handle = rt.spawn(async move {
        display
            .run_event_loop(event_queue)
            .await
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
