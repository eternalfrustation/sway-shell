use neli::{
    FromBytes, TypeSize,
    attr::Attribute,
    consts::{
        nl::NlmF,
        rtnl::{IflaStats, RtAddrFamily, Rtm},
    },
    err::RouterError,
    nl::NlPayload,
    rtnl::{Ifinfomsg, IfinfomsgBuilder, Ifstatsmsg, IfstatsmsgBuilder},
};

use crate::netlink::{MacAddr, Netlink, NetlinkCommandError, NetlinkRetrievable};

#[derive(Debug, Clone, FromBytes)]
pub struct LinkStats64 {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
    pub multicast: u64,
    pub collisions: u64,
    pub rx_length_errors: u64,
    pub rx_over_errors: u64,
    pub rx_crc_errors: u64,
    pub rx_frame_errors: u64,
    pub rx_fifo_errors: u64,
    pub rx_missed_errors: u64,
    pub tx_aborted_errors: u64,
    pub tx_carrier_errors: u64,
    pub tx_fifo_errors: u64,
    pub tx_heartbeat_errors: u64,
    pub tx_window_errors: u64,
    pub rx_compressed: u64,
    pub tx_compressed: u64,
    pub rx_nohandler: u64,
    pub rx_otherhost_dropped: u64,
}

#[derive(Debug, Clone, FromBytes)]
pub struct LinkStats {
    pub rx_packets: u32,
    pub tx_packets: u32,
    pub rx_bytes: u32,
    pub tx_bytes: u32,
    pub rx_errors: u32,
    pub tx_errors: u32,
    pub rx_dropped: u32,
    pub tx_dropped: u32,
    pub multicast: u32,
    pub collisions: u32,
    pub rx_length_errors: u32,
    pub rx_over_errors: u32,
    pub rx_crc_errors: u32,
    pub rx_frame_errors: u32,
    pub rx_fifo_errors: u32,
    pub rx_missed_errors: u32,
    pub tx_aborted_errors: u32,
    pub tx_carrier_errors: u32,
    pub tx_fifo_errors: u32,
    pub tx_heartbeat_errors: u32,
    pub tx_window_errors: u32,
    pub rx_compressed: u32,
    pub tx_compressed: u32,
    pub rx_nohandler: u32,
}

#[neli::neli_enum(serialized_type = "u8")]
pub enum RtLinkFamily {
    IPMR = 128,
    IP6MR = 129, /* Several more, common ones above */
}

pub type RoutelinkStatsError = RouterError<Rtm, Ifstatsmsg>;
pub type RoutelinkInfoError = RouterError<Rtm, Ifinfomsg>;

impl Into<NetlinkCommandError> for RoutelinkStatsError {
    fn into(self) -> NetlinkCommandError {
        NetlinkCommandError::RtStatsCommandRouterError(self)
    }
}

impl Into<NetlinkCommandError> for RoutelinkInfoError {
    fn into(self) -> NetlinkCommandError {
        NetlinkCommandError::RtInfoCommandRouterError(self)
    }
}

impl NetlinkRetrievable<RoutelinkStatsError> for LinkStats64 {
    async fn retrieve(netlink: &Netlink) -> Result<Vec<Self>, RoutelinkStatsError> {
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
        let mut stats = Vec::new();
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
                stats.push(
                    attr.get_payload_as::<LinkStats64>()
                        .expect("To only get binary stuff that can fit into a Link64 struct"),
                )
            }
        }
        Ok(stats)
    }
}

#[derive(Debug, Clone, FromBytes)]
pub struct LinkIfMap {
    mem_start: u64,
    mem_end: u64,
    base_addr: u64,
    irq: u16,
    dma: u8,
    port: u8,
}

#[derive(Debug, Clone, FromBytes)]
pub struct Ipv4Devconf {
    forwarding: u32,
    mc_forwarding: u32,
    proxy_arp: u32,
    accept_redirects: u32,
    secure_redirects: u32,
    send_redirects: u32,
    shared_media: u32,
    rp_filter: u32,
    accept_source_route: u32,
    bootp_relay: u32,
    log_martians: u32,
    tag: u32,
    arpfilter: u32,
    medium_id: u32,
    noxfrm: u32,
    nopolicy: u32,
    force_igmp_version: u32,
    arp_announce: u32,
    arp_ignore: u32,
    promote_secondaries: u32,
    arp_accept: u32,
    arp_notify: u32,
    accept_local: u32,
    src_vmark: u32,
    proxy_arp_pvlan: u32,
    route_localnet: u32,
    igmpv2_unsolicited_report_interval: u32,
    igmpv3_unsolicited_report_interval: u32,
    ignore_routes_with_linkdown: u32,
    drop_unicast_in_l2_multicast: u32,
    drop_gratuitous_arp: u32,
    bc_forwarding: u32,
    arp_evict_nocarrier: u32,
}

impl TypeSize for Ipv4Devconf {
    fn type_size() -> usize {
        33 * 4
    }
}

#[derive(Debug, Clone, FromBytes)]
pub struct IflaAttrs {
    conf: Ipv4Devconf,
}

#[derive(Debug, Clone, FromBytes)]
pub struct Ipv6Devconf {
    forwarding: u32,
    hoplimit: u32,
    mtu6: u32,
    accept_ra: u32,
    accept_redirects: u32,
    autoconf: u32,
    dad_transmits: u32,
    rtr_solicits: u32,
    rtr_solicit_interval: u32,
    rtr_solicit_delay: u32,
    use_tempaddr: u32,
    temp_valid_lft: u32,
    temp_prefered_lft: u32,
    regen_max_retry: u32,
    max_desync_factor: u32,
    max_addresses: u32,
    force_mld_version: u32,
    accept_ra_defrtr: u32,
    accept_ra_pinfo: u32,
    accept_ra_rtr_pref: u32,
    rtr_probe_interval: u32,
    accept_ra_rt_info_max_plen: u32,
    proxy_ndp: u32,
    optimistic_dad: u32,
    accept_source_route: u32,
    mc_forwarding: u32,
    disable_ipv6: u32,
    accept_dad: u32,
    force_tllao: u32,
    ndisc_notify: u32,
    mldv1_unsolicited_report_interval: u32,
    mldv2_unsolicited_report_interval: u32,
    suppress_frag_ndisc: u32,
    accept_ra_from_local: u32,
    use_optimistic: u32,
    accept_ra_mtu: u32,
    stable_secret: u32,
    use_oif_addrs_only: u32,
    accept_ra_min_hop_limit: u32,
    ignore_routes_with_linkdown: u32,
    drop_unicast_in_l2_multicast: u32,
    drop_unsolicited_na: u32,
    keep_addr_on_down: u32,
    rtr_solicit_max_interval: u32,
    seg6_enabled: u32,
    seg6_require_hmac: u32,
    enhanced_dad: u32,
    addr_gen_mode: u8,
    disable_policy: u32,
    accept_ra_rt_info_min_plen: u32,
    ndisc_tclass: u32,
    rpl_seg_enabled: u32,
    ra_defrtr_metric: u32,
    ioam6_enabled: u32,
    ioam6_id: u32,
    ioam6_id_wide: u32,
    ndisc_evict_nocarrier: u32,
    accept_untracked_na: u32,
}

impl TypeSize for Ipv6Devconf {
    fn type_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

#[derive(Debug, Clone, FromBytes)]
pub struct Inet6Stats {
    inpkts: u64,
    inoctets: u64,
    indelivers: u64,
    outforwdatagrams: u64,
    outpkts: u64,
    outoctets: u64,
    inhdrerrors: u64,
    intoobigerrors: u64,
    innoroutes: u64,
    inaddrerrors: u64,
    inunknownprotos: u64,
    intruncatedpkts: u64,
    indiscards: u64,
    outdiscards: u64,
    outnoroutes: u64,
    reasmtimeout: u64,
    reasmreqds: u64,
    reasmoks: u64,
    reasmfails: u64,
    fragoks: u64,
    fragfails: u64,
    fragcreates: u64,
    inmcastpkts: u64,
    outmcastpkts: u64,
    inbcastpkts: u64,
    outbcastpkts: u64,
    inmcastoctets: u64,
    outmcastoctets: u64,
    inbcastoctets: u64,
    outbcastoctets: u64,
    csumerrors: u64,
    noectpkts: u64,
    ect1_pkts: u64,
    ect0_pkts: u64,
    cepkts: u64,
    reasm_overlaps: u64,
}

impl TypeSize for Inet6Stats {
    fn type_size() -> usize {
        36 * 8
    }
}

#[derive(Debug, Clone, derive_builder::Builder)]
pub struct LinkInfo {
    pub ifi_index: i32,
    pub address: MacAddr,
    pub broadcast: MacAddr,
    pub ifname: String,
    pub mtu: u32,
    pub qdisc: String,
    #[builder(default)]
    pub link: Option<u32>,
    pub stats: LinkStats,
    #[builder(default)]
    pub cost: Option<String>,
    #[builder(default)]
    pub priority: Option<String>,
    #[builder(default)]
    pub master: Option<u32>,
    pub txqlen: u32,
    pub map: LinkIfMap,
    #[builder(default)]
    pub weight: Option<u32>,
    pub operstate: u8,
    pub linkmode: u8,
    #[builder(default)]
    pub net_ns_pid: Option<u32>,
    #[builder(default)]
    pub ifalias: Option<String>,
    #[builder(default)]
    pub num_vf: Option<u32>,
    pub stats64: LinkStats64,
    pub group: u32,
    #[builder(default)]
    pub net_ns_fd: Option<u32>,
    pub promiscuity: u32,
    pub num_tx_queues: u32,
    pub num_rx_queues: u32,
    pub carrier: u8,
    pub carrier_changes: u32,
    #[builder(default)]
    pub link_netnsid: Option<i32>,
    #[builder(default)]
    pub phys_port_name: Option<String>,
    pub proto_down: u8,
    pub gso_max_segs: u32,
    pub gso_max_size: u32,
    #[builder(default)]
    pub event: Option<u32>,
    #[builder(default)]
    pub new_netnsid: Option<i32>,
    #[builder(default)]
    pub target_netnsid: Option<i32>,
    pub carrier_up_count: u32,
    pub carrier_down_count: u32,
    #[builder(default)]
    pub new_ifindex: Option<i32>,
    pub min_mtu: u32,
    pub max_mtu: u32,
    #[builder(default)]
    pub alt_ifname: Option<String>,
    #[builder(default)]
    pub perm_address: Option<MacAddr>,
    #[builder(default)]
    pub proto_down_reason: Option<String>,
    #[builder(default)]
    pub parent_dev_name: Option<String>,
    #[builder(default)]
    pub parent_dev_bus_name: Option<String>,
    #[builder(default)]
    pub gro_max_size: Option<u32>,
    #[builder(default)]
    pub tso_max_size: Option<u32>,
    #[builder(default)]
    pub tso_max_segs: Option<u32>,
    #[builder(default)]
    pub allmulti: Option<u32>,
    #[builder(default)]
    pub gso_ipv4_max_size: Option<u32>,
    #[builder(default)]
    pub gro_ipv4_max_size: Option<u32>,
}

impl NetlinkRetrievable<RoutelinkStatsError> for LinkInfo {
    async fn retrieve(netlink: &Netlink) -> Result<Vec<Self>, RoutelinkStatsError> {
        let mut recv = netlink
            .rtnl
            .send::<_, _, Rtm, ()>(
                Rtm::Getlink,
                NlmF::DUMP | NlmF::ACK,
                neli::nl::NlPayload::Payload(
                    IfinfomsgBuilder::default()
                        .ifi_family(RtAddrFamily::Inet)
                        .build()?,
                ),
            )
            .await
            .unwrap();
        let mut links = Vec::new();
        while let Some(response) = recv.next::<Rtm, Ifinfomsg>().await {
            let response = response.unwrap();
            let payload = {
                match response.nl_payload() {
                    NlPayload::Payload(x) => x,
                    _ => {
                        continue;
                    }
                }
            };

            let mut link_builder = LinkInfoBuilder::default();
            link_builder.ifi_index(*payload.ifi_index());
            let attr_handle = payload.rtattrs().get_attr_handle();
            for attr in attr_handle.iter() {
                use neli::consts::rtnl::Ifla::*;
                match attr.rta_type() {
                    Unspec => {
                        log::error!("Unspecified Value encountered when parsing Getlink result");
                    }
                    UnrecognizedConst(v) => {
                        log::info!(
                            "Unrecognized Const encountered when parsing get-link result: {v}"
                        );
                    }
                    Address => {
                        link_builder.address(
                            attr.get_payload_as()
                                .expect("There to be mac address that is valid"),
                        );
                    }
                    Broadcast => {
                        link_builder.broadcast(
                            attr.get_payload_as()
                                .expect("There to be a valid broadcast mac address"),
                        );
                    }
                    Ifname => {
                        link_builder.ifname(
                            attr.get_payload_as_with_len::<String>()
                                .expect("Ifname to be a valid string"),
                        );
                    }
                    Mtu => {
                        link_builder
                            .mtu(attr.get_payload_as::<u32>().expect("Mtu to be a valid u32"));
                    }
                    Link => {
                        link_builder.link(Some(
                            attr.get_payload_as::<u32>()
                                .expect("Link to be a valid u32"),
                        ));
                    }
                    Qdisc => {
                        link_builder.qdisc(
                            attr.get_payload_as_with_len::<String>()
                                .expect("Qdisc to be a valid string"),
                        );
                    }
                    Stats => {
                        //println!("{:?}", attr.rta_payload().len());
                        link_builder.stats(
                            attr.get_payload_as()
                                .expect("Stats to be a valid LinkStats struct"),
                        );
                    }
                    Cost => {
                        log::warn!("IFLA_COST is a nested attribute, parsing is not implemented");
                    }
                    Priority => {
                        link_builder.priority(Some(
                            attr.get_payload_as::<u32>()
                                .expect("Priority to be a valid u32")
                                .to_string(),
                        ));
                    }
                    Master => {
                        link_builder.master(Some(
                            attr.get_payload_as::<u32>()
                                .expect("Master to be a valid u32"),
                        ));
                    }
                    Wireless => {
                        log::warn!(
                            "IFLA_WIRELESS is a nested attribute, parsing is not implemented"
                        );
                    }
                    Protinfo => {
                        log::warn!(
                            "IFLA_PROTINFO is a nested attribute, parsing is not implemented"
                        );
                    }
                    Txqlen => {
                        link_builder.txqlen(
                            attr.get_payload_as::<u32>()
                                .expect("Txqlen to be a valid u32"),
                        );
                    }
                    Map => {
                        link_builder.map(
                            attr.get_payload_as()
                                .expect("Map to be a valid LinkIfMap struct"),
                        );
                    }
                    Weight => {
                        link_builder.weight(Some(
                            attr.get_payload_as::<u32>()
                                .expect("Weight to be a valid u32"),
                        ));
                    }
                    Operstate => {
                        link_builder.operstate(
                            attr.get_payload_as::<u8>()
                                .expect("Operstate to be a valid u8"),
                        );
                    }
                    Linkmode => {
                        link_builder.linkmode(
                            attr.get_payload_as::<u8>()
                                .expect("Linkmode to be a valid u8"),
                        );
                    }
                    Linkinfo => {
                        log::warn!(
                            "IFLA_LINKINFO is a complex nested attribute, full parsing is not implemented here."
                        );
                    }
                    NetNsPid => {
                        link_builder.net_ns_pid(Some(
                            attr.get_payload_as::<u32>()
                                .expect("NetNsPid to be a valid u32"),
                        ));
                    }
                    Ifalias => {
                        link_builder.ifalias(Some(
                            attr.get_payload_as_with_len::<String>()
                                .expect("Ifalias to be a valid string"),
                        ));
                    }
                    NumVf => {
                        link_builder.num_vf(Some(
                            attr.get_payload_as::<u32>()
                                .expect("NumVf to be a valid u32"),
                        ));
                    }
                    VfinfoList => {
                        log::warn!(
                            "IFLA_VFINFO_LIST is a nested attribute, parsing is not implemented"
                        );
                    }
                    Stats64 => {
                        link_builder.stats64(
                            attr.get_payload_as()
                                .expect("Stats64 to be a valid LinkStats64 struct"),
                        );
                    }
                    VfPorts => {
                        log::warn!(
                            "IFLA_VF_PORTS is a nested attribute, parsing is not implemented"
                        );
                    }
                    PortSelf => {
                        log::warn!(
                            "IFLA_PORT_SELF is a nested attribute, parsing is not implemented"
                        );
                    }
                    AfSpec => {
                        log::warn!(
                            "IFLA_AF_SPEC is a nested attribute, parsing is not implemented"
                        );
                    }
                    Group => {
                        link_builder.group(
                            attr.get_payload_as::<u32>()
                                .expect("Group to be a valid u32"),
                        );
                    }
                    NetNsFd => {
                        link_builder.net_ns_fd(Some(
                            attr.get_payload_as::<u32>()
                                .expect("NetNsFd to be a valid u32"),
                        ));
                    }
                    ExtMask => {
                        log::debug!("Skipping IFLA_EXT_MASK attribute");
                    }
                    Promiscuity => {
                        link_builder.promiscuity(
                            attr.get_payload_as::<u32>()
                                .expect("Promiscuity to be a valid u32"),
                        );
                    }
                    NumTxQueues => {
                        link_builder.num_tx_queues(
                            attr.get_payload_as::<u32>()
                                .expect("NumTxQueues to be a valid u32"),
                        );
                    }
                    NumRxQueues => {
                        link_builder.num_rx_queues(
                            attr.get_payload_as::<u32>()
                                .expect("NumRxQueues to be a valid u32"),
                        );
                    }
                    Carrier => {
                        link_builder.carrier(
                            attr.get_payload_as::<u8>()
                                .expect("Carrier to be a valid u8"),
                        );
                    }
                    PhysPortId => {
                        log::debug!("Skipping IFLA_PHYS_PORT_ID attribute");
                    }
                    CarrierChanges => {
                        link_builder.carrier_changes(
                            attr.get_payload_as::<u32>()
                                .expect("CarrierChanges to be a valid u32"),
                        );
                    }
                    PhysSwitchId => {
                        log::debug!("Skipping IFLA_PHYS_SWITCH_ID attribute");
                    }
                    LinkNetnsid => {
                        link_builder.link_netnsid(Some(
                            attr.get_payload_as::<i32>()
                                .expect("LinkNetnsid to be a valid i32"),
                        ));
                    }
                    PhysPortName => {
                        link_builder.phys_port_name(Some(
                            attr.get_payload_as_with_len::<String>()
                                .expect("PhysPortName to be a valid string"),
                        ));
                    }
                    ProtoDown => {
                        link_builder.proto_down(
                            attr.get_payload_as::<u8>()
                                .expect("ProtoDown to be a valid u8"),
                        );
                    }
                    GsoMaxSegs => {
                        link_builder.gso_max_segs(
                            attr.get_payload_as::<u32>()
                                .expect("GsoMaxSegs to be a valid u32"),
                        );
                    }
                    GsoMaxSize => {
                        link_builder.gso_max_size(
                            attr.get_payload_as::<u32>()
                                .expect("GsoMaxSize to be a valid u32"),
                        );
                    }
                    Pad => { /* Padding attribute, ignored */ }
                    Xdp => {
                        log::warn!("IFLA_XDP is a nested attribute, parsing is not implemented");
                    }
                    Event => {
                        link_builder.event(Some(
                            attr.get_payload_as::<u32>()
                                .expect("Event to be a valid u32"),
                        ));
                    }
                    NewNetnsid => {
                        link_builder.new_netnsid(Some(
                            attr.get_payload_as::<i32>()
                                .expect("NewNetnsid to be a valid i32"),
                        ));
                    }
                    IfNetnsid => {
                        link_builder.target_netnsid(Some(
                            attr.get_payload_as::<i32>()
                                .expect("IfNetnsid to be a valid i32"),
                        ));
                    }
                    CarrierUpCount => {
                        link_builder.carrier_up_count(
                            attr.get_payload_as::<u32>()
                                .expect("CarrierUpCount to be a valid u32"),
                        );
                    }
                    CarrierDownCount => {
                        link_builder.carrier_down_count(
                            attr.get_payload_as::<u32>()
                                .expect("CarrierDownCount to be a valid u32"),
                        );
                    }
                    NewIfindex => {
                        link_builder.new_ifindex(Some(
                            attr.get_payload_as::<i32>()
                                .expect("NewIfindex to be a valid i32"),
                        ));
                    }
                    MinMtu => {
                        link_builder.min_mtu(
                            attr.get_payload_as::<u32>()
                                .expect("MinMtu to be a valid u32"),
                        );
                    }
                    MaxMtu => {
                        link_builder.max_mtu(
                            attr.get_payload_as::<u32>()
                                .expect("MaxMtu to be a valid u32"),
                        );
                    }
                    PropList => {
                        log::warn!(
                            "IFLA_PROP_LIST is a nested attribute, parsing is not implemented"
                        );
                    }
                    AltIfname => {
                        link_builder.alt_ifname(Some(
                            attr.get_payload_as_with_len::<String>()
                                .expect("AltIfname to be a valid string"),
                        ));
                    }
                    PermAddress => {
                        link_builder.perm_address(
                            Some( attr.get_payload_as()
                                .expect("PermAddress to be a valid mac address"), )
                        );
                    }
                    ProtoDownReason => {
                        log::warn!(
                            "IFLA_PROTODOWN_REASON is a nested attribute, parsing is not implemented"
                        );
                    }
                    IflaDevlinkPort => {
                        log::warn!(
                            "IFLA_DEVLINK_PORT is a nested attribute, parsing is not implemented"
                        );
                    }
                    IflaGsoIpv4MaxSize => {
                        link_builder.gso_ipv4_max_size(Some(
                            attr.get_payload_as::<u32>()
                                .expect("IflaGsoIpv4MaxSize to be a valid u32"),
                        ));
                    }
                    IflaGroIpv4MaxSize => {
                        link_builder.gro_ipv4_max_size(Some(
                            attr.get_payload_as::<u32>()
                                .expect("IflaGroIpv4MaxSize to be a valid u32"),
                        ));
                    }
                    IflaDpllPin => {
                        log::warn!("IFLA_DPLL_PIN parsing is not implemented");
                    }
                    IflaMaxPacingOffloadHorizon => {
                        log::warn!("IFLA_MAX_PACING_OFFLOAD_HORIZON parsing is not implemented");
                    }
                    IflaNetnsImmutable => {
                        log::warn!("IFLA_NETNS_IMMUTABLE parsing is not implemented");
                    }
                    IflaParentDevName => {
                        link_builder.parent_dev_name(Some(
                            attr.get_payload_as_with_len::<String>()
                                .expect("IflaParentDevName to be a valid String"),
                        ));
                    }
                    IflaParentDevBusName => {
                        link_builder.parent_dev_bus_name(Some(
                            attr.get_payload_as_with_len::<String>()
                                .expect("IflaParentDevBusName to be a valid String"),
                        ));
                    }
                    IflaGroMaxSize => {
                        link_builder.gro_max_size(Some(
                            attr.get_payload_as::<u32>()
                                .expect("IflaGroMaxSize to be a valid u32"),
                        ));
                    }
                    IflaTsoMaxSize => {
                        link_builder.tso_max_size(Some(
                            attr.get_payload_as::<u32>()
                                .expect("IflaTsoMaxSize to be a valid u32"),
                        ));
                    }
                    IflaTsoMaxSegs => {
                        link_builder.tso_max_segs(Some(
                            attr.get_payload_as::<u32>()
                                .expect("IflaTsoMaxSegs to be a valid u32"),
                        ));
                    }
                    IflaAllmulti => {
                        link_builder.allmulti(Some(
                            attr.get_payload_as::<u32>()
                                .expect("IflaAllmulti to be a valid u32"),
                        ));
                    }
                }
            }
            match link_builder.build() {
                Ok(link) => {
                    links.push(link);
                }
                Err(e) => {
                    log::error!("{e:?}")
                }
            }
        }
        Ok(links)
    }
}
