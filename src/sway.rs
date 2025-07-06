use std::sync::Arc;

use swayipc::{Event, EventType, Node, Rect, WorkspaceChange};
use tokio::{
    runtime::Runtime,
    sync::mpsc::{Receiver, Sender, channel, error::SendError},
};

use crate::state::Message;

#[derive(Debug)]
pub enum WorkspaceFromNodeError {
    NoOutput,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: i64,
    /// The workspace number or -1 for workspaces that do not start with a
    /// number.
    pub num: i32,
    /// The name of the workspace.
    pub name: Option<String>,
    pub layout: String,
    /// Whether the workspace is currently visible on any output.
    pub visible: bool,
    /// Whether the workspace is currently focused by the default seat (seat0).
    pub focused: bool,
    /// Whether a view on the workspace has the urgent flag set.
    pub urgent: bool,
    pub representation: Option<String>,
    pub orientation: String,
    /// The bounds of the workspace. It consists of x, y, width, and height.
    pub rect: Rect,
    /// The name of the output that the workspace is on.
    pub output: String,
    pub focus: Vec<i64>,
}

impl TryFrom<Node> for Workspace {
    type Error = WorkspaceFromNodeError;
    fn try_from(value: Node) -> Result<Self, Self::Error> {
        Ok(Workspace {
            id: value.id,
            num: value.num.unwrap_or(-1),
            name: value.name,
            layout: format!("{:?}", value.layout).to_lowercase(),
            visible: value.visible.unwrap_or(false),
            focused: value.focused,
            focus: value.focus,
            urgent: value.urgent,
            representation: value.representation,
            orientation: "none".to_string(),
            rect: value.rect,
            output: value.output.ok_or(WorkspaceFromNodeError::NoOutput)?,
        })
    }
}

impl From<swayipc::Workspace> for Workspace {
    fn from(value: swayipc::Workspace) -> Self {
        Workspace {
            id: value.id,
            num: value.num,
            name: Some(value.name),
            layout: value.layout,
            focus: value.focus,
            focused: value.focused,
            visible: value.visible,
            urgent: value.urgent,
            representation: value.representation,
            orientation: value.orientation,
            rect: value.rect,
            output: value.output,
        }
    }
}

#[derive(Debug)]
enum SwayError {
    ConnectionError(swayipc::Error),
    ChannelError(SendError<Message>),
}

impl From<swayipc::Error> for SwayError {
    fn from(value: swayipc::Error) -> Self {
        Self::ConnectionError(value)
    }
}

impl From<SendError<Message>> for SwayError {
    fn from(value: SendError<Message>) -> Self {
        Self::ChannelError(value)
    }
}

async fn sway_generator(mut output: Sender<Message>) -> Result<(), SwayError> {
    let mut conn = swayipc::Connection::new()?;
    for workspace in conn.get_workspaces()?.into_iter().map(|v| v.into()) {
        output.send(Message::WorkspaceAdd(workspace)).await?;
    }

    for event in conn.subscribe([EventType::Workspace])? {
        match event {
            Err(e) => {
                log::error!("{e:?}");
            }
            Ok(event) => {
                match event {
                    Event::Workspace(workspace_event) => match workspace_event.change {
                        WorkspaceChange::Init => {
                            output
                                .send(Message::WorkspaceAdd(
                                    workspace_event
                                        .current
                                        .expect("Workspace to not be null when it is created")
                                        .try_into()
                                        .expect("This to be a workspace"),
                                ))
                                .await?;
                        }
                        WorkspaceChange::Empty => {
                            output
                                .send(Message::WorkspaceDel(
                                    workspace_event
                                        .current
                                        .expect("Workspace not null when emptying")
                                        .id,
                                ))
                                .await?;
                        }
                        WorkspaceChange::Focus => {
                            if let Some(v) =
                                workspace_event.old.map(|v| Message::WorkspaceChangeFocus {
                                    id: v.id,
                                    focus: v.focus,
                                    focused: v.focused,
                                })
                            {
                                output.send(v).await?;
                            }
                            if let Some(v) =
                                workspace_event
                                    .current
                                    .map(|v| Message::WorkspaceChangeFocus {
                                        id: v.id,
                                        focus: v.focus,
                                        focused: v.focused,
                                    })
                            {
                                output.send(v).await?;
                            }
                        }
                        WorkspaceChange::Move => {
                            log::info!("Workspace moved, do nothing");
                        }
                        WorkspaceChange::Rename => {
                            output
                                .send(
                                    workspace_event
                                        .current
                                        .map(|v| Message::WorkspaceRename {
                                            id: v.id,
                                            name: v.name,
                                        })
                                        .expect("Workspace not null when emptying"),
                                )
                                .await?;
                        }
                        WorkspaceChange::Urgent => {
                            output
                                .send(
                                    workspace_event
                                        .current
                                        .map(|v| Message::WorkspaceChangeUrgency {
                                            id: v.id,
                                            urgent: v.urgent,
                                        })
                                        .expect("Workspace not null when emptying"),
                                )
                                .await?;
                            output
                                .send(
                                    workspace_event
                                        .old
                                        .map(|v| Message::WorkspaceChangeUrgency {
                                            id: v.id,
                                            urgent: v.urgent,
                                        })
                                        .expect("Workspace not null when emptying"),
                                )
                                .await?;
                        }
                        WorkspaceChange::Reload => {
                            log::info!("Config Reloaded, nothing changes for me");
                        }
                        _ => log::error!("Unknown Workspace Event type"),
                    },
                    _ => {
                        log::error!("Unknown event encountered");
                    }
                };
            }
        };
    }
    Ok(())
}

pub fn sway_subscription(rt: Arc<Runtime>) -> Receiver<Message> {
    let (sender, receiver) = channel(1);
    rt.spawn(async move {
        loop {
            log::error!(
                "Sway subscription event loop returned, this should never happen {:?}",
                sway_generator(sender.clone()).await
            )
        }
    });

    receiver
}
