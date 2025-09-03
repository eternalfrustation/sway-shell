use std::time::Duration;

use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::error::SendError;
use tokio::{runtime::Handle, sync::mpsc::Sender};

use crate::netlink::ethtool::EthtoolPhy;
use crate::netlink::nl80211::Nl80211Interface;
use crate::netlink::routel::LinkInfo;
use crate::netlink::{Netlink, NetlinkCommandError, NetlinkInitError};
use crate::state::Message;

#[derive(Debug, Clone)]
pub enum Network {
    Wifi {
        if_index: i32,
        if_name: String,
        ssid: Option<String>,
        up: u64,
        down: u64,
        up_rate: u64,
        down_rate: u64,
    },
    Network {
        if_index: i32,
        name: String,
        up: u64,
        down: u64,
        up_rate: u64,
        down_rate: u64,
    },
}

impl Network {
    fn from_linkinfo(
        link_info: Vec<LinkInfo>,
        wifi_interfaces: Vec<Nl80211Interface>,
        ethtool_interfaces: Vec<EthtoolPhy>,
        prev_link_info: Vec<Self>,
        interval: Duration,
    ) -> Vec<Self> {
        link_info
            .into_iter()
            .map(|link| {
                let prev_link_stats = prev_link_info.iter().find_map(|prev_link| match prev_link {
                    Network::Wifi {
                        if_index,
                        if_name,
                        ssid,
                        up,
                        down,
                        up_rate,
                        down_rate,
                    } => {
                        if *if_index == link.ifi_index {
                            Some((up, down))
                        } else {
                            None
                        }
                    }
                    Network::Network {
                        if_index,
                        name,
                        up,
                        down,
                        up_rate,
                        down_rate,
                    } => {
                        if *if_index == link.ifi_index {
                            Some((up, down))
                        } else {
                            None
                        }
                    }
                });
                if let Some(wifi_interface) = wifi_interfaces
                    .iter()
                    .find(|iface| iface.if_index as i32 == link.ifi_index)
                {
                    Self::Wifi {
                        if_index: link.ifi_index,
                        if_name: link.ifname,
                        ssid: wifi_interface.ssid.clone(),
                        up: link.stats64.tx_bytes,
                        down: link.stats64.rx_bytes,
                        up_rate: prev_link_stats
                            .map(|(prev_up, _)| {
                                (link.stats64.tx_bytes.saturating_sub(*prev_up)) / interval.as_secs()
                            })
                            .unwrap_or_default(),
                        down_rate: prev_link_stats
                            .map(|(_, prev_down)| {
                                (link.stats64.tx_bytes.saturating_sub(*prev_down))
                                    / interval.as_secs()
                            })
                            .unwrap_or_default(),
                    }
                } else {
                    Self::Network {
                        if_index: link.ifi_index,
                        name: link.ifname,
                        up: link.stats64.tx_bytes,
                        down: link.stats64.rx_bytes,
                        up_rate: prev_link_stats
                            .map(|(prev_up, _)| {
                                (link.stats64.tx_bytes.saturating_sub(*prev_up)) / interval.as_secs()
                            })
                            .unwrap_or_default(),
                        down_rate: prev_link_stats
                            .map(|(_, prev_down)| {
                                (link.stats64.tx_bytes.saturating_sub(*prev_down)) / interval.as_secs()
                            })
                            .unwrap_or_default(),
                    }
                }
            })
            .collect()
    }
}

pub type NetworkMessage = Vec<Network>;

#[derive(Debug)]
pub enum NetworkError {
    NetlinkInitError(NetlinkInitError),
    NetlinkCommandError(NetlinkCommandError),
    SendError(SendError<Message>),
}

impl From<NetlinkInitError> for NetworkError {
    fn from(value: NetlinkInitError) -> Self {
        Self::NetlinkInitError(value)
    }
}

impl From<NetlinkCommandError> for NetworkError {
    fn from(value: NetlinkCommandError) -> Self {
        Self::NetlinkCommandError(value)
    }
}

impl From<SendError<Message>> for NetworkError {
    fn from(value: SendError<Message>) -> Self {
        Self::SendError(value)
    }
}

async fn network_generator(sender: Sender<Message>) -> Result<(), NetworkError> {
    let netlink = Netlink::connect().await?;
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    let mut prev_instant = interval.tick().await;
    let mut prev_link_info = Vec::new();
    loop {
        let new_instant = interval.tick().await;
        let duration = new_instant - prev_instant;
        prev_instant = new_instant;

        let networks = Network::from_linkinfo(
            netlink.retrieve().await?,
            netlink.retrieve().await?,
            netlink.retrieve().await?,
            prev_link_info.clone(),
            duration,
        );
        prev_link_info = networks.clone();
        println!("{:#?}", networks);
        sender.send(Message::Network(networks)).await?;
    }
}

// TODO: USE NOTIFICATIONS INSTEAD OF TIMER
pub fn network_subscription(rt: Handle) -> tokio_stream::wrappers::ReceiverStream<Message> {
    let (sender, receiver) = channel(1);
    rt.clone().spawn(async move {
        loop {
            log::error!(
                "Network event loop returned, this should never happen, trying to reconnect {:?}",
                network_generator(sender.clone()).await
            );
        }
    });
    tokio_stream::wrappers::ReceiverStream::new(receiver)
}
