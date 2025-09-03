use std::io::Read;

pub mod ethtool;
pub mod nl80211;
pub mod routel;

use macaddr::{MacAddr6, MacAddr8};
use neli::FromBytes;
use neli::err::DeError;
use neli::{
    consts::{genl::CtrlCmd, socket::NlFamily},
    err::RouterError,
    genl::GenlmsghdrBuilderError,
    router::asynchronous::NlRouter,
    utils::Groups,
};

use crate::netlink::ethtool::EthtoolError;
use crate::netlink::nl80211::Nl80211Error;
use crate::netlink::routel::{ RoutelinkStatsError,RoutelinkInfoError };

#[derive(Debug, Clone)]
pub struct WifiStation {
    pub bssid: String,
    pub ssid: String,
    pub signal_strength: i32,
}

#[derive(Debug, Clone)]
pub enum MacAddr {
    Mac6(MacAddr6),
    Mac8(MacAddr8),
}

impl FromBytes for MacAddr {
    fn from_bytes(
        buffer: &mut std::io::Cursor<impl AsRef<[u8]>>,
    ) -> Result<Self, neli::err::DeError> {
        let mut mac_buf = [0u8; 8];
        let bytes_read = buffer.read(&mut mac_buf)?;
        match bytes_read {
            6 => Ok(MacAddr::Mac6(MacAddr6::new(
                mac_buf[0], mac_buf[1], mac_buf[2], mac_buf[3], mac_buf[4], mac_buf[5],
            ))),
            8 => Ok(MacAddr::Mac8(MacAddr8::new(
                mac_buf[0], mac_buf[1], mac_buf[2], mac_buf[3], mac_buf[4], mac_buf[5], mac_buf[6],
                mac_buf[7],
            ))),
            _ => Err(DeError::InvalidInput(bytes_read)),
        }
    }
}

pub struct Netlink {
    pub nl80211_sock: NlRouter,
    pub ethtool_sock: NlRouter,
    pub nl80211_family_id: u16,
    pub ethtool_family_id: u16,
    pub rtnl: NlRouter,
}

#[derive(Debug)]
pub enum NetlinkCommandError {
    MsgHdrError(GenlmsghdrBuilderError),

    Nl80211CommandRouterError(Nl80211Error),
    RtStatsCommandRouterError(RoutelinkStatsError),
    RtInfoCommandRouterError(RoutelinkInfoError),
    EthtoolCommandRouterError(EthtoolError),
}

impl From<GenlmsghdrBuilderError> for NetlinkCommandError {
    fn from(value: GenlmsghdrBuilderError) -> Self {
        Self::MsgHdrError(value)
    }
}

#[derive(Debug, Clone)]
pub enum NetlinkInitError {
    FamilyResolutionError(RouterError<u16, neli::types::Buffer>),

    CommandRouterError(
        RouterError<
            neli::consts::nl::GenlId,
            neli::genl::Genlmsghdr<CtrlCmd, neli::consts::genl::CtrlAttr>,
        >,
    ),
}

impl
    From<
        RouterError<
            neli::consts::nl::GenlId,
            neli::genl::Genlmsghdr<CtrlCmd, neli::consts::genl::CtrlAttr>,
        >,
    > for NetlinkInitError
{
    fn from(
        value: RouterError<
            neli::consts::nl::GenlId,
            neli::genl::Genlmsghdr<CtrlCmd, neli::consts::genl::CtrlAttr>,
        >,
    ) -> Self {
        Self::CommandRouterError(value)
    }
}

impl From<RouterError<u16, neli::types::Buffer>> for NetlinkInitError {
    fn from(value: RouterError<u16, neli::types::Buffer>) -> Self {
        Self::FamilyResolutionError(value)
    }
}

impl Netlink {
    pub async fn connect() -> Result<Self, NetlinkInitError> {
        let (nl80211_sock, _) = NlRouter::connect(
            NlFamily::Generic, /* family */
            Some(0),           /* pid */
            Groups::empty(),   /* groups */
        )
        .await?;
        let (ethtool_sock, _) = NlRouter::connect(
            NlFamily::Generic, /* family */
            Some(0),           /* pid */
            Groups::empty(),   /* groups */
        )
        .await?;

        ethtool_sock.enable_ext_ack(true)?;

        let nl80211_family_id = nl80211_sock.resolve_genl_family("nl80211").await?;
        let ethtool_family_id = nl80211_sock.resolve_genl_family("ethtool").await?;

        let (rtnl, _) = NlRouter::connect(NlFamily::Route, None, Groups::empty()).await?;
        rtnl.enable_ext_ack(true)?;
        rtnl.enable_strict_checking(true)?;
        Ok(Self {
            nl80211_family_id,
            ethtool_family_id,
            nl80211_sock,
            ethtool_sock,
            rtnl,
        })
    }

    pub async fn retrieve<E: Into<NetlinkCommandError>, T: NetlinkRetrievable<E>>(
        &self,
    ) -> Result<Vec<T>, NetlinkCommandError> {
        T::retrieve(self).await.map_err(|e| e.into())
    }
}

pub trait NetlinkRetrievable<E: Into<NetlinkCommandError>> {
    fn retrieve(netlink: &Netlink) -> impl Future<Output = Result<Vec<Self>, E>>
    where
        Self: Sized;
}
