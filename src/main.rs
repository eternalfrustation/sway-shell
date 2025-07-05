pub mod layer;
pub mod viewable;
pub mod state;
pub mod sway;


use layer::Renderer;
use state::State;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let state = State::default();
    let (renderer, event_queue) = Renderer::new(state.into()).await;
    renderer
        .start_event_loop(event_queue)
        .expect("For there to be no problem when running the event loop");
}
