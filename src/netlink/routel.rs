use neli::{
    FromBytes,
    attr::Attribute,
    consts::{
        nl::NlmF,
        rtnl::{IflaStats, RtAddrFamily, Rtm},
    },
    err::RouterError,
    nl::NlPayload,
    rtnl::{Ifstatsmsg, IfstatsmsgBuilder},
};

use crate::netlink::{Netlink, NetlinkCommandError, NetlinkRetrievable};

#[derive(Debug, Clone, FromBytes)]
pub struct Link64 {
    rx_packets: u64,
    tx_packets: u64,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_errors: u64,
    tx_errors: u64,
    rx_dropped: u64,
    tx_dropped: u64,
    multicast: u64,
    collisions: u64,
    rx_length_errors: u64,
    rx_over_errors: u64,
    rx_crc_errors: u64,
    rx_frame_errors: u64,
    rx_fifo_errors: u64,
    rx_missed_errors: u64,
    tx_aborted_errors: u64,
    tx_carrier_errors: u64,
    tx_fifo_errors: u64,
    tx_heartbeat_errors: u64,
    tx_window_errors: u64,
    rx_compressed: u64,
    tx_compressed: u64,
    rx_nohandler: u64,
    rx_otherhost_dropped: u64,
}

#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(setter(into))]
pub struct RtLinkStats {
    family: RtLinkFamily,
    ifindex: u32,
    filter_mask: u32,
    link_64: Link64,
}

#[neli::neli_enum(serialized_type = "u8")]
pub enum RtLinkFamily {
    IPMR = 128,
    IP6MR = 129, /* Several more, common ones above */
}

pub type RoutelinkError = RouterError<Rtm, Ifstatsmsg>;

impl Into<NetlinkCommandError> for RoutelinkError {
    fn into(self) -> NetlinkCommandError {
        NetlinkCommandError::RtCommandRouterError(self)
    }
}

impl NetlinkRetrievable<RoutelinkError> for RtLinkStats {
    async fn retrieve(netlink: &Netlink) -> Result<Vec<Self>, RoutelinkError> {
        let mut recv = netlink
            .rtnl
            .send::<_, _, Rtm, ()>(
                Rtm::Getstats,
                NlmF::DUMP | NlmF::ACK,
                neli::nl::NlPayload::Payload(
                    IfstatsmsgBuilder::default()
                        .family(RtAddrFamily::Unspecified)
                        .filter_mask(IflaStats::LINK_64)
                        .build()?,
                ),
            )
            .await?;
        while let Some(response) = recv.next::<Rtm, Ifstatsmsg>().await {
            let response = response?;
            let payload = {
                match response.nl_payload() {
                    NlPayload::Payload(x) => x,
                    _ => {
                        continue;
                    }
                }
            };

            let attr_handle = payload.rtattrs().get_attr_handle();
            for attr in attr_handle.iter() {
                println!("{:?}", attr.get_payload_as::<Link64>());
            }
        }
        Ok(vec![])
    }
}
