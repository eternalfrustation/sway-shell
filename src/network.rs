use tokio::sync::mpsc::channel;
use tokio::{runtime::Handle, sync::mpsc::Sender};

use crate::netlink::ethtool::EthtoolPhy;
use crate::netlink::nl80211::Nl80211Interface;
use crate::netlink::routel::{LinkInfo, LinkStats64};
use crate::netlink::{Netlink, NetlinkCommandError, NetlinkInitError};
use crate::state::Message;

#[derive(Debug, Clone)]
pub enum Network {
    Wifi(Nl80211Interface),
    Network { name: String },
}

pub type NetworkMessage = Vec<Network>;

#[derive(Debug)]
pub enum NetworkError {
    NetlinkInitError(NetlinkInitError),
    Nl80211Error(NetlinkCommandError),
}

impl From<NetlinkInitError> for NetworkError {
    fn from(value: NetlinkInitError) -> Self {
        Self::NetlinkInitError(value)
    }
}

/// To find the values, look in include/uapi/linux/ethtool_netlink_generated.h
#[neli::neli_enum(serialized_type = "u8")]
pub enum EthtoolCommand {
    Unspecified = 0,
    GetLinkInfo = 2, /* Many many more elided */
}
impl neli::consts::genl::Cmd for EthtoolCommand {}

#[neli::neli_enum(serialized_type = "u16")]
pub enum EthtoolAttribute {
    Unspecified = 0,
    /* Literally hundreds elided */
}
impl neli::consts::genl::NlAttrType for EthtoolAttribute {}

async fn network_generator(sender: Sender<Message>) -> Result<(), NetworkError> {
    let netlink = Netlink::connect().await?;
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    loop {
        let interfaces: Result<Vec<Nl80211Interface>, _> = netlink.retrieve().await;
        let interfaces: Result<Vec<EthtoolPhy>, _> = netlink.retrieve().await;
        let interfaces: Result<Vec<LinkInfo>, _> = netlink.retrieve().await;
        println!("{interfaces:#?}");
        interval.tick().await;
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
