use mpd::Status;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::block_in_place;
use wgpu::IndexFormat;
use wgpu::util::DeviceExt;

use crate::layer::DisplayMessage;
use crate::sway::Workspace;

#[derive(Debug, Clone)]
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
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            mpd_status: None,
        }
    }

    pub async fn run_event_loop(
        mut self,
        mut message_receiver: Receiver<Message>,
        render_sender: Sender<Self>,
    ) {
        render_sender
            .send(self.clone())
            .await
            .expect("To be able to send render requests without drama, when initializing");
        while let Some(message) = message_receiver.recv().await {
            self.update(message);
            render_sender
                .send(self.clone())
                .await
                .expect("To be able to send render requests without drama");
        }
    }

    fn update(&mut self, message: Message) {
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
