pub mod layer;
pub mod state;
pub mod sway;

use layer::Wgpu;
use smithay_client_toolkit::{globals::ProvidesBoundGlobal, shell::WaylandSurface};

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let (conn, globals, mut event_queue) = Wgpu::connect();
    let mut wgpu = Wgpu::new(conn, globals, &mut event_queue).await;

    loop {
        event_queue.blocking_dispatch(&mut wgpu).unwrap();

        wgpu.layer.commit();

        if wgpu.exit {
            log::info!("exiting example");
            break;
        }
    }

    // On exit we must destroy the surface before the window is destroyed.
    drop(wgpu.surface);
    drop(wgpu.layer);
}
