use std::sync::Arc;

use wayland_client::Proxy;

use futures::StreamExt;
use mpd::Status;
use wgpu::IndexFormat;

use crate::{layer::Renderer, sway::Workspace, viewable::Viewable};

#[derive(Debug, Default)]
pub struct State {
    pub workspaces: Vec<Workspace>,
    pub mpd_status: Option<Status>,
}

#[derive(Debug)]
pub enum Message {
    WorkspaceAdd(Workspace),
    WorkspaceDel(i64),
    WorkspaceChangeFocus {
        id: i64,
        focus: Vec<i64>,
        focused: bool,
    },
    WorkspaceRename {
        id: i64,
        name: Option<String>,
    },
    WorkspaceChangeUrgency {
        id: i64,
        urgent: bool,
    },
}

impl State {
    fn update(&mut self, message: Message) {
        log::info!("{message:?}");
        match message {
            Message::WorkspaceAdd(workspace) => self.workspaces.push(workspace),
            Message::WorkspaceDel(id) => {
                self.workspaces = self
                    .workspaces
                    .clone()
                    .into_iter()
                    .filter(|v| v.id != id)
                    .collect()
            }
            Message::WorkspaceChangeFocus { id, focus, focused } => {
                if let Some(workspace) =
                    &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                {
                    workspace.focus = focus;
                    workspace.focused = focused;
                }
            }
            Message::WorkspaceRename { id, name } => {
                if let Some(workspace) =
                    &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                {
                    workspace.name = name;
                }
            }
            Message::WorkspaceChangeUrgency { id, urgent } => {
                if let Some(workspace) =
                    &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                {
                    workspace.urgent = urgent;
                }
            }
        }
    }
}

impl Viewable<Renderer<Self>> for State {
    fn draw_frame(self: Arc<Self>, renderer: &mut Renderer<Self>) {
        let adapter = &renderer.adapter;
        let surface = &renderer.surface;
        let device = &renderer.device;
        let queue = &renderer.queue;
        // We don't plan to render much in this example, just clear the surface.
        let surface_texture = surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

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
            renderpass.set_pipeline(&renderer.render_pipeline);
            renderpass.set_vertex_buffer(0, renderer.square_vb.slice(..));
            renderpass.set_index_buffer(renderer.square_ib.slice(..), IndexFormat::Uint16);
            renderpass.draw_indexed(0..renderer.square_num_vertices, 0, 0..1);
        }

        // Submit the command in the queue to execute
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }
}
