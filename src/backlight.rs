use std::io::{Read, Seek, SeekFrom};
use std::num::ParseIntError;
use std::{
    fs::{self, File},
    io::Error,
    os::fd::AsRawFd,
};

use mio::{Events, Interest, Token};
use mio::{Poll, unix::SourceFd};
use tokio::{
    runtime::Handle,
    sync::mpsc::{Sender, channel, error::SendError},
};
use tokio_stream::wrappers::ReceiverStream;

use crate::files::{ReadIntError, read_int_from_file};
use crate::state::Message;

#[derive(Debug)]
enum BacklightError {
    StdIoError(Error),
    ReadIntError(ReadIntError),
    SendError(SendError<Message>),
}

impl From<Error> for BacklightError {
    fn from(value: Error) -> Self {
        Self::StdIoError(value)
    }
}

impl From<ReadIntError> for BacklightError {
    fn from(value: ReadIntError) -> Self {
        Self::ReadIntError(value)
    }
}

#[derive(Debug, Clone)]
pub struct Backlight {
    pub max_brightness: usize,
    pub brightness: usize,
}

#[derive(Debug)]
pub enum BacklightMessage {
    BacklightsInit(Vec< Backlight >),
    BrightnessChange { index: usize, brightness: usize },
}

impl From<SendError<Message>> for BacklightError {
    fn from(value: SendError<Message>) -> Self {
        Self::SendError(value)
    }
}

fn backlight_generator(sender: Sender<Message>) -> Result<(), BacklightError> {
    let mut backlight_poller = Poll::new()?;
    let mut backlight_paths = Vec::new();
    let mut backlights = Vec::new();
    // Need this to keep the actual_brightness files open to listen to "polling"
    let mut backlight_files = Vec::new();
    let mut backlight_brightness_file = Vec::new();

    for (i, backlight_dir) in fs::read_dir("/sys/class/backlight")?.enumerate() {
        let backlight_dir = backlight_dir?;
        backlight_paths.push(backlight_dir.path());
        let actual_brightness_path = backlight_dir.path().join("actual_brightness");
        let brightness_path = backlight_dir.path().join("brightness");
        let max_brightness_path = backlight_dir.path().join("max_brightness");
        let actual_brightness_file = File::open(actual_brightness_path)?;
        backlight_poller.registry().register(
            &mut SourceFd(&actual_brightness_file.as_raw_fd()),
            Token(i),
            Interest::PRIORITY,
        )?;
        let mut max_brightness_file = File::open(max_brightness_path)?;
        let mut brightness_file = File::open(brightness_path)?;
        backlight_files.push(actual_brightness_file);
        let max_brightness = read_int_from_file(&mut max_brightness_file)?;
        let brightness = read_int_from_file(&mut brightness_file)?;
        backlights.push(Backlight {
                max_brightness,
                brightness,
            });
        backlight_brightness_file.push(brightness_file)
    }
        sender.blocking_send(Message::Backlight(BacklightMessage::BacklightsInit(
            backlights
        )))?;
    let mut events = Events::with_capacity(1);
    loop {
        backlight_poller.poll(&mut events, None)?;
        for event in events.iter() {
            sender.blocking_send(Message::Backlight(BacklightMessage::BrightnessChange {
                index: event.token().0,
                brightness: read_int_from_file(&mut backlight_brightness_file[event.token().0])?,
            }))?;
        }
    }
}

pub fn backlight_subscription(rt: Handle) -> ReceiverStream<Message> {
    let (sender, receiver) = channel(1);
    rt.clone().spawn_blocking(move || {
        loop {
            log::error!("Backlight subscription event loop returned, this should never happen, trying to reconnect {:?}", backlight_generator(sender.clone()));
        }
    });
    ReceiverStream::new(receiver)
}
