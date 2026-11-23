//! Nettverk (Nettverk-siden): datatypene tjenesten sender i `get_network`,
//! og tolkingen av `/proc/net/*`, `resolv.conf` og Wi-Fi-tallene, som er ren
//! tekst inn og verdier ut. Lesingen står i `rigg-hw/src/sys/netinfo.rs`.
//!
//! Alt her er maskinverdier: `kind`, `operstate` og `security` er stabile
//! nøkler som vinduet oversetter ved visning. Ingenting her skriver noe eller
//! snakker med nettet.

use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};

/// Svaret på `get_network`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetInfo {
    /// Når lista ble laget (ms).
    pub t: i64,
    /// `sysfs` (ekte) eller `sim`.
    pub source: String,
    /// NetworkManager svarte på systembussen.
    pub nm: bool,
    pub hostname: Option<String>,
    pub ifaces: Vec<NetIface>,
    pub routes: Routes,
    pub dns: Dns,
    pub sockets: Sockets,
}

/// Ett grensesnitt.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetIface {
    pub name: String,
    /// `ethernet`, `wifi`, `mobile`, `loopback`, `bridge`, `vpn` (tun/wg),
    /// `virtual` (veth, docker, tap …) eller `other`.
    pub kind: String,
    /// `operstate`: `up`, `down`, `dormant`, `lowerlayerdown`, `unknown` …
    pub operstate: String,
    /// `carrier`: kabel eller tilknytning finnes. `None` når grensesnittet er nede.
    pub carrier: Option<bool>,
    pub mac: Option<String>,
    pub mtu: Option<u32>,
    /// Lenkehastighet i Mbit/s (`speed`; -1 og ukjent blir `None`).
    pub speed_mbps: Option<u32>,
    /// `full`, `half` eller `None`.
    pub duplex: Option<String>,
    pub ipv4: Vec<IpAddr>,
    pub ipv6: Vec<IpAddr>,
    pub driver: Option<String>,
    /// `PCI`, `USB`, `SDIO` eller `None` (virtuelle).
    pub bus: Option<String>,
    /// PCI-adressen («0000:18:00.0») eller USB-stien.
    pub bus_addr: Option<String>,
    /// «8086:1539».
    pub ids: Option<String>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    /// Navnet på tilkoblingen i NetworkManager («Hjemme», «Kablet tilkobling 1»).
    pub connection: Option<String>,
    pub firmware: Option<String>,
    /// Standardruten går via dette grensesnittet.
    pub default_route: bool,
    pub stats: NetCounters,
    pub wifi: Option<Wifi>,
}

impl NetIface {
    /// Fysiske kort (Ethernet, Wi-Fi, mobilnett) og ukjente med maskinvare.
    pub fn physical(&self) -> bool {
        matches!(self.kind.as_str(), "ethernet" | "wifi" | "mobile") || (self.kind == "other" && self.bus.is_some())
    }

    /// Oppe med forbindelse (operstate `up`, eller `unknown` med adresse, som tun).
    pub fn connected(&self) -> bool {
        match self.operstate.as_str() {
            "up" => self.carrier != Some(false),
            "unknown" => self.carrier != Some(false) && (!self.ipv4.is_empty() || self.ipv6.iter().any(|a| a.scope == "global")),
            _ => false,
        }
    }
}

/// Én adresse med prefiks.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct IpAddr {
    pub addr: String,
    pub prefix: u8,
    /// `global`, `link`, `host` eller `site`.
    pub scope: String,
    /// IPv6: midlertidig adresse (personvern).
    pub temporary: bool,
}

impl IpAddr {
    /// «192.0.2.42/24».
    pub fn cidr(&self) -> String {
        format!("{}/{}", self.addr, self.prefix)
    }
}

/// Tellerne i `statistics/` (siden grensesnittet kom opp).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetCounters {
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
    pub rx_packets: Option<u64>,
    pub tx_packets: Option<u64>,
    pub rx_errors: Option<u64>,
    pub tx_errors: Option<u64>,
    pub rx_dropped: Option<u64>,
    pub tx_dropped: Option<u64>,
    pub multicast: Option<u64>,
    pub collisions: Option<u64>,
}

/// Wi-Fi-tilknytningen. Alt er valgfritt: uten NetworkManager finnes bare
/// signalet fra `/proc/net/wireless`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Wifi {
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub signal_dbm: Option<f64>,
    /// Signalstyrke 0–100 % (NetworkManager, ellers kvaliteten i `/proc/net/wireless`).
    pub signal_pct: Option<f64>,
    pub freq_mhz: Option<u32>,
    /// Nåværende bitrate i Mbit/s.
    pub bitrate_mbps: Option<f64>,
    /// `open`, `wep`, `wpa`, `wpa2`, `wpa3`, `wpa2-enterprise`, `wpa3-enterprise`, `owe`.
    pub security: Option<String>,
}

/// Standardrutene.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Routes {
    pub gateway4: Option<Gateway>,
    pub gateway6: Option<Gateway>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Gateway {
    pub addr: String,
    pub iface: String,
    pub metric: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Dns {
    pub servers: Vec<String>,
    pub domains: Vec<String>,
    /// `networkmanager`, `resolved` (`/run/systemd/resolve/resolv.conf`) eller `resolv.conf`.
    pub source: Option<String>,
    /// `/etc/resolv.conf` peker på en lokal mellomtjener (127.0.0.53, systemd-resolved).
    pub stub: Option<String>,
}

/// Socketene i dette navnerommet (`/proc/net/{tcp,tcp6,udp,udp6}`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Sockets {
    pub tcp_established: u32,
    pub tcp_listen: u32,
    /// TIME_WAIT, CLOSE_WAIT, SYN_SENT og de andre.
    pub tcp_other: u32,
    pub udp: u32,
    pub listeners: Vec<Listener>,
}

/// En port som lytter (TCP LISTEN, eller UDP bundet uten motpart).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Listener {
    /// `tcp` eller `udp`.
    pub proto: String,
    /// 4 eller 6.
    pub family: u8,
    pub addr: String,
    pub port: u16,
    pub uid: Option<u32>,
    pub user: Option<String>,
    pub pid: Option<u32>,
    pub process: Option<String>,
    #[serde(skip)]
    pub inode: u64,
}

// ---------------------------------------------------------------- Grensesnitt

/// Slaget grensesnitt ut fra navnet, ARPHRD-typen (`type`), `DEVTYPE` fra
/// `uevent`, om det har `wireless`/`phy80211`, om det har en enhet (`device`)
/// og om det er en bro.
pub fn iface_kind(name: &str, arphrd: Option<u32>, devtype: Option<&str>, wireless: bool, has_device: bool, bridge: bool) -> &'static str {
    if arphrd == Some(772) || name == "lo" {
        return "loopback";
    }
    if wireless || devtype == Some("wlan") {
        return "wifi";
    }
    if devtype == Some("wwan") || name.starts_with("ww") {
        return "mobile";
    }
    if bridge || devtype == Some("bridge") {
        return "bridge";
    }
    // 65534 = ARPHRD_NONE (tun), 65535 = ARPHRD_VOID; wg/tun/tailscale.
    if matches!(arphrd, Some(65534) | Some(65535)) || devtype == Some("wireguard") || ["tun", "wg", "tailscale", "ppp", "vpn"].iter().any(|p| name.starts_with(p)) {
        return "vpn";
    }
    if !has_device || ["veth", "docker", "virbr", "vnet", "tap", "lxc", "lxd", "vmnet", "zt", "br-", "dummy", "bond", "vlan"].iter().any(|p| name.starts_with(p)) {
        return "virtual";
    }
    if arphrd == Some(1) {
        return "ethernet";
    }
    "other"
}

/// `speed` i Mbit/s: -1, 0, 4294967295 og tekst er ukjent.
pub fn link_speed(s: &str) -> Option<u32> {
    let v: i64 = s.trim().parse().ok()?;
    (v > 0 && v < u32::MAX as i64).then_some(v as u32)
}

/// `duplex`: `full` eller `half`; `unknown` er `None`.
pub fn duplex(s: &str) -> Option<String> {
    let s = s.trim();
    matches!(s, "full" | "half").then(|| s.to_string())
}

/// En MAC-adresse som kan vises (ikke tom og ikke bare nuller).
pub fn mac(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && s.chars().any(|c| c.is_ascii_hexdigit() && c != '0')).then(|| s.to_ascii_uppercase())
}

/// `DEVTYPE=wlan` fra `uevent`.
pub fn uevent_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=').map(|v| v.trim().to_string())).filter(|v| !v.is_empty())
}

/// Bussen ut fra den fulle stien til `device` («…/0000:18:00.0» → PCI).
pub fn bus_from_path(path: &str) -> Option<&'static str> {
    let path = path.find("/devices/").map_or(path, |i| &path[i..]);
    if path.contains("/usb") {
        Some("USB")
    } else if path.contains("/mmc") {
        Some("SDIO")
    } else if path.contains("/pci") {
        Some("PCI")
    } else {
        None
    }
}

// ---------------------------------------------------------------- Adresser

/// IPv4 slik `/proc/net/route` skriver den: 8 heksadesimale sifre i
/// maskinens byte-rekkefølge (lite endian på x86 og ARM).
pub fn hex_ipv4(h: &str) -> Option<Ipv4Addr> {
    let v = u32::from_str_radix(h.trim(), 16).ok()?;
    Some(Ipv4Addr::from(v.to_le_bytes()))
}

/// IPv6 slik `/proc/net/ipv6_route` og `if_inet6` skriver den: 32 sifre i
/// nettverksrekkefølge.
pub fn hex_ipv6_be(h: &str) -> Option<Ipv6Addr> {
    let h = h.trim();
    if h.len() != 32 {
        return None;
    }
    let v = u128::from_str_radix(h, 16).ok()?;
    Some(Ipv6Addr::from(v))
}

/// IPv6 slik `/proc/net/tcp6` skriver den: fire 32-bits ord, hvert i
/// maskinens rekkefølge.
pub fn hex_ipv6_words(h: &str) -> Option<Ipv6Addr> {
    let h = h.trim();
    if h.len() != 32 {
        return None;
    }
    let mut b = [0u8; 16];
    for i in 0..4 {
        let w = u32::from_str_radix(&h[i * 8..i * 8 + 8], 16).ok()?;
        b[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    Some(Ipv6Addr::from(b))
}

/// Prefikslengden for en nettmaske.
pub fn mask_prefix(mask: Ipv4Addr) -> u8 {
    u32::from(mask).count_ones() as u8
}

pub fn scope4(a: Ipv4Addr) -> &'static str {
    if a.is_loopback() {
        "host"
    } else if a.is_link_local() {
        "link"
    } else {
        "global"
    }
}

/// `/proc/net/if_inet6`: adresse, indeks, prefiks, omfang, flagg og navn
/// (alt heksadesimalt). Gir (grensesnitt, adresse).
pub fn parse_if_inet6(text: &str) -> Vec<(String, IpAddr)> {
    let mut out = Vec::new();
    for l in text.lines() {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() < 6 {
            continue;
        }
        let (Some(a), Ok(prefix), Ok(scope), Ok(flags)) = (hex_ipv6_be(p[0]), u8::from_str_radix(p[2], 16), u8::from_str_radix(p[3], 16), u8::from_str_radix(p[4], 16)) else { continue };
        let scope = match scope {
            0x10 => "host",
            0x20 => "link",
            0x40 => "site",
            _ => "global",
        };
        // IFA_F_TEMPORARY = 0x01.
        out.push((p[5].to_string(), IpAddr { addr: a.to_string(), prefix, scope: scope.into(), temporary: flags & 0x01 != 0 }));
    }
    out
}

/// Standardruten i `/proc/net/route` med lavest metrikk.
pub fn parse_route(text: &str) -> Option<Gateway> {
    let mut best: Option<Gateway> = None;
    for l in text.lines().skip(1) {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() < 8 {
            continue;
        }
        let flags = u32::from_str_radix(p[3], 16).unwrap_or(0);
        // RTF_UP = 1, RTF_GATEWAY = 2.
        if p[1] != "00000000" || p[7] != "00000000" || flags & 0x3 != 0x3 {
            continue;
        }
        let (Some(gw), Ok(metric)) = (hex_ipv4(p[2]), p[6].parse::<u32>()) else { continue };
        if best.as_ref().is_none_or(|b| metric < b.metric) {
            best = Some(Gateway { addr: gw.to_string(), iface: p[0].to_string(), metric });
        }
    }
    best
}

/// Standardruten i `/proc/net/ipv6_route` (::/0 med neste hopp) med lavest metrikk.
pub fn parse_ipv6_route(text: &str) -> Option<Gateway> {
    let mut best: Option<Gateway> = None;
    for l in text.lines() {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() < 10 {
            continue;
        }
        if p[1] != "00" || u128::from_str_radix(p[0], 16).ok() != Some(0) || p[9] == "lo" {
            continue;
        }
        let Some(hop) = hex_ipv6_be(p[4]).filter(|a| !a.is_unspecified()) else { continue };
        let metric = u32::from_str_radix(p[5], 16).unwrap_or(u32::MAX);
        // RTF_UP = 0x1, RTF_REJECT = 0x200.
        let flags = u32::from_str_radix(p[8], 16).unwrap_or(0);
        if flags & 0x1 == 0 || flags & 0x200 != 0 {
            continue;
        }
        if best.as_ref().is_none_or(|b| metric < b.metric) {
            best = Some(Gateway { addr: hop.to_string(), iface: p[9].to_string(), metric });
        }
    }
    best
}

/// `nameserver` og `search`/`domain` i en `resolv.conf`.
pub fn parse_resolv_conf(text: &str) -> (Vec<String>, Vec<String>) {
    let (mut servers, mut domains) = (Vec::new(), Vec::new());
    for l in text.lines() {
        let l = l.split(['#', ';']).next().unwrap_or("").trim();
        let mut w = l.split_whitespace();
        match w.next() {
            Some("nameserver") => {
                if let Some(s) = w.next()
                    && !servers.iter().any(|x| x == s)
                {
                    servers.push(s.to_string());
                }
            }
            Some("search") | Some("domain") => {
                for d in w {
                    if d != "." && !domains.iter().any(|x| x == d) {
                        domains.push(d.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    (servers, domains)
}

/// Er adressen en lokal mellomtjener (127.0.0.53 hos systemd-resolved)?
pub fn is_stub(server: &str) -> bool {
    server.parse::<Ipv4Addr>().is_ok_and(|a| a.is_loopback()) || server == "::1"
}

// ---------------------------------------------------------------- Socketer

/// Én linje i `/proc/net/{tcp,udp}[6]`.
#[derive(Clone, Debug, PartialEq)]
pub struct SockLine {
    pub local: (String, u16),
    pub remote: (String, u16),
    pub state: u8,
    pub uid: u32,
    pub inode: u64,
}

fn endpoint(s: &str, v6: bool) -> Option<(String, u16)> {
    let (a, p) = s.split_once(':')?;
    let port = u16::from_str_radix(p, 16).ok()?;
    let addr = if v6 {
        let a = hex_ipv6_words(a)?;
        // IPv4-tilordnet («::ffff:127.0.0.1») skrives som IPv4.
        a.to_ipv4_mapped().map(|x| x.to_string()).unwrap_or_else(|| a.to_string())
    } else {
        hex_ipv4(a)?.to_string()
    };
    Some((addr, port))
}

pub fn parse_sockets(text: &str, v6: bool) -> Vec<SockLine> {
    let mut out = Vec::new();
    for l in text.lines().skip(1) {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() < 10 {
            continue;
        }
        let (Some(local), Some(remote), Ok(state), Ok(uid), Ok(inode)) = (endpoint(p[1], v6), endpoint(p[2], v6), u8::from_str_radix(p[3], 16), p[7].parse(), p[9].parse()) else { continue };
        out.push(SockLine { local, remote, state, uid, inode });
    }
    out
}

pub const TCP_ESTABLISHED: u8 = 0x01;
pub const TCP_LISTEN: u8 = 0x0A;
/// UDP bundet uten motpart (`TCP_CLOSE` i tabellen).
pub const UDP_UNCONNECTED: u8 = 0x07;

/// Tellingen og lytteportene fra de fire tabellene (TCP v4, TCP v6, UDP v4, UDP v6).
pub fn summarize(tcp4: &[SockLine], tcp6: &[SockLine], udp4: &[SockLine], udp6: &[SockLine]) -> Sockets {
    let mut s = Sockets::default();
    let mut push = |proto: &str, family: u8, l: &SockLine| {
        if !s.listeners.iter().any(|x: &Listener| x.proto == proto && x.port == l.local.1 && x.addr == l.local.0) {
            s.listeners.push(Listener { proto: proto.into(), family, addr: l.local.0.clone(), port: l.local.1, uid: Some(l.uid), inode: l.inode, ..Default::default() });
        }
    };
    for (list, fam) in [(tcp4, 4), (tcp6, 6)] {
        for l in list {
            match l.state {
                TCP_ESTABLISHED => s.tcp_established += 1,
                TCP_LISTEN => {
                    s.tcp_listen += 1;
                    push("tcp", fam, l);
                }
                _ => s.tcp_other += 1,
            }
        }
    }
    for (list, fam) in [(udp4, 4), (udp6, 6)] {
        for l in list {
            s.udp += 1;
            if l.state == UDP_UNCONNECTED && l.remote.1 == 0 {
                push("udp", fam, l);
            }
        }
    }
    s.listeners.sort_by(|a, b| (a.proto.as_str(), a.port, a.family, a.addr.as_str()).cmp(&(b.proto.as_str(), b.port, b.family, b.addr.as_str())));
    s
}

/// `ip_local_port_range` («32768\t60999»); standarden når fila mangler.
pub fn port_range(text: &str) -> (u16, u16) {
    let mut w = text.split_whitespace().filter_map(|x| x.parse::<u16>().ok());
    match (w.next(), w.next()) {
        (Some(a), Some(b)) if a <= b => (a, b),
        _ => (32768, 60999),
    }
}

/// UDP-socketer i det flyktige portområdet er klienter (DNS-oppslag, QUIC),
/// ikke tjenester: de telles, men står ikke blant lytteportene.
pub fn drop_ephemeral_udp(s: &mut Sockets, range: (u16, u16)) {
    s.listeners.retain(|l| l.proto != "udp" || !(range.0..=range.1).contains(&l.port));
}

/// «socket:[12345]» → 12345.
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
}

/// Brukernavnet for en UID i `/etc/passwd`.
pub fn passwd_name(text: &str, uid: u32) -> Option<String> {
    text.lines().find_map(|l| {
        let mut p = l.split(':');
        let name = p.next()?;
        let id: u32 = p.nth(1)?.parse().ok()?;
        (id == uid && !name.is_empty()).then(|| name.to_string())
    })
}

/// Lytter porten bare lokalt (127.0.0.0/8, ::1)?
pub fn local_only(addr: &str) -> bool {
    match addr.parse::<std::net::IpAddr>() {
        Ok(a) => a.is_loopback(),
        Err(_) => false,
    }
}

/// Lytter porten på alle adresser (0.0.0.0 eller ::)?
pub fn any_addr(addr: &str) -> bool {
    addr.parse::<std::net::IpAddr>().is_ok_and(|a| a.is_unspecified())
}

// ---------------------------------------------------------------- Wi-Fi

/// `/proc/net/wireless`: (grensesnitt, kvalitet 0–100 %, signal dBm).
/// Kvaliteten er `link` av 70 (iwlwifi, ath og de fleste andre).
pub fn parse_proc_wireless(text: &str) -> Vec<(String, Option<f64>, Option<f64>)> {
    let mut out = Vec::new();
    for l in text.lines().skip(2) {
        let Some((name, rest)) = l.split_once(':') else { continue };
        let p: Vec<&str> = rest.split_whitespace().collect();
        if p.len() < 3 {
            continue;
        }
        let num = |s: &str| s.trim_end_matches('.').parse::<f64>().ok();
        let link = num(p[1]).map(|v| (v / 70.0 * 100.0).clamp(0.0, 100.0).round());
        let level = num(p[2]).filter(|v| *v < 0.0 && *v > -150.0);
        out.push((name.trim().to_string(), link, level));
    }
    out
}

/// Kanalen for en frekvens (MHz): 2,4, 5 og 6 GHz.
pub fn wifi_channel(freq: u32) -> Option<u32> {
    match freq {
        2484 => Some(14),
        2412..=2472 => Some((freq - 2407) / 5),
        5160..=5885 => Some((freq - 5000) / 5),
        5955..=7115 => Some((freq - 5950) / 5),
        _ => None,
    }
}

/// Båndet for en frekvens: «2.4», «5» eller «6» (GHz).
pub fn wifi_band(freq: u32) -> Option<&'static str> {
    match freq {
        2400..=2500 => Some("2.4"),
        4900..=5899 => Some("5"),
        5925..=7125 => Some("6"),
        _ => None,
    }
}

/// Signal i % fra dBm når NetworkManager ikke gir styrken: −100 dBm er 0 %,
/// −50 dBm og sterkere er 100 % (samme skala som NetworkManager bruker).
pub fn dbm_pct(dbm: f64) -> f64 {
    (2.0 * (dbm + 100.0)).clamp(0.0, 100.0)
}

/// Sikkerheten fra NetworkManagers flagg på tilgangspunktet: `Flags`
/// (`NM80211ApFlags`), `WpaFlags` og `RsnFlags` (`NM80211ApSecurityFlags`).
pub fn wifi_security(flags: u32, wpa: u32, rsn: u32) -> &'static str {
    const PRIVACY: u32 = 0x1;
    const PSK: u32 = 0x100;
    const EAP: u32 = 0x200;
    const SAE: u32 = 0x400;
    const OWE: u32 = 0x800;
    const EAP_SUITE_B: u32 = 0x2000;
    if rsn & (EAP_SUITE_B) != 0 {
        "wpa3-enterprise"
    } else if rsn & SAE != 0 {
        "wpa3"
    } else if rsn & OWE != 0 {
        "owe"
    } else if rsn & EAP != 0 || wpa & EAP != 0 {
        "wpa2-enterprise"
    } else if rsn & PSK != 0 {
        "wpa2"
    } else if wpa & PSK != 0 {
        "wpa"
    } else if flags & PRIVACY != 0 {
        "wep"
    } else {
        "open"
    }
}

/// SSID-en som tekst (bytene fra NetworkManager kan være hva som helst).
pub fn ssid_text(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(bytes).trim_matches('\0').to_string();
    (!s.is_empty()).then_some(s)
}

// ---------------------------------------------------------------- Sammendrag

/// Grensesnittet som er «aktiv tilkobling»: det standardruten går via,
/// ellers det første tilkoblede fysiske.
pub fn primary(info: &NetInfo) -> Option<&NetIface> {
    info.ifaces
        .iter()
        .find(|i| i.default_route && i.kind != "loopback")
        .or_else(|| info.ifaces.iter().find(|i| i.physical() && i.connected()))
}

/// Summen mottatt og sendt over de fysiske kortene (byte).
pub fn totals(info: &NetInfo) -> (Option<u64>, Option<u64>) {
    let mut rx: Option<u64> = None;
    let mut tx: Option<u64> = None;
    for i in info.ifaces.iter().filter(|i| i.physical()) {
        if let Some(v) = i.stats.rx_bytes {
            rx = Some(rx.unwrap_or(0) + v);
        }
        if let Some(v) = i.stats.tx_bytes {
            tx = Some(tx.unwrap_or(0) + v);
        }
    }
    (rx, tx)
}

/// Rekkefølgen på siden: standardruten først, så tilkoblede, Ethernet før
/// Wi-Fi, og navnet. Loopback står alltid sist.
pub fn sort_ifaces(list: &mut [NetIface]) {
    let rank = |k: &str| match k {
        "ethernet" => 0,
        "wifi" => 1,
        "mobile" => 2,
        "other" => 3,
        "vpn" => 4,
        "bridge" => 5,
        "virtual" => 6,
        _ => 7,
    };
    let key = |i: &NetIface| (i.kind == "loopback", !i.default_route, !i.connected(), rank(&i.kind));
    list.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
wlo1\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
enp5s0\t00000000\t0102000A\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
wlo1\t0001A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n";

    #[test]
    fn standardruten_med_lavest_metrikk() {
        let g = parse_route(ROUTE).unwrap();
        assert_eq!(g, Gateway { addr: "10.0.2.1".into(), iface: "enp5s0".into(), metric: 100 });
        assert_eq!(parse_route(&ROUTE.lines().take(1).chain(ROUTE.lines().skip(3)).collect::<Vec<_>>().join("\n")), None);
        assert_eq!(hex_ipv4("0101A8C0"), Some(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(mask_prefix(Ipv4Addr::new(255, 255, 255, 0)), 24);
    }

    #[test]
    fn ipv6_standardrute_og_adresser() {
        let r = "fe800000000000000000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000400 00000001 00000000 00000001     wlo1\n\
00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000258 00000003 00000000 00450003     wlo1\n\
00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200       lo\n";
        let g = parse_ipv6_route(r).unwrap();
        assert_eq!(g.addr, "fe80::1");
        assert_eq!(g.iface, "wlo1");
        assert_eq!(g.metric, 600);
        assert_eq!(parse_ipv6_route(r.lines().last().unwrap()), None, "lo og avvisning teller ikke");
        let a = parse_if_inet6("00000000000000000000000000000001 01 80 10 80       lo\nfe80000000000000b1d318cc29b03dbd 03 40 20 80     wlo1\n20010db8000000000000000000000042 03 40 00 01     wlo1\n");
        assert_eq!(a.len(), 3);
        assert_eq!(a[0].1, IpAddr { addr: "::1".into(), prefix: 128, scope: "host".into(), temporary: false });
        assert_eq!(a[1].1.scope, "link");
        assert_eq!(a[2].1, IpAddr { addr: "2001:db8::42".into(), prefix: 64, scope: "global".into(), temporary: true });
        assert_eq!(a[2].1.cidr(), "2001:db8::42/64");
    }

    #[test]
    fn resolv_conf_og_mellomtjener() {
        let (s, d) = parse_resolv_conf("# kommentar\nnameserver 127.0.0.53\noptions edns0 trust-ad\nsearch hjemme.lan .\nnameserver 2001:db8::53 ; slutt\nnameserver 127.0.0.53\n");
        assert_eq!(s, vec!["127.0.0.53", "2001:db8::53"]);
        assert_eq!(d, vec!["hjemme.lan"]);
        assert!(is_stub("127.0.0.53") && is_stub("::1") && !is_stub("192.0.2.53"));
        assert_eq!(parse_resolv_conf(""), (vec![], vec![]));
    }

    #[test]
    fn socketer_og_lytteporter() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 3600007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000   989        0 11752 1 0000000000000000 100 0 0 10 5\n\
   1: 2A0200C0:A683 0100000A:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 1725798 1 0000000000000000 100 0 0 10 0\n\
   2: 2A0200C0:A684 0100000A:01BB 06 00000000:00000000 00:00000000 00000000  1000        0 0 1 0000000000000000 100 0 0 10 0\n\
   3: zz\n";
        let t4 = parse_sockets(tcp, false);
        assert_eq!(t4.len(), 3);
        assert_eq!(t4[0].local, ("127.0.0.54".into(), 53));
        assert_eq!(t4[1].remote, ("10.0.0.1".into(), 443));
        let udp6 = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n\
 4436: 00000000000000000000000000000000:EB02 00000000000000000000000000000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 485103 2 0000000000000000 0\n\
 4437: 0000000000000000FFFF00000100007F:0277 00000000000000000000000000000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 485104 2 0000000000000000 0\n\
 4438: 00000000000000000000000001000000:0143 00000000000000000000000001000000:1F90 01 00000000:00000000 00:00000000 00000000     0        0 7163 2 0000000000000000 0\n";
        let u6 = parse_sockets(udp6, true);
        assert_eq!(u6[0].local, ("::".into(), 60162));
        assert_eq!(u6[1].local, ("127.0.0.1".into(), 631), "IPv4-tilordnet skrives som IPv4");
        assert_eq!(u6[2].local.0, "::1");
        let s = summarize(&t4, &[], &[], &u6);
        assert_eq!((s.tcp_established, s.tcp_listen, s.tcp_other, s.udp), (1, 1, 1, 3));
        let ports: Vec<(&str, u16)> = s.listeners.iter().map(|l| (l.proto.as_str(), l.port)).collect();
        assert_eq!(ports, vec![("tcp", 53), ("udp", 631), ("udp", 60162)], "UDP med motpart lytter ikke");
        let mut s2 = s.clone();
        drop_ephemeral_udp(&mut s2, port_range("32768\t60999\n"));
        assert_eq!(s2.listeners.len(), 2, "flyktig UDP-port er en klient");
        assert_eq!(s2.udp, 3, "men telles");
        assert_eq!(port_range(""), (32768, 60999));
        assert_eq!(port_range("1024 2000"), (1024, 2000));
        assert!(local_only("127.0.0.54") && local_only("::1") && !local_only("::"));
        assert!(any_addr("::") && any_addr("0.0.0.0") && !any_addr("127.0.0.1"));
        assert_eq!(socket_inode("socket:[485103]"), Some(485103));
        assert_eq!(socket_inode("pipe:[1]"), None);
        assert_eq!(passwd_name("root:x:0:0:root:/root:/bin/bash\nsystemd-resolve:x:989:989::/:/usr/sbin/nologin\n", 989).as_deref(), Some("systemd-resolve"));
        assert_eq!(passwd_name("root:x:0:0::/:/bin/sh\n", 5), None);
    }

    #[test]
    fn wifi_tallene() {
        let w = parse_proc_wireless("Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE\n face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22\n  wlo1: 0000   70.  -34.  -256        0      0      0      0  39030        0\n  wlp2: 0000   35.  0.  -256        0      0      0      0  0        0\n");
        assert_eq!(w[0], ("wlo1".into(), Some(100.0), Some(-34.0)));
        assert_eq!(w[1], ("wlp2".into(), Some(50.0), None), "nivå 0 er ikke et målt signal");
        assert_eq!(wifi_channel(2412), Some(1));
        assert_eq!(wifi_channel(2484), Some(14));
        assert_eq!(wifi_channel(5180), Some(36));
        assert_eq!(wifi_channel(5955), Some(1));
        assert_eq!(wifi_channel(900), None);
        assert_eq!(wifi_band(2437), Some("2.4"));
        assert_eq!(wifi_band(5500), Some("5"));
        assert_eq!(wifi_band(6115), Some("6"));
        assert_eq!(dbm_pct(-52.0), 96.0);
        assert_eq!(dbm_pct(-30.0), 100.0);
        assert_eq!(dbm_pct(-110.0), 0.0);
        assert_eq!(wifi_security(0, 0, 0), "open");
        assert_eq!(wifi_security(1, 0, 0), "wep");
        assert_eq!(wifi_security(1, 0x188, 0), "wpa");
        assert_eq!(wifi_security(1, 0, 0x188), "wpa2");
        assert_eq!(wifi_security(1, 0, 0x588), "wpa3");
        assert_eq!(wifi_security(1, 0, 0x288), "wpa2-enterprise");
        assert_eq!(ssid_text(b"Rigg\0"), Some("Rigg".into()));
        assert_eq!(ssid_text(b""), None);
    }

    #[test]
    fn grensesnitt_og_sammendrag() {
        assert_eq!(iface_kind("lo", Some(772), None, false, false, false), "loopback");
        assert_eq!(iface_kind("wlo1", Some(1), Some("wlan"), true, true, false), "wifi");
        assert_eq!(iface_kind("enp24s0", Some(1), None, false, true, false), "ethernet");
        assert_eq!(iface_kind("docker0", Some(1), Some("bridge"), false, false, true), "bridge");
        assert_eq!(iface_kind("veth12ab", Some(1), None, false, false, false), "virtual");
        assert_eq!(iface_kind("tun0", Some(65534), None, false, false, false), "vpn");
        assert_eq!(iface_kind("wg0", Some(65534), Some("wireguard"), false, false, false), "vpn");
        assert_eq!(iface_kind("wwan0", Some(1), Some("wwan"), false, true, false), "mobile");
        assert_eq!(link_speed("1000\n"), Some(1000));
        assert_eq!(link_speed("-1"), None);
        assert_eq!(link_speed("4294967295"), None);
        assert_eq!(duplex("full\n").as_deref(), Some("full"));
        assert_eq!(duplex("unknown"), None);
        assert_eq!(mac("30:9c:23:df:40:36\n").as_deref(), Some("30:9C:23:DF:40:36"));
        assert_eq!(mac("00:00:00:00:00:00"), None);
        assert_eq!(uevent_value("DEVTYPE=wlan\nINTERFACE=wlo1\n", "DEVTYPE").as_deref(), Some("wlan"));
        assert_eq!(uevent_value("INTERFACE=wlo1\n", "DEVTYPE"), None);
        assert_eq!(bus_from_path("/sys/devices/pci0000:00/0000:00:01.3/0000:18:00.0"), Some("PCI"));
        assert_eq!(bus_from_path("/sys/devices/pci0000:00/0000:00:14.0/usb2/2-1/2-1:1.0"), Some("USB"));
        assert_eq!(bus_from_path("/sys/devices/virtual/net/docker0"), None);

        let eth = NetIface { name: "enp5s0".into(), kind: "ethernet".into(), operstate: "down".into(), carrier: Some(false), stats: NetCounters { rx_bytes: Some(10), tx_bytes: Some(5), ..Default::default() }, ..Default::default() };
        let wifi = NetIface {
            name: "wlo1".into(),
            kind: "wifi".into(),
            operstate: "up".into(),
            carrier: Some(true),
            default_route: true,
            stats: NetCounters { rx_bytes: Some(1000), tx_bytes: None, ..Default::default() },
            ..Default::default()
        };
        let lo = NetIface { name: "lo".into(), kind: "loopback".into(), operstate: "unknown".into(), stats: NetCounters { rx_bytes: Some(99), tx_bytes: Some(99), ..Default::default() }, ..Default::default() };
        let tun = NetIface { name: "tun0".into(), kind: "vpn".into(), operstate: "unknown".into(), carrier: Some(true), ipv4: vec![IpAddr { addr: "10.8.0.2".into(), prefix: 24, scope: "global".into(), temporary: false }], ..Default::default() };
        assert!(tun.connected() && !tun.physical());
        assert!(!eth.connected() && eth.physical());
        let mut info = NetInfo { ifaces: vec![lo, tun, eth, wifi], ..Default::default() };
        assert_eq!(primary(&info).map(|i| i.name.as_str()), Some("wlo1"));
        assert_eq!(totals(&info), (Some(1010), Some(5)), "loopback og VPN telles ikke");
        sort_ifaces(&mut info.ifaces);
        let names: Vec<&str> = info.ifaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["wlo1", "tun0", "enp5s0", "lo"]);
        info.ifaces.retain(|i| i.name != "wlo1");
        assert_eq!(primary(&info), None, "VPN uten standardrute er ikke aktiv tilkobling");
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["ifaces"][0]["operstate"], "unknown");
        assert!(json["ifaces"][0].get("speedMbps").is_some(), "camelCase i protokollen");
        let back: NetInfo = serde_json::from_value(json!({"t": 1, "ifaces": [{"name": "x"}]})).unwrap();
        assert_eq!(back.ifaces[0].name, "x");
    }

    use serde_json::json;
}
