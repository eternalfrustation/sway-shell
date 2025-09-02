use neli::{
    attr::Attribute,
    consts::nl::NlmF,
    err::RouterError,
    genl::{Genlmsghdr, GenlmsghdrBuilder},
    nl::NlPayload,
    router::asynchronous::NlRouterReceiverHandle,
};

use bitflags::bitflags;

use crate::netlink::{Netlink, NetlinkCommandError, NetlinkRetrievable};

#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(setter(into))]
pub struct EthtoolPhy {
    pub phy_index: u32,
    pub driver_name: String,
    pub name: String,
    pub upstream_type: u32,
    pub upstream_index: u32,
    #[builder(default)]
    pub upstream_sfp_name: Option<String>,
    #[builder(default)]
    pub downstream_sfp_name: Option<String>,
}

/// To find the values, look in usr/include/linux/ethtool_netlink_generated.h
#[neli::neli_enum(serialized_type = "u8")]
pub enum EthtoolCommand {
    PhyGet = 45,
    StatsGet = 32,
    /* Many many more elided */
}
impl neli::consts::genl::Cmd for EthtoolCommand {}

pub struct EthToolCommandHeaderFlags(u32);

bitflags! {
    impl EthToolCommandHeaderFlags: u32 {
        const COMPACT_BITSETS = 1;
        const OMIT_REPLY = 2;
        const STATS = 4;
    }
}

pub struct EthToolCommandHeader {
    dev_index: u32,
    dev_name: String,
    flags: EthToolCommandHeaderFlags,
    phy_index: u32,
}

#[neli::neli_enum(serialized_type = "u32")]
pub enum EthtoolUpstreamType {
    Mac = 0,
    Phy = 1,
}

#[neli::neli_enum(serialized_type = "u16")]
pub enum EthtoolPhyAttribute {
    Unspecified = 0,
    ReqHdr = 1,
    Index = 2,
    DrvName = 3,
    Name = 4,
    UpstreamType = 5,
    UpstreamIndex = 6,
    UpstreamSfpName = 7,
    DownstreamSfpName = 8,
}
impl neli::consts::genl::NlAttrType for EthtoolPhyAttribute {}

pub type EthtoolError =
    RouterError<u16, neli::genl::Genlmsghdr<EthtoolCommand, EthtoolPhyAttribute>>;

impl Into<NetlinkCommandError> for EthtoolError {
    fn into(self) -> NetlinkCommandError {
        NetlinkCommandError::EthtoolCommandRouterError(self)
    }
}

impl NetlinkRetrievable<EthtoolError> for EthtoolPhy {
    async fn retrieve(netlink: &Netlink) -> Result<Vec<Self>, EthtoolError> {
        let mut recv: NlRouterReceiverHandle<u16, Genlmsghdr<EthtoolCommand, EthtoolPhyAttribute>> =
            netlink
                .ethtool_sock
                .send(
                    netlink.ethtool_family_id,
                    NlmF::DUMP,
                    NlPayload::Payload(
                        GenlmsghdrBuilder::default()
                            .cmd(EthtoolCommand::PhyGet)
                            .version(1)
                            .build()?,
                    ),
                )
                .await?;
        let mut ethernet_interfaces = Vec::new();
        let mut maybe_msg = recv
            .next::<u16, Genlmsghdr<EthtoolCommand, EthtoolPhyAttribute>>()
            .await;

        while let Some(Ok(msg)) = maybe_msg {
            maybe_msg = recv
                .next::<u16, Genlmsghdr<EthtoolCommand, EthtoolPhyAttribute>>()
                .await;

            let mut interface_builder = EthtoolPhyBuilder::default();
            // Messages with the NlmF::DUMP flag end with an empty payload message
            // Don't parse message unless receive proper payload (non-error, non-empty, non-ack)
            let payload: &Genlmsghdr<_, _> = match msg.nl_payload() {
                NlPayload::Payload(p) => p,
                x => {
                    continue;
                }
            };

            let attr_handle = payload.attrs().get_attr_handle();
            for attr in attr_handle.iter() {
                match attr.nla_type().nla_type() {
                    EthtoolPhyAttribute::Unspecified => {
                        log::error!(
                            "Unspecified Value encountered when parsing get-interfaces result"
                        );
                    }
                    EthtoolPhyAttribute::UnrecognizedConst(v) => {
                        log::error!(
                            "Unrecognized Const encountered when parsing get-interfaces result: {v}"
                        );
                    }
                    EthtoolPhyAttribute::ReqHdr => {}
                    EthtoolPhyAttribute::Index => {
                        interface_builder.phy_index(
                            attr.get_payload_as::<u32>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    EthtoolPhyAttribute::DrvName => {
                        interface_builder.driver_name(
                            attr.get_payload_as_with_len::<String>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    EthtoolPhyAttribute::Name => {
                        interface_builder.name(
                            attr.get_payload_as_with_len::<String>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    EthtoolPhyAttribute::UpstreamType => {
                        interface_builder.upstream_type(
                            attr.get_payload_as::<EthtoolUpstreamType>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    EthtoolPhyAttribute::UpstreamIndex => {
                        interface_builder.upstream_index(
                            attr.get_payload_as::<u32>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    EthtoolPhyAttribute::UpstreamSfpName => {
                        interface_builder.upstream_sfp_name(
                            attr.get_payload_as_with_len::<String>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                    EthtoolPhyAttribute::DownstreamSfpName => {
                        interface_builder.downstream_sfp_name(
                            attr.get_payload_as_with_len::<String>()
                                .expect("There to be vif radio mask that fits in u32"),
                        );
                    }
                }
            }
            match interface_builder.build() {
                Ok(phy) => {
                    ethernet_interfaces.push(phy);
                }
                Err(e) => {
                    log::error!("{e:?}")
                }
            }
        }
        Ok(ethernet_interfaces)
    }
}
