use neli::{
    attr::Attribute, consts::nl::NlmF, err::RouterError, genl::{Genlmsghdr, GenlmsghdrBuilder}, nl::NlPayload, router::asynchronous::NlRouterReceiverHandle, FromBytes
};

use crate::netlink::{MacAddr, Netlink, NetlinkCommandError, NetlinkRetrievable};

#[derive(Debug, Clone, derive_builder::Builder, FromBytes)]
#[builder(setter(into))]
pub struct Nl80211TxqStats {
    backlog_bytes: u32,
    backlog_packets: u32,
    flows: u32,
    drops: u32,
    ecn_marks: u32,
    overlimit: u32,
    overmemory: u32,
    collisions: u32,
    tx_bytes: u32,
    tx_packets: u32,
    max_flows: u32,
}

#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(setter(into))]
pub struct Nl80211Interface {
    pub if_name: String,
    pub if_type: Nl80211IfType,
    pub if_index: u32,
    pub wiphy: u32,
    pub wdev: u64,
    pub mac: MacAddr,
    pub generation: u32,
    pub txq_stats: Nl80211TxqStats,
    pub addr4: u8,
    pub vif_radio_mask: u32,
    #[builder(default)]
    pub wiphy_tx_power_level: Option<u32>,
    #[builder(default)]
    pub ssid: Option<String>,
}

/// To find the values, look in include/uapi/linux/nl80211.h
#[neli::neli_enum(serialized_type = "u8")]
pub enum Nl80211Command {
    Unspecified = 0,
    GetWiPhy = 1,
    GetInterface = 5,
    /* Many many more elided */
}
impl neli::consts::genl::Cmd for Nl80211Command {}

#[neli::neli_enum(serialized_type = "u32")]
pub enum Nl80211IfType {
    Unspecified = 0,
    Station = 2,
    Ap = 3,
    Monitor = 6,
    P2pDevice = 10,
    /* Several more, common ones above */
}

#[neli::neli_enum(serialized_type = "u16")]
pub enum Nl80211InterfaceAttribute {
    Unspecified = 0,

    WiPhy = 1,

    IfIndex = 3,
    IfName = 4,
    IfType = 5,

    Mac = 6,

    Generation = 46,

    Ssid = 52,

    Addr4 = 83,

    WiPhyTxPowerLevel = 98,

    Wdev = 153,

    TxqStats = 265,

    VifRadioMask = 333,
    /* Literally hundreds elided */
}
impl neli::consts::genl::NlAttrType for Nl80211InterfaceAttribute {}

pub type Nl80211Error =
    RouterError<u16, neli::genl::Genlmsghdr<Nl80211Command, Nl80211InterfaceAttribute>>;

impl Into<NetlinkCommandError> for Nl80211Error {
    fn into(self) -> NetlinkCommandError {
        NetlinkCommandError::Nl80211CommandRouterError(self)
    }
}

impl NetlinkRetrievable<Nl80211Error> for Nl80211Interface {
    async fn retrieve(netlink: &Netlink) -> Result<Vec<Self>, Nl80211Error> {
        let mut recv: NlRouterReceiverHandle<
            u16,
            Genlmsghdr<Nl80211Command, Nl80211InterfaceAttribute>,
        > = netlink
            .nl80211_sock
            .send(
                netlink.nl80211_family_id,
                NlmF::DUMP | NlmF::ACK,
                NlPayload::Payload(
                    GenlmsghdrBuilder::default()
                        .cmd(Nl80211Command::GetInterface)
                        .version(1)
                        .build()?,
                ),
            )
            .await?;
        let mut wifi_interfaces = Vec::new();
        while let Some(Ok(msg)) = recv
            .next::<u16, Genlmsghdr<Nl80211Command, Nl80211InterfaceAttribute>>()
            .await
        {
            let mut interface_builder = Nl80211InterfaceBuilder::default();
            // Messages with the NlmF::DUMP flag end with an empty payload message
            // Don't parse message unless receive proper payload (non-error, non-empty, non-ack)
            let payload: &Genlmsghdr<_, _> = match msg.nl_payload() {
                NlPayload::Payload(p) => p,
                _ => continue,
            };

            let attr_handle = payload.attrs().get_attr_handle();
            for attr in attr_handle.iter() {
                match attr.nla_type().nla_type() {
                    Nl80211InterfaceAttribute::WiPhy => {
                        interface_builder.wiphy(
                            attr.get_payload_as::<u32>()
                                .expect("There to be WiPhy as u32 for attribute WiPhy"),
                        );
                    }
                    Nl80211InterfaceAttribute::IfName => {
                        interface_builder.if_name(
                            attr.get_payload_as_with_len::<String>()
                                .expect("There to be IfName as String"),
                        );
                    }
                    Nl80211InterfaceAttribute::IfType => {
                        interface_builder.if_type(
                            attr.get_payload_as::<Nl80211IfType>().expect(
                                "There to to be IfType that fits in Nl80211IfType, i.e. u16",
                            ),
                        );
                    }
                    Nl80211InterfaceAttribute::Wdev => {
                        interface_builder.wdev(
                            attr.get_payload_as::<u64>()
                                .expect("There to be Wdev id that fits in u64"),
                        );
                    }
                    Nl80211InterfaceAttribute::Unspecified => {
                        log::error!(
                            "Unspecified Value encountered when parsing get-interfaces result"
                        );
                    }
                    Nl80211InterfaceAttribute::IfIndex => {
                        interface_builder.if_index(
                            attr.get_payload_as::<u32>()
                                .expect("There to be IfIndex that fits in u32"),
                        );
                    }
                    Nl80211InterfaceAttribute::Mac => {
                        interface_builder.mac(
                            attr.get_payload_as::<MacAddr>()
                                .expect("There to be Mac Address data that fits in MacAddr"),
                        );
                    }
                    Nl80211InterfaceAttribute::Generation => {
                        interface_builder.generation(
                            attr.get_payload_as::<u32>()
                                .expect("There to be Mac Address data that fits in MacAddr"),
                        );
                    }
                    Nl80211InterfaceAttribute::Addr4 => {
                        interface_builder.addr4(
                            attr.get_payload_as::<u8>()
                                .expect("There to be Mac Address data that fits in MacAddr"),
                        );
                    }
                    Nl80211InterfaceAttribute::TxqStats => {
                        interface_builder.txq_stats(
                            attr.get_payload_as::<Nl80211TxqStats>()
                                .expect("There to be Mac Address data that fits in MacAddr"),
                        );
                    }
                    Nl80211InterfaceAttribute::Ssid => {
                        interface_builder.ssid(
                            attr.get_payload_as_with_len::<String>()
                                .expect("There to be SSID that fits in String"),
                        );
                    }
                    Nl80211InterfaceAttribute::WiPhyTxPowerLevel => {
                        interface_builder.wiphy_tx_power_level(
                            attr.get_payload_as::<u32>()
                                .expect("There to be WiPhy TxPower Level that fits in u32"),
                        );
                    }
                    Nl80211InterfaceAttribute::VifRadioMask => {
                        interface_builder.vif_radio_mask(
                            attr.get_payload_as::<u32>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    Nl80211InterfaceAttribute::UnrecognizedConst(v) => {
                        log::error!(
                            "Unrecognized Const encountered when parsing get-interfaces result: {v}"
                        );
                    }
                }
            }
            match interface_builder.build() {
                Ok(wifi) => {
                    wifi_interfaces.push(wifi);
                }
                Err(e) => {
                    log::error!("{e:?}")
                }
            }
        }
        Ok(wifi_interfaces)
    }
}
