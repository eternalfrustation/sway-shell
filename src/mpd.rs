use std::{env::VarError, os::unix::net::UnixStream, path::PathBuf, sync::Arc};

use mpd::{Client, Idle, Subsystem};
use tokio::{
    runtime::Runtime,
    sync::mpsc::{Sender, channel, error::SendError},
    time::MissedTickBehavior,
};
use tokio_stream::Stream;

use crate::state::Message;

#[derive(Debug)]
enum MpdError {
    VarError(VarError),
    StdIOError(std::io::Error),
    MpdInternalError(mpd::error::Error),
    SendError(SendError<Message>),
}

impl From<VarError> for MpdError {
    fn from(value: VarError) -> Self {
        Self::VarError(value)
    }
}

impl From<std::io::Error> for MpdError {
    fn from(value: std::io::Error) -> Self {
        Self::StdIOError(value)
    }
}

impl From<mpd::error::Error> for MpdError {
    fn from(value: mpd::error::Error) -> Self {
        Self::MpdInternalError(value)
    }
}

impl From<SendError<Message>> for MpdError {
    fn from(value: SendError<Message>) -> Self {
        Self::SendError(value)
    }
}

fn mpd_generator(output: Sender<Message>, rt: Arc<Runtime>) -> Result<(), MpdError> {
    let a = PathBuf::from(std::env::var("XDG_RUNTIME_DIR")?).join("mpd/socket");
    let mut conn = mpd::client::Client::new(UnixStream::connect(a.clone())?)?;
    let status = conn.status()?;
    let mut timed_update = None;
    let mut previous_state = status.state;
    output.blocking_send(Message::MpdPlayerUpdate { status })?;
    loop {
        let events = conn.wait(&[Subsystem::Player])?;
        for event in &events {
            match event {
                Subsystem::Player => {
                    let status = conn.status()?;
                    if status.state != previous_state {
                        match status.state {
                            mpd::State::Play => {
                                let a = a.clone();
                                let output = output.clone();
                                timed_update = Some(rt.spawn(async move {
                                    let mut conn = mpd::client::Client::new(
                                        UnixStream::connect(a.clone()).unwrap(),
                                    )
                                    .unwrap();
                                    let mut interval =
                                        tokio::time::interval(tokio::time::Duration::from_secs(1));
                                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                                    loop {
                                        let status = conn.status().ok();
                                        if let Some(Some(elapsed)) = status.map(|s| s.elapsed) {
                                            output.send(Message::MpdTimeElapsed { elapsed }).await;
                                        }
                                        interval.tick().await;
                                    }
                                }));
                            }
                            mpd::State::Stop => {
                                if let Some(ref timed_update) = timed_update {
                                    timed_update.abort();
                                }
                            }
                            mpd::State::Pause => {
                                if let Some(ref timed_update) = timed_update {
                                    timed_update.abort();
                                }
                            }
                        }
                        rt.spawn(async {});
                        previous_state = status.state;
                    }
                    output.blocking_send(Message::MpdPlayerUpdate { status })?;
                }
                _ => {}
            }
        }
    }
}

pub fn mpd_subscription(rt: Arc<Runtime>) -> impl Stream<Item = Message> {
    let (sender, receiver) = channel(1);
    rt.clone().spawn_blocking(move || {
        loop {
            log::error!(
                "Sway subscription event loop returned, this should never happen, trying to reconnect {:?}",
                mpd_generator(sender.clone(), rt.clone())
            )
        }
    });

    tokio_stream::wrappers::ReceiverStream::new(receiver)
}
