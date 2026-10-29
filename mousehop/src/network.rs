//! Local network identity + interface classification.
//!
//! Two jobs, both backed by `netdev` (cross-platform):
//!
//! 1. **Network fingerprint** ([`NetworkSnapshot::network_id`]) — a stable,
//!    opaque id for "which LAN am I on right now", used to scope
//!    per-network address locks (a lock set at home shouldn't pin a
//!    home IP at a coffee shop). The default-gateway MAC is the
//!    primary signal: it survives DHCP lease changes and distinguishes
//!    two different routers that both hand out `192.168.1.0/24`. Falls
//!    back to the default interface's subnet when no gateway MAC is
//!    available (point-to-point links, some VPNs).
//!
//! 2. **Interface advertisement** ([`NetworkSnapshot::addresses`]) — the
//!    set of physical-interface addresses this host should advertise
//!    over mDNS, each tagged Wired/Wi-Fi so peers can label them. The
//!    interface *type* is invisible from L3, so the owner is the only
//!    one who can classify it.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use mousehop_ipc::IfaceKind;
use netdev::interface::types::InterfaceType;

/// Slow OS queries must never run on the input executor (Windows may
/// resolve gateway MACs with synchronous ARP). One job per owner, with
/// cancellation-safe polling across select! iterations and no backlog.
pub(crate) struct BackgroundRefresh<T> {
    job: Option<tokio::task::JoinHandle<T>>,
}

impl<T: Send + 'static> BackgroundRefresh<T> {
    pub(crate) fn new() -> Self {
        Self { job: None }
    }

    pub(crate) fn request(&mut self, query: impl FnOnce() -> T + Send + 'static) {
        if self.job.is_none() {
            self.job = Some(tokio::task::spawn_blocking(query));
        }
    }

    pub(crate) async fn event(&mut self) -> Result<T, tokio::task::JoinError> {
        let result = match self.job.as_mut() {
            Some(job) => job.await,
            None => std::future::pending().await,
        };
        self.job.take();
        result
    }
}

impl<T> Drop for BackgroundRefresh<T> {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            job.abort();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NetworkSnapshot {
    pub(crate) network_id: Option<String>,
    pub(crate) primary: Option<Ipv4Addr>,
    pub(crate) addresses: Vec<IpAddr>,
    pub(crate) kinds: HashMap<IpAddr, IfaceKind>,
}

impl NetworkSnapshot {
    /// Only call on a blocking worker. Share this result between discovery
    /// and network locks rather than repeatedly enumerating for each field.
    pub(crate) fn query() -> Result<Self, &'static str> {
        let interfaces = netdev::get_interfaces();
        if interfaces.is_empty() {
            return Err("interface enumeration returned no interfaces");
        }
        let default = default_interface(&interfaces, netdev::net::ip::get_local_ipaddr()).cloned();
        Ok(Self::from_interfaces(interfaces, default))
    }

    fn from_interfaces(
        interfaces: Vec<netdev::Interface>,
        default: Option<netdev::Interface>,
    ) -> Self {
        let mut snapshot = Self::default();
        if let Some(iface) = default {
            snapshot.network_id = iface
                .gateway
                .as_ref()
                .and_then(|gw| {
                    let mac = gw.mac_addr.to_string();
                    (!mac.is_empty() && mac != "00:00:00:00:00:00").then(|| format!("gw:{mac}"))
                })
                .or_else(|| {
                    iface
                        .ipv4
                        .first()
                        .map(|net| format!("net:{}/{}", net.network(), net.prefix_len()))
                });
            snapshot.primary = iface.ipv4.first().map(|net| net.addr());
            if let Some(primary) = snapshot.primary {
                snapshot
                    .kinds
                    .insert(IpAddr::V4(primary), iface_kind(iface.if_type));
            }
        }
        for iface in interfaces {
            let kind = iface_kind(iface.if_type);
            if kind == IfaceKind::Other || is_virtual_name(&iface.name) {
                continue;
            }
            let v4 = iface.ipv4.iter().map(|n| IpAddr::V4(n.addr()));
            let v6 = iface.ipv6.iter().map(|n| IpAddr::V6(n.addr()));
            for ip in v4.chain(v6).filter(is_routable_ip) {
                snapshot.addresses.push(ip);
                snapshot.kinds.insert(ip, kind);
            }
        }
        if let Some(primary) = snapshot.primary {
            if !snapshot.addresses.contains(&IpAddr::V4(primary)) {
                snapshot.addresses.push(IpAddr::V4(primary));
            }
        }
        snapshot.addresses.sort();
        snapshot.addresses.dedup();
        snapshot
    }
}

fn default_interface(
    interfaces: &[netdev::Interface],
    local_ip: Option<IpAddr>,
) -> Option<&netdev::Interface> {
    if let Some(iface) = interfaces.iter().find(|iface| iface.default) {
        return Some(iface);
    }
    let ip = local_ip?;
    let exact = interfaces.iter().find(|iface| match ip {
        IpAddr::V4(ip) => iface.ipv4.iter().any(|net| net.addr() == ip),
        IpAddr::V6(ip) => iface.ipv6.iter().any(|net| net.addr() == ip),
    });
    exact.or_else(|| {
        interfaces.iter().find(|iface| match ip {
            IpAddr::V4(ip) => iface.ipv4.iter().any(|net| net.contains(&ip)),
            IpAddr::V6(ip) => iface.ipv6.iter().any(|net| net.contains(&ip)),
        })
    })
}

/// Map a `netdev` interface type to the coarse kind we surface to
/// users. Everything that isn't clearly Ethernet or Wi-Fi (loopback,
/// VPN/tunnel, bridge, cellular, unknown) collapses to `Other`.
pub(crate) fn iface_kind(t: InterfaceType) -> IfaceKind {
    match t {
        InterfaceType::Ethernet
        | InterfaceType::Ethernet3Megabit
        | InterfaceType::FastEthernetT
        | InterfaceType::FastEthernetFx
        | InterfaceType::GigabitEthernet => IfaceKind::Wired,
        InterfaceType::Wireless80211 | InterfaceType::PeerToPeerWireless => IfaceKind::WiFi,
        _ => IfaceKind::Other,
    }
}

/// Virtual-interface name prefixes to never advertise — container
/// bridges, libvirt, VM host-only nets, raw tun/tap. These often
/// present as Ethernet at the link layer (so the type check alone
/// misses them) yet carry host-local addresses no LAN peer can reach.
const VIRTUAL_IFACE_PREFIXES: &[&str] = &[
    "docker", "br-", "virbr", "veth", "vmnet", "vboxnet", "tap", "tun", "utun", "zt",
];

fn is_virtual_name(name: &str) -> bool {
    VIRTUAL_IFACE_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// True for addresses worth advertising / dialing: globally- or
/// LAN-routable unicast. Excludes loopback, unspecified, and link-local
/// (IPv4 169.254/16 and IPv6 `fe80::/10` — the latter needs a scope id
/// we can't carry, see `dns::is_unusable_candidate`).
pub(crate) fn is_routable_ip(ip: &IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local(),
        IpAddr::V6(v6) => !is_v6_link_local(v6),
    }
}

fn is_v6_link_local(v6: &Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test(flavor = "current_thread")]
    async fn slow_refresh_does_not_block_dispatch_or_spawn_backlog() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
                mpsc,
            },
            time::Duration,
        };

        let mut refresh = BackgroundRefresh::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        refresh.request(move || {
            worker_calls.fetch_add(1, Ordering::SeqCst);
            let _ = started_tx.send(());
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            42
        });
        started_rx.await.unwrap();
        for _ in 0..100 {
            let calls = calls.clone();
            refresh.request(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                99
            });
        }
        // Simulate another select! branch winning while enumeration is stuck.
        // Cancelling this await must leave the same job tracked, not detach it.
        assert!(
            tokio::time::timeout(Duration::from_millis(30), refresh.event())
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release_tx.send(()).unwrap();
        assert_eq!(refresh.event().await.unwrap(), 42);
        refresh.request(|| 7);
        assert_eq!(refresh.event().await.unwrap(), 7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_worker_can_be_retried() {
        let mut refresh = BackgroundRefresh::<u8>::new();
        refresh.request(|| panic!("injected query failure"));
        assert!(refresh.event().await.is_err());
        refresh.request(|| 1);
        assert_eq!(refresh.event().await.unwrap(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "live OS interface-query scheduling probe"]
    async fn live_refresh_keeps_executor_responsive() {
        use std::time::{Duration, Instant};

        let mut refresh = BackgroundRefresh::new();
        for round in 1..=3 {
            let started = Instant::now();
            refresh.request(NetworkSnapshot::query);
            let mut heartbeat = tokio::time::interval(Duration::from_millis(10));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last_tick = Instant::now();
            let mut max_gap = Duration::ZERO;
            loop {
                tokio::select! {
                    result = refresh.event() => {
                        result.unwrap().unwrap();
                        break;
                    }
                    _ = heartbeat.tick() => {
                        let now = Instant::now();
                        max_gap = max_gap.max(now.duration_since(last_tick));
                        last_tick = now;
                    }
                }
            }
            max_gap = max_gap.max(last_tick.elapsed());
            eprintln!(
                "network round={round} query={:?} max_executor_gap={max_gap:?}",
                started.elapsed()
            );
            assert!(
                max_gap < Duration::from_millis(250),
                "interface query blocked the executor"
            );
        }
    }

    #[test]
    fn snapshot_preserves_default_subnet_and_labels_all_physical_addresses() {
        let mut wired = netdev::Interface::dummy();
        wired.name = "en6".into();
        wired.if_type = InterfaceType::Ethernet;
        wired.ipv4.push("192.168.1.2/24".parse().unwrap());
        let mut wifi = wired.clone();
        wifi.name = "en0".into();
        wifi.if_type = InterfaceType::Wireless80211;
        wifi.ipv4 = vec!["192.168.1.3/24".parse().unwrap()];
        let mut virtual_iface = wired.clone();
        virtual_iface.name = "docker0".into();
        virtual_iface.ipv4 = vec!["172.16.0.1/16".parse().unwrap()];
        let snapshot =
            NetworkSnapshot::from_interfaces(vec![wifi, virtual_iface, wired.clone()], Some(wired));
        assert_eq!(snapshot.network_id.as_deref(), Some("net:192.168.1.0/24"));
        assert_eq!(snapshot.primary, Some(Ipv4Addr::new(192, 168, 1, 2)));
        assert_eq!(snapshot.addresses.len(), 2);
        assert_eq!(
            snapshot.kinds[&"192.168.1.2".parse().unwrap()],
            IfaceKind::Wired
        );
        assert_eq!(
            snapshot.kinds[&"192.168.1.3".parse().unwrap()],
            IfaceKind::WiFi
        );
    }

    #[test]
    fn default_vpn_address_remains_advertised_and_offline_snapshot_is_empty() {
        let mut vpn = netdev::Interface::dummy();
        vpn.name = "utun0".into();
        vpn.if_type = InterfaceType::Tunnel;
        vpn.ipv4.push("10.1.0.2/24".parse().unwrap());
        let snapshot = NetworkSnapshot::from_interfaces(vec![vpn.clone()], Some(vpn));
        assert_eq!(
            snapshot.addresses,
            vec!["10.1.0.2".parse::<IpAddr>().unwrap()]
        );
        assert_eq!(snapshot.kinds[&snapshot.addresses[0]], IfaceKind::Other);
        assert_eq!(
            NetworkSnapshot::from_interfaces(vec![], None),
            NetworkSnapshot::default()
        );
    }

    #[test]
    fn default_selection_prefers_exact_address_over_first_matching_subnet() {
        let mut first = netdev::Interface::dummy();
        first.index = 1;
        first.ipv4.push("192.168.1.2/24".parse().unwrap());
        let mut second = first.clone();
        second.index = 2;
        second.ipv4 = vec!["192.168.1.3/24".parse().unwrap()];
        let interfaces = vec![first, second];
        assert_eq!(
            default_interface(&interfaces, Some("192.168.1.3".parse().unwrap()))
                .unwrap()
                .index,
            2
        );
        assert!(default_interface(&interfaces, None).is_none());
    }

    #[test]
    fn classifies_interface_types() {
        assert_eq!(iface_kind(InterfaceType::GigabitEthernet), IfaceKind::Wired);
        assert_eq!(iface_kind(InterfaceType::Wireless80211), IfaceKind::WiFi);
        assert_eq!(iface_kind(InterfaceType::Tunnel), IfaceKind::Other);
        assert_eq!(iface_kind(InterfaceType::Loopback), IfaceKind::Other);
        assert_eq!(iface_kind(InterfaceType::Bridge), IfaceKind::Other);
    }

    #[test]
    fn routable_filter_excludes_loopback_and_link_local() {
        assert!(is_routable_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5))));
        assert!(!is_routable_ip(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_routable_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
        assert!(!is_routable_ip(&"fe80::1".parse::<IpAddr>().unwrap()));
        assert!(is_routable_ip(&"fdb7:7db7::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn virtual_names_are_skipped() {
        assert!(is_virtual_name("docker0"));
        assert!(is_virtual_name("br-1a2b3c"));
        assert!(is_virtual_name("veth9f8e"));
        assert!(!is_virtual_name("enp191s0"));
        assert!(!is_virtual_name("wlan0"));
    }
}
