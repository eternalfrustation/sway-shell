use std::time::Duration;

use mpd::Status;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;

use crate::{
    font::{Line, Vector},
    sway::Workspace,
};

#[derive(Debug, Clone)]
pub struct State {
    pub workspaces: Vec<Workspace>,
    pub mpd_status: Option<Status>,
    pub mpd_current_song: Option<mpd::Song>,
    pub press_position: Vector,
    pub lines: Vec<Line>,
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
    MpdPlayerUpdate {
        status: mpd::Status,
    },
    MpdSongUpdate {
        song: Option<mpd::Song>,
    },
    MpdTimeElapsed {
        elapsed: Duration,
    },
    PointerPress {
        pos: Vector,
    },
    PointerRelease {
        pos: Vector,
    },
}

impl State {
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            mpd_status: None,
            mpd_current_song: None,
            press_position: Vector { x: 0., y: 0. },
            lines: vec![],
        }
    }

    pub async fn run_event_loop<S: StreamExt<Item = Message> + std::marker::Unpin>(
        mut self,
        mut message_receiver: S,
        render_sender: Sender<Self>,
    ) {
        render_sender
            .send(self.clone())
            .await
            .expect("To be able to send render requests without drama, when initializing");
        while let Some(message) = message_receiver.next().await {
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
            Message::WorkspaceAdd(workspace) => {
                self.workspaces.push(workspace);
                self.workspaces.sort_by_key(|v| v.num);
            }
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
            Message::MpdPlayerUpdate { status } => {
                self.mpd_status = Some(status);
            }
            Message::MpdTimeElapsed { elapsed } => {
                if let Some(ref mut mpd_stats) = self.mpd_status {
                    mpd_stats.elapsed = Some(elapsed);
                }
            }
            Message::MpdSongUpdate { song } => {
                self.mpd_current_song = song;
            }
            Message::PointerPress { pos } => self.press_position = pos,
            Message::PointerRelease { pos } => self.lines.push(Line(self.press_position, pos)),
        }
    }
}
