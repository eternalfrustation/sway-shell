use std::{env::VarError, fmt::Display, os::unix::net::UnixStream, path::PathBuf};

use mpd::{Idle, Subsystem};
use tokio::{
    runtime::Handle,
    sync::mpsc::{Sender, channel, error::SendError},
    time::MissedTickBehavior,
};

use crate::state::Message;

#[derive(Debug)]
enum MpdError {
    VarError(VarError),
    StdIOError(std::io::Error),
    MpdInternalError(mpd::error::Error),
    SendError(SendError<Message>),
}

#[derive(Debug)]
pub enum MpdMessage {
    MpdPlayerUpdate { status: mpd::Status },
    MpdSongUpdate { song: Option<mpd::Song> },
    MpdTimeElapsed { status: mpd::Status },
}

impl Display for MpdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MpdError::VarError(var_error) => {
                f.write_fmt(format_args!("Environment Variable Error: {}", var_error))
            }
            MpdError::StdIOError(error) => f.write_fmt(format_args!("StdIO Error: {}", error)),
            MpdError::MpdInternalError(error) => f.write_fmt(format_args!("MPD Error: {}", error)),
            MpdError::SendError(send_error) => {
                f.write_fmt(format_args!("Channel Error: {}", send_error))
            }
        }
    }
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

async fn song_duration_generator(output: Sender<Message>, mpd_socket_conn: PathBuf) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let conn = mpd::client::Client::new(UnixStream::connect(mpd_socket_conn.clone()).unwrap());

    if let Ok(mut conn) = conn {
        loop {
            let status = conn.status().ok();
            if let Some(status) = status {
                match output
                    .send(Message::Mpd(MpdMessage::MpdTimeElapsed { status: status }))
                    .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        log::error!("Error while sending message for timed response: {e}");
                        break;
                    }
                }
            }
            interval.tick().await;
        }
    } else {
        log::error!("{:?}", conn);
    }
}

fn mpd_generator(output: Sender<Message>, rt: Handle) -> Result<(), MpdError> {
    let a = PathBuf::from(std::env::var("XDG_RUNTIME_DIR")?).join("mpd/socket");
    let mut conn = mpd::client::Client::new(UnixStream::connect(a.clone())?)?;
    let status = conn.status()?;
    let mut previous_state = status.state;
    let mut timed_update = if previous_state == mpd::State::Play {
        Some(rt.spawn(song_duration_generator(output.clone(), a.clone())))
    } else {
        None
    };
    output.blocking_send(Message::Mpd(MpdMessage::MpdPlayerUpdate { status }))?;
    output.blocking_send(Message::Mpd(MpdMessage::MpdSongUpdate {
        song: conn.currentsong()?,
    }))?;
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
                                timed_update = Some(
                                    rt.spawn(song_duration_generator(output.clone(), a.clone())),
                                );
                            }
                            mpd::State::Stop => {
                                if let Some(timed_update) = timed_update {
                                    timed_update.abort()
                                }
                                timed_update = None;
                            }
                            mpd::State::Pause => {
                                if let Some(timed_update) = timed_update {
                                    timed_update.abort()
                                }
                                timed_update = None;
                            }
                        }
                        rt.spawn(async {});
                        previous_state = status.state;
                    }
                    output.blocking_send(Message::Mpd(MpdMessage::MpdPlayerUpdate { status }))?;
                    let song = conn.currentsong()?;
                    output.blocking_send(Message::Mpd(MpdMessage::MpdSongUpdate { song }))?;
                }
                _ => {}
            }
        }
    }
}

pub fn mpd_subscription(rt: Handle) -> tokio_stream::wrappers::ReceiverStream<Message> {
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
