use mpd::Status;
use tokio::sync::mpsc::{Receiver, Sender};

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
    WorkspaceChangeVisiblity {
        id: i64,
        visible: bool,
    },
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
            log::info!("{message:?}");
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
                } else {
                    log::error!("Couldn't find the workspace when changing focus");
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
            Message::WorkspaceChangeVisiblity { id, visible } => {
                if let Some(workspace) =
                    &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                {
                    workspace.visible = visible;
                }
            }
        }
    }
}
