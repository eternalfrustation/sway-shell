use mpd::Status;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;

use crate::{
    audio::{AudioMessage, AudioState},
    font::{Line, Segment, Vec2},
    mpd::MpdMessage,
    network::{Network, NetworkMessage},
    renderer::{RenderState, Renderable},
    sway::{SwayMessage, Workspace},
};

#[derive(Debug, Clone)]
pub struct State {
    pub workspaces: Vec<Workspace>,
    pub mpd_status: Option<Status>,
    pub mpd_current_song: Option<mpd::Song>,
    pub press_position: Vec2,
    pub segments: Vec<Segment>,
    pub networks: Vec<Network>,
    pub audio_state: AudioState,
    pub focused_window_name: Option<String>,
}

#[derive(Debug)]
pub enum Message {
    Sway(SwayMessage),
    Mpd(MpdMessage),
    Network(NetworkMessage),
    Audio(AudioMessage),
    PointerPress { pos: Vec2 },
    PointerRelease { pos: Vec2 },
}

impl State {
    pub fn new() -> Self {
        Self {
            focused_window_name: None,
            workspaces: Vec::new(),
            mpd_status: None,
            mpd_current_song: None,
            press_position: Vec2 { x: 0., y: 0. },
            segments: vec![],
            networks: vec![],
            audio_state: AudioState::default(),
        }
    }

    pub fn to_renderable_state(&self) -> RenderState {
        let mut left = Vec::new();
        for workspace in self.workspaces.iter() {
            if let Some(name) = &workspace.name {
                left.push(Renderable::Text {
                    text: name.to_string(),
                    fg: if workspace.visible {
                        0xffFFffFF
                    } else {
                        0xff111111
                    },
                    bg: if workspace.visible {
                        0xff111111
                    } else {
                        0xff000000
                    },
                })
            } else {
                left.push(Renderable::Text {
                    text: workspace.num.to_string(),
                    fg: 0xffFFffFF,
                    bg: 0,
                });
            }
            left.push(Renderable::Space(1.))
        }
        left.push(Renderable::Space(1.));
        if let Some(mpd_status) = &self.mpd_status {
            if let Some((elapsed, total)) = mpd_status.time {
                let completed = elapsed.as_secs_f32() / total.as_secs_f32();
                left.push(Renderable::Box {
                    fg: 0xff00ffff,
                    bg: 0xff00ffff,
                    width: 10.,
                    height: 10.,
                    skip: 0.,
                });
                left.push(if mpd_status.state == mpd::status::State::Play {
                    Renderable::Box {
                        fg: 0xffff00ff,
                        bg: 0xffff00ff,
                        width: 10. * completed,
                        height: 10.,
                        skip: 10.,
                    }
                } else {
                    Renderable::Box {
                        fg: 0xffffffff,
                        bg: 0xffffffff,
                        width: 10. * completed,
                        height: 10.,
                        skip: 10.,
                    }
                });
            }
        }

        left.push(Renderable::Space(1.));

        if let Some(song) = &self.mpd_current_song {
            if let Some(name) = &song.title {
                let mut trunc_name = name.clone();
                trunc_name.truncate(trunc_name.floor_char_boundary(30));
                if name.len() > 30 {
                    trunc_name = trunc_name + "...";
                }
                left.push(Renderable::Text {
                    text: trunc_name,
                    fg: 0xffffffff,
                    bg: 0x00000000,
                })
            }
        }

        let mut center = Vec::new();
        if let Some(window_name) = &self.focused_window_name {
            let mut trunc_name = window_name.clone();
            trunc_name.truncate(trunc_name.floor_char_boundary(30));
            if window_name.len() > 30 {
                trunc_name = trunc_name + "...";
            }
            center.push(Renderable::Text {
                text: trunc_name,
                fg: 0xffffffff,
                bg: 0x00000000,
            })
        }

        let mut right = Vec::new();

        for network in self.networks.iter() {
            match network {
                Network::Wifi {
                    if_index: _,
                    if_name: _,
                    ssid,
                    up: _,
                    down: _,
                    up_rate,
                    down_rate,
                } => {
                    right.push(Renderable::Text {
                        text: format!(
                            "{} {}↓ {}↑",
                            if let Some(ssid) = ssid { ssid } else { "" }.to_string(),
                            display_bytes(*up_rate) + "/s",
                            display_bytes(*down_rate) + "/s",
                        ),
                        fg: 0xffffffff,
                        bg: 0x00000000,
                    });
                }
                Network::Network {
                    if_index: _,
                    name,
                    up: _,
                    down: _,
                    up_rate,
                    down_rate,
                } => {
                    if name == "lo" {
                        continue;
                    }
                    right.push(Renderable::Text {
                        text: format!(
                            "{} {}↓ {}↑",
                            name,
                            display_bytes(*up_rate) + "/s",
                            display_bytes(*down_rate) + "/s",
                        ),
                        fg: 0xffffffff,
                        bg: 0x00000000,
                    });
                }
            }
            right.push(Renderable::Space(1.0))
        }

        for sink_volume in self.audio_state.sink_volume.iter() {
            right.push(Renderable::Text {
                text: format!("{:.1}%", sink_volume.cbrt() * 100.0),
                fg: 0xffffffff,
                bg: 0x00000000,
            });
            right.push(Renderable::Space(1.0))
        }

        RenderState {
            left,
            right,
            center,
        }
    }

    pub async fn run_event_loop<S: StreamExt<Item = Message> + std::marker::Unpin>(
        mut self,
        mut message_receiver: S,
        render_sender: Sender<RenderState>,
    ) {
        render_sender
            .send(self.to_renderable_state())
            .await
            .expect("To be able to send render requests without drama, when initializing");
        while let Some(message) = message_receiver.next().await {
            self.update(message);
            render_sender
                .send(self.to_renderable_state())
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
                SwayMessage::WindowFocusedChange { window_name } => {
                    self.focused_window_name = window_name
                }
            },
            Message::Mpd(mpd_message) => match mpd_message {
                MpdMessage::MpdPlayerUpdate { status } => {
                    self.mpd_status = Some(status);
                }
                MpdMessage::MpdTimeElapsed { status } => {
                    self.mpd_status = Some(status);
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
            Message::Audio(audio_message) => match audio_message {
                AudioMessage::SinkVolume(items) => self.audio_state.sink_volume = items,
                AudioMessage::SourceVolume(items) => self.audio_state.source_volume = items,
            },
        }
    }
}

const UNITS: [(&str, u64); 5] = [
    ("B", 1),
    ("KiB", 1024),
    ("MiB", 1024),
    ("GiB", 1024),
    ("TiB", 1024),
];

fn display_bytes(x: u64) -> String {
    let mut scaled_size = x;
    let mut current_unit_idx = 0;
    while scaled_size
        > (UNITS
            .get(current_unit_idx + 1)
            .map(|unit| unit.1)
            .unwrap_or(u64::MAX))
    {
        current_unit_idx += 1;
        scaled_size /= UNITS[current_unit_idx].1
    }
    let display_str = format!("{scaled_size} {}", UNITS[current_unit_idx].0);
    format!("{display_str:>8}")
}
