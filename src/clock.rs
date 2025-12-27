use std::time::Duration;
use tokio::sync::mpsc::Sender;
use std::thread;

use tokio::{runtime::Handle, sync::mpsc::channel};
use tokio_stream::wrappers::ReceiverStream;

use crate::state::Message;

#[derive(Debug)]
pub enum ClockMessage {
    TimeUpdate(chrono::DateTime<chrono::Local>),
}

fn clock_generator(
    sender: Sender<Message>,
) -> Result<(), tokio::sync::mpsc::error::SendError<Message>> {
    loop {
        sender.blocking_send(Message::ClockMessage(ClockMessage::TimeUpdate(
            chrono::Local::now(),
        )))?;
        thread::sleep(Duration::from_mins(1));
    }
}

pub fn clock_subscription(rt: Handle) -> ReceiverStream<Message> {
    let (sender, receiver) = channel(1);
    rt.clone().spawn_blocking(move || {
        loop {
            log::error!("Clock subscription event loop returned, this should never happen, trying to reconnect {:?}", clock_generator(sender.clone()));
        }
    });
    ReceiverStream::new(receiver)
}
