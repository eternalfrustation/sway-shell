use wayland_client::Proxy;

use futures::StreamExt;
use mpd::Status;

use crate::{sway::Workspace, viewable::Viewable};

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

impl Viewable for State {

}
