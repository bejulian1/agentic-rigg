//! Simulert nettverk til tester og skjermbilder (`--backend sim`): kablet
//! Ethernet med standardruten, Wi-Fi som også er tilkoblet, en Docker-bro,
//! et veth-par og loopback. Adressene er dokumentasjonsadresser (192.0.2.0/24
//! og 2001:db8::/32) og MAC-adressene er lokalt administrerte, så ingenting
//! kan forveksles med en ekte maskin. Ratene går i rolige bølger med
//! nedlastingsbyger, så grafene har noe å vise.

use rigg_core::model::NetSample;
use rigg_core::net::{Dns, Gateway, IpAddr, Listener, NetCounters, NetIface, NetInfo, Routes, Sockets, Wifi};
use std::collections::BTreeMap;

/// Navnene på de simulerte grensesnittene.
pub const NAMES: [&str; 5] = ["enp5s0", "wlp4s0", "docker0", "veth3f2a1c", "lo"];

fn v4(a: &str, p: u8) -> IpAddr {
    IpAddr { addr: a.into(), prefix: p, scope: if a.starts_with("127.") { "host" } else { "global" }.into(), temporary: false }
}

fn v6(a: &str, p: u8, scope: &str, temporary: bool) -> IpAddr {
    IpAddr { addr: a.into(), prefix: p, scope: scope.into(), temporary }
}

/// Ratene (byte/s) for et grensesnitt ved tiden `s` (sekunder siden start).
pub fn rates(name: &str, s: f64) -> Option<(f64, f64)> {
    let wave = |p: f64, ph: f64| 0.5 + 0.5 * (s / p + ph).sin();
    Some(match name {
        "enp5s0" => {
            // En nedlasting på ~40 s hvert 3. minutt, ellers surfing og strømming.
            let burst = if (s % 180.0) < 40.0 { 38e6 * (0.85 + 0.15 * (s * 0.9).sin()) } else { 0.0 };
            (420e3 + 1.6e6 * wave(23.0, 0.0) + burst, 60e3 + 180e3 * wave(17.0, 1.3) + burst * 0.02)
        }
        "wlp4s0" => (12e3 + 30e3 * wave(31.0, 2.0), 4e3 + 9e3 * wave(29.0, 0.4)),
        "docker0" => (0.0, 0.0),
        "veth3f2a1c" => (2e3 * wave(11.0, 0.0), 1.5e3 * wave(13.0, 0.0)),
        _ => return None,
    })
}

/// Erstatter de ekte nettkortene i prøven med de simulerte (loopback er ikke med, som i `net.rs`).
pub fn overlay(net: &mut BTreeMap<String, NetSample>, start: i64, now: i64) {
    let s = (now - start).max(0) as f64 / 1000.0;
    net.clear();
    for n in NAMES.iter().filter(|n| **n != "lo") {
        let (rx, tx) = rates(n, s).unwrap_or_default();
        let up = *n != "docker0";
        net.insert(
            n.to_string(),
            NetSample { rx: Some(rx), tx: Some(tx), state: Some(if up { "up" } else { "down" }.into()), wireless: *n == "wlp4s0" },
        );
    }
}

fn counters(rx: u64, tx: u64, extra_s: f64, avg: (f64, f64), errors: u64, dropped: u64) -> NetCounters {
    let rx = rx + (avg.0 * extra_s) as u64;
    let tx = tx + (avg.1 * extra_s) as u64;
    NetCounters {
        rx_bytes: Some(rx),
        tx_bytes: Some(tx),
        rx_packets: Some(rx / 1100),
        tx_packets: Some(tx / 380),
        rx_errors: Some(errors),
        tx_errors: Some(0),
        rx_dropped: Some(dropped),
        tx_dropped: Some(0),
        multicast: Some(rx / 900_000),
        collisions: Some(0),
    }
}

/// Hele svaret på `get_network` med simulerte data.
pub fn info(start: i64, now: i64) -> NetInfo {
    let s = (now - start).max(0) as f64 / 1000.0;
    let eth = NetIface {
        name: "enp5s0".into(),
        kind: "ethernet".into(),
        operstate: "up".into(),
        carrier: Some(true),
        mac: Some("02:00:5E:10:00:2A".into()),
        mtu: Some(1500),
        speed_mbps: Some(1000),
        duplex: Some("full".into()),
        ipv4: vec![v4("192.0.2.42", 24)],
        ipv6: vec![v6("2001:db8:42::2a", 64, "global", false), v6("2001:db8:42::7c1e:93ab", 64, "global", true), v6("fe80::5eff:fe10:2a", 64, "link", false)],
        driver: Some("igb".into()),
        bus: Some("PCI".into()),
        bus_addr: Some("0000:05:00.0".into()),
        ids: Some("8086:1539".into()),
        vendor: Some("Intel".into()),
        model: Some("I211 Gigabit Network Connection".into()),
        connection: Some("LAN".into()),
        firmware: Some("0.6-1".into()),
        default_route: true,
        stats: counters(48_200_000_000, 3_900_000_000, s, (2.6e6, 0.2e6), 0, 37),
        wifi: None,
    };
    let wifi = NetIface {
        name: "wlp4s0".into(),
        kind: "wifi".into(),
        operstate: "up".into(),
        carrier: Some(true),
        mac: Some("02:00:5E:10:00:3B".into()),
        mtu: Some(1500),
        ipv4: vec![v4("192.0.2.77", 24)],
        ipv6: vec![v6("fe80::5eff:fe10:3b", 64, "link", false)],
        driver: Some("iwlwifi".into()),
        bus: Some("PCI".into()),
        bus_addr: Some("0000:04:00.0".into()),
        ids: Some("8086:2526".into()),
        vendor: Some("Intel".into()),
        model: Some("Wireless-AC 9260".into()),
        connection: Some("Rigg-lab".into()),
        firmware: Some("46.fae53a8b.0".into()),
        stats: counters(1_240_000_000, 310_000_000, s, (27e3, 8.5e3), 0, 4),
        wifi: Some(Wifi {
            ssid: Some("Rigg-lab".into()),
            bssid: Some("02:00:5E:AA:01:24".into()),
            signal_dbm: Some(-58.0 + 3.0 * (s / 40.0).sin()),
            signal_pct: Some(84.0),
            freq_mhz: Some(5180),
            bitrate_mbps: Some(866.7),
            security: Some("wpa3".into()),
        }),
        ..Default::default()
    };
    let docker = NetIface {
        name: "docker0".into(),
        kind: "bridge".into(),
        operstate: "down".into(),
        carrier: Some(false),
        mac: Some("02:42:AC:11:00:01".into()),
        mtu: Some(1500),
        ipv4: vec![v4("172.17.0.1", 16)],
        stats: counters(0, 0, 0.0, (0.0, 0.0), 0, 0),
        ..Default::default()
    };
    let veth = NetIface {
        name: "veth3f2a1c".into(),
        kind: "virtual".into(),
        operstate: "up".into(),
        carrier: Some(true),
        mac: Some("02:5A:1C:3F:2A:1C".into()),
        mtu: Some(1500),
        ipv6: vec![v6("fe80::5a1c:3fff:fe2a:1c", 64, "link", false)],
        stats: counters(18_000_000, 9_000_000, s, (1e3, 0.75e3), 0, 0),
        ..Default::default()
    };
    let lo = NetIface {
        name: "lo".into(),
        kind: "loopback".into(),
        operstate: "unknown".into(),
        carrier: Some(true),
        mtu: Some(65536),
        ipv4: vec![v4("127.0.0.1", 8)],
        ipv6: vec![v6("::1", 128, "host", false)],
        stats: counters(52_000_000, 52_000_000, s, (2e3, 2e3), 0, 0),
        ..Default::default()
    };
    let listener = |proto: &str, family: u8, addr: &str, port: u16, user: &str, process: Option<(&str, u32)>| Listener {
        proto: proto.into(),
        family,
        addr: addr.into(),
        port,
        uid: Some(if user == "root" { 0 } else { 1000 }),
        user: Some(user.into()),
        pid: process.map(|p| p.1),
        process: process.map(|p| p.0.into()),
        inode: 0,
    };
    NetInfo {
        t: now,
        source: "sim".into(),
        nm: true,
        hostname: Some("rigg-sim".into()),
        ifaces: vec![eth, wifi, veth, docker, lo],
        routes: Routes {
            gateway4: Some(Gateway { addr: "192.0.2.1".into(), iface: "enp5s0".into(), metric: 100 }),
            gateway6: Some(Gateway { addr: "fe80::1".into(), iface: "enp5s0".into(), metric: 100 }),
        },
        dns: Dns { servers: vec!["192.0.2.1".into(), "2001:db8:42::1".into()], domains: vec!["lab.example".into()], source: Some("networkmanager".into()), stub: Some("127.0.0.53".into()) },
        sockets: Sockets {
            tcp_established: 23 + ((s / 20.0).sin() * 3.0).round() as u32,
            tcp_listen: 6,
            tcp_other: 4,
            udp: 9,
            listeners: vec![
                listener("tcp", 4, "127.0.0.53", 53, "systemd-resolve", None),
                listener("tcp", 4, "0.0.0.0", 22, "root", None),
                listener("tcp", 6, "::", 22, "root", None),
                listener("tcp", 4, "127.0.0.1", 631, "root", None),
                listener("tcp", 4, "127.0.0.1", 8384, "rigg", Some(("syncthing", 2290))),
                listener("tcp", 6, "::1", 8080, "rigg", Some(("python3", 41207))),
                listener("udp", 4, "0.0.0.0", 5353, "avahi", None),
                listener("udp", 6, "::", 5353, "avahi", None),
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulert_nett_henger_sammen() {
        let i = info(0, 60_000);
        assert_eq!(i.source, "sim");
        assert_eq!(rigg_core::net::primary(&i).map(|p| p.name.as_str()), Some("enp5s0"));
        assert!(i.ifaces.iter().all(|f| NAMES.contains(&f.name.as_str())));
        let later = info(0, 120_000);
        assert!(later.ifaces[0].stats.rx_bytes > i.ifaces[0].stats.rx_bytes, "tellerne øker");
        let mut net = BTreeMap::new();
        net.insert("wlo1".to_string(), NetSample::default());
        overlay(&mut net, 0, 10_000);
        assert!(!net.contains_key("wlo1") && !net.contains_key("lo"));
        assert!(net["enp5s0"].rx.unwrap() > 30e6, "byge i starten");
        assert!(net["wlp4s0"].wireless);
        assert_eq!(rates("lo", 1.0), None);
    }
}
