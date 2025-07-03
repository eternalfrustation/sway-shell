pub mod layer;
pub mod state;
pub mod sway;

use layer::Wgpu;


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
