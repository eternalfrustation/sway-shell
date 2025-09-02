use mpd::Status;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;

use crate::{
    font::{Line, Segment, Vector},
    mpd::MpdMessage,
    network::{Network, NetworkMessage},
    sway::{SwayMessage, Workspace},
};

#[derive(Debug, Clone)]
pub struct State {
    pub workspaces: Vec<Workspace>,
    pub mpd_status: Option<Status>,
    pub mpd_current_song: Option<mpd::Song>,
    pub press_position: Vector,
    pub segments: Vec<Segment>,
    pub networks: Vec<Network>,
}

#[derive(Debug)]
pub enum Message {
    Sway(SwayMessage),
    Mpd(MpdMessage),
    Network(NetworkMessage),
    PointerPress { pos: Vector },
    PointerRelease { pos: Vector },
}

impl State {
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            mpd_status: None,
            mpd_current_song: None,
            press_position: Vector { x: 0., y: 0. },
            segments: vec![],
            networks: vec![],
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
            self.update(message);
            render_sender
                .send(self.clone())
                .await
                .expect("To be able to send render requests without drama");
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Sway(sway_message) => match sway_message {
                SwayMessage::WorkspaceAdd(workspace) => {
                    self.workspaces.push(workspace);
                    self.workspaces.sort_by_key(|v| v.num);
                }
                SwayMessage::WorkspaceDel(id) => {
                    self.workspaces = self
                        .workspaces
                        .clone()
                        .into_iter()
                        .filter(|v| v.id != id)
                        .collect()
                }
                SwayMessage::WorkspaceChangeFocus { id, focus, focused } => {
                    if let Some(workspace) =
                        &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                    {
                        workspace.focus = focus;
                        workspace.focused = focused;
                    } else {
                        log::error!("Couldn't find the workspace when changing focus");
                    }
                }
                SwayMessage::WorkspaceRename { id, name } => {
                    if let Some(workspace) =
                        &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                    {
                        workspace.name = name;
                    }
                }
                SwayMessage::WorkspaceChangeUrgency { id, urgent } => {
                    if let Some(workspace) =
                        &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                    {
                        workspace.urgent = urgent;
                    }
                }
                SwayMessage::WorkspaceChangeVisiblity { id, visible } => {
                    if let Some(workspace) =
                        &mut self.workspaces.iter_mut().filter(|v| v.id == id).next()
                    {
                        workspace.visible = visible;
                    }
                }
            },
            Message::Mpd(mpd_message) => match mpd_message {
                MpdMessage::MpdPlayerUpdate { status } => {
                    self.mpd_status = Some(status);
                }
                MpdMessage::MpdTimeElapsed { elapsed } => {
                    if let Some(ref mut mpd_stats) = self.mpd_status {
                        mpd_stats.elapsed = Some(elapsed);
                    }
                }
                MpdMessage::MpdSongUpdate { song } => {
                    self.mpd_current_song = song;
                }
            },
            Message::PointerPress { pos } => self.press_position = pos,
            Message::PointerRelease { pos } => {
                self.segments
                    .push(Segment::LINE(Line(self.press_position, pos)));
            }
            Message::Network(network_message) => self.networks = network_message,
        }
    }
}
