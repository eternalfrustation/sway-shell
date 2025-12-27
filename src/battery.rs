use std::{fs, str::FromStr, thread, time::Duration};

use mio::{Events, Interest, Poll, Token};
use tokio::{
    runtime::Handle,
    sync::mpsc::{Sender, channel, error::SendError},
};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    files::{ReadIntError, read_int_from_file_path, read_string_from_file_path},
    state::Message,
};

#[derive(Debug)]
enum BatteryError {
    StdIoError(std::io::Error),
    ReadIntError(ReadIntError),

    SendError(SendError<Message>),
}

impl From<std::io::Error> for BatteryError {
    fn from(value: std::io::Error) -> Self {
        Self::StdIoError(value)
    }
}

impl From<ReadIntError> for BatteryError {
    fn from(value: ReadIntError) -> Self {
        Self::ReadIntError(value)
    }
}

impl From<SendError<Message>> for BatteryError {
    fn from(value: SendError<Message>) -> Self {
        Self::SendError(value)
    }
}

#[derive(Debug)]
pub enum BatteryMessage {
    UpdatePowerSupplies(Vec<PowerSupply>),
}

#[derive(Debug, Clone)]
pub enum PowerSupply {
    Battery {
        status: PowerSupplyStatus,
        capacity: usize,
    },
    Mains {
        online: bool,
    },
}

#[derive(Debug)]
pub enum PowerSupplyType {
    Unknown,
    Battery,
    UPS,
    Mains,
    USB,
    UsbDcp,
    UsbCdp,
    UsbAca,
    UsbC,
    UsbPd,
    UsbPdDrp,
    BrickID,
    Wireless,
}

impl FromStr for PowerSupplyType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Battery" => Self::Battery,
            "UPS" => Self::UPS,
            "Mains" => Self::Mains,
            "USB" => Self::USB,
            "USB_DCP" => Self::UsbDcp,
            "USB_CDP" => Self::UsbCdp,
            "USB_ACA" => Self::UsbAca,
            "USB_C" => Self::UsbC,
            "USB_PD" => Self::UsbPd,
            "USB_PD_DRP" => Self::UsbPdDrp,
            "BrickID" => Self::BrickID,
            "Wireless" => Self::Wireless,
            _ => Self::Unknown,
        })
    }
}

#[derive(Debug, Clone)]
pub enum PowerSupplyStatus {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

impl FromStr for PowerSupplyStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Charging" => Self::Charging,
            "Discharging" => Self::Discharging,
            "Not charging" => Self::NotCharging,
            "Full" => Self::Full,
            _ => Self::Unknown,
        })
    }
}

fn battery_generator(sender: Sender<Message>) -> Result<(), BatteryError> {
    loop {
        let mut power_supplies = Vec::new();
        for power_supply_dir in fs::read_dir("/sys/class/power_supply")? {
            let power_supply_dir = power_supply_dir?;
            let power_supply_type: PowerSupplyType =
                read_string_from_file_path(power_supply_dir.path().join("type"))?
                    .trim()
                    .parse()
                    .expect("This will never happen because _ case catches all strings");
            match power_supply_type {
                PowerSupplyType::Battery => {
                    let status: PowerSupplyStatus =
                        read_string_from_file_path(power_supply_dir.path().join("status"))?
                            .trim()
                            .parse()
                            .expect("All paths are handled");
                    let capacity =
                        read_int_from_file_path(power_supply_dir.path().join("capacity"))?;
                    power_supplies.push(PowerSupply::Battery { status, capacity });
                }
                PowerSupplyType::Mains => {
                    let online = read_int_from_file_path(power_supply_dir.path().join("online"))?;
                    power_supplies.push(PowerSupply::Mains { online: online > 0 });
                }
                x => {
                    log::error!("power supply type: {x:?} not handled");
                }
            };
        }
        sender.blocking_send(Message::Battery(BatteryMessage::UpdatePowerSupplies(
            power_supplies,
        )))?;
        thread::sleep(Duration::from_mins(1));
    }
}

pub fn battery_subscription(rt: Handle) -> ReceiverStream<Message> {
    let (sender, receiver) = channel(1);
    rt.clone().spawn_blocking(move || {
        loop {
            log::error!("Battery subscription event loop returned, this should never happen, trying to reconnect: {:?}", battery_generator(sender.clone()));
        }
    });
    ReceiverStream::new(receiver)
}
