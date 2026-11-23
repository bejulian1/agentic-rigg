//! Nettverk-siden (`get_network`): grensesnittene i `/sys/class/net`,
//! IPv6 fra `/proc/net/if_inet6`, IPv4 fra `getifaddrs`, rutene i
//! `/proc/net/route` og `ipv6_route`, DNS fra NetworkManager eller
//! `resolv.conf`, Wi-Fi-signalet i `/proc/net/wireless` og socketene i
//! `/proc/net/{tcp,tcp6,udp,udp6}`. Alt er lesing uten root. Prosessen bak en
//! lytteport finnes bare når `/proc/PID/fd` kan leses (egne prosesser).
//!
//! Alle stier går gjennom `root`, så testene kan bruke en fixture-mappe
//! (`tests/fixtures/nett`). `getifaddrs` og systembussen brukes bare med `/`.

use rigg_core::net::{self as N, IpAddr, NetCounters, NetIface, NetInfo, Wifi};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Leverandør og modell for en PCI-ID («8086», «1539»), slått opp i pci.ids.
pub type PciNames<'a> = &'a mut dyn FnMut(&str, &str) -> (Option<String>, Option<String>);

fn txt(p: &Path) -> Option<String> {
    crate::read_text(p).filter(|s| !s.is_empty())
}

fn num<T: std::str::FromStr>(p: &Path) -> Option<T> {
    txt(p)?.parse().ok()
}

/// Leser og husker det som er dyrt (navnene fra `pci.ids` og `/etc/passwd`).
pub struct NetReader {
    root: PathBuf,
    nm: super::nm::Nm,
    /// «8086:1539» → (leverandør, modell).
    pci_names: HashMap<String, (Option<String>, Option<String>)>,
    pci_ids: Option<String>,
}

impl NetReader {
    pub fn new(root: &Path) -> NetReader {
        NetReader { root: root.to_path_buf(), nm: super::nm::Nm::new(), pci_names: HashMap::new(), pci_ids: None }
    }

    fn host(&self) -> bool {
        self.root == Path::new("/")
    }

    /// Hele svaret på `get_network`.
    pub fn read(&mut self, now: i64) -> NetInfo {
        let v4 = if self.host() { ipv4_addrs() } else { read_v4_fixture(&self.root) };
        let nm = if self.host() { self.nm.read() } else { None };
        let root = self.root.clone();
        let mut info = read_with(&root, now, &v4, nm.as_ref(), &mut |vendor, device| self.pci_name(vendor, device));
        // pci.ids er over 1 MB: navnene huskes, men fila slippes etter hver lesing.
        self.pci_ids = None;
        info.source = "sysfs".into();
        info
    }

    fn pci_name(&mut self, vendor: &str, device: &str) -> (Option<String>, Option<String>) {
        let key = format!("{vendor}:{device}");
        if let Some(v) = self.pci_names.get(&key) {
            return v.clone();
        }
        let root = self.root.clone();
        let ids = self
            .pci_ids
            .get_or_insert_with(|| ["usr/share/hwdata/pci.ids", "usr/share/misc/pci.ids", "usr/share/pci.ids"].iter().find_map(|p| std::fs::read_to_string(root.join(p)).ok()).unwrap_or_default());
        let (v, d) = super::pci::pci_name(ids, vendor, device);
        let v = (v.as_deref().map(super::pci::short_vendor), d.as_deref().map(super::pci::short_device));
        self.pci_names.insert(key, v.clone());
        v
    }
}

/// Testene: IPv4-adressene i `ipv4` i fixture-mappa («navn adresse/prefiks» per linje).
fn read_v4_fixture(root: &Path) -> Vec<(String, IpAddr)> {
    std::fs::read_to_string(root.join("ipv4"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (n, a) = l.split_once(' ')?;
            let (a, p) = a.trim().split_once('/')?;
            let addr: std::net::Ipv4Addr = a.parse().ok()?;
            Some((n.to_string(), IpAddr { addr: a.to_string(), prefix: p.parse().ok()?, scope: N::scope4(addr).into(), temporary: false }))
        })
        .collect()
}

/// IPv4-adressene fra `getifaddrs` (ingen fil i `/proc` har dem med prefiks).
pub fn ipv4_addrs() -> Vec<(String, IpAddr)> {
    let mut out = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fyller en lenket liste vi frigir med freeifaddrs under.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return out;
    }
    let mut cur = head;
    while !cur.is_null() {
        // SAFETY: cur er et gyldig element i lista fra getifaddrs.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
            continue;
        }
        // SAFETY: ifa_addr peker på en sockaddr; familien sier hvilken.
        if i32::from(unsafe { (*ifa.ifa_addr).sa_family }) != libc::AF_INET {
            continue;
        }
        // SAFETY: AF_INET betyr sockaddr_in; ifa_name er en C-streng.
        let (addr, name) = unsafe { ((*(ifa.ifa_addr as *const libc::sockaddr_in)).sin_addr.s_addr, std::ffi::CStr::from_ptr(ifa.ifa_name)) };
        let addr = std::net::Ipv4Addr::from(u32::from_be(addr));
        let prefix = if ifa.ifa_netmask.is_null() {
            32
        } else {
            // SAFETY: nettmasken har samme familie som adressen.
            N::mask_prefix(std::net::Ipv4Addr::from(u32::from_be(unsafe { (*(ifa.ifa_netmask as *const libc::sockaddr_in)).sin_addr.s_addr })))
        };
        out.push((name.to_string_lossy().into_owned(), IpAddr { addr: addr.to_string(), prefix, scope: N::scope4(addr).into(), temporary: false }));
    }
    // SAFETY: head kom fra getifaddrs og frigis én gang.
    unsafe { libc::freeifaddrs(head) };
    out
}

fn counters(d: &Path) -> NetCounters {
    let s = |f: &str| num::<u64>(&d.join("statistics").join(f));
    NetCounters {
        rx_bytes: s("rx_bytes"),
        tx_bytes: s("tx_bytes"),
        rx_packets: s("rx_packets"),
        tx_packets: s("tx_packets"),
        rx_errors: s("rx_errors"),
        tx_errors: s("tx_errors"),
        rx_dropped: s("rx_dropped"),
        tx_dropped: s("tx_dropped"),
        multicast: s("multicast"),
        collisions: s("collisions"),
    }
}

/// Ett grensesnitt fra `/sys/class/net/<navn>`.
fn read_iface(dir: &Path, name: &str, pci: PciNames) -> NetIface {
    let d = dir.join(name);
    let uevent = txt(&d.join("uevent")).unwrap_or_default();
    let devtype = N::uevent_value(&uevent, "DEVTYPE");
    let wireless = d.join("wireless").is_dir() || d.join("phy80211").exists();
    let dev = d.join("device");
    let has_device = dev.exists();
    let kind = N::iface_kind(name, num(&d.join("type")), devtype.as_deref(), wireless, has_device, d.join("bridge").is_dir());
    let operstate = txt(&d.join("operstate")).unwrap_or_else(|| "unknown".into());
    let mut i = NetIface {
        name: name.to_string(),
        kind: kind.into(),
        carrier: txt(&d.join("carrier")).map(|c| c == "1"),
        mac: txt(&d.join("address")).and_then(|m| N::mac(&m)).filter(|_| kind != "loopback"),
        mtu: num(&d.join("mtu")),
        // Hastighet og dupleks gir EINVAL når kabelen er ute; da er de ukjente.
        speed_mbps: if operstate == "up" { txt(&d.join("speed")).and_then(|s| N::link_speed(&s)) } else { None },
        duplex: if operstate == "up" { txt(&d.join("duplex")).and_then(|s| N::duplex(&s)) } else { None },
        operstate,
        stats: counters(&d),
        ..Default::default()
    };
    if has_device {
        let real = std::fs::canonicalize(&dev).unwrap_or_else(|_| dev.clone());
        i.bus = N::bus_from_path(&real.to_string_lossy()).map(str::to_string);
        i.driver = std::fs::read_link(dev.join("driver")).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let hex = |p: &Path| txt(p).map(|s| s.trim_start_matches("0x").to_ascii_lowercase());
        match i.bus.as_deref() {
            Some("PCI") => {
                i.bus_addr = real.file_name().map(|n| n.to_string_lossy().into_owned());
                if let (Some(v), Some(dv)) = (hex(&dev.join("vendor")), hex(&dev.join("device"))) {
                    (i.vendor, i.model) = pci(&v, &dv);
                    i.ids = Some(format!("{v}:{dv}"));
                }
            }
            Some("USB") => {
                // `device` er USB-grensesnittet; enheten med navnene er mappa over.
                let up = real.parent().map(Path::to_path_buf).unwrap_or_default();
                i.bus_addr = up.file_name().map(|n| n.to_string_lossy().into_owned());
                i.vendor = txt(&up.join("manufacturer"));
                i.model = txt(&up.join("product"));
                if let (Some(v), Some(p)) = (hex(&up.join("idVendor")), hex(&up.join("idProduct"))) {
                    i.ids = Some(format!("{v}:{p}"));
                }
            }
            _ => {}
        }
    }
    i
}

/// Lytteportene får prosess og bruker der det kan leses.
fn owners(root: &Path, s: &mut N::Sockets) {
    let passwd = std::fs::read_to_string(root.join("etc/passwd")).unwrap_or_default();
    for l in &mut s.listeners {
        l.user = l.uid.and_then(|u| N::passwd_name(&passwd, u));
    }
    let want: HashMap<u64, usize> = s.listeners.iter().enumerate().filter(|(_, l)| l.inode > 0).map(|(i, l)| (l.inode, i)).collect();
    if want.is_empty() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root.join("proc")) else { return };
    let mut found = 0;
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        // Andres prosesser gir EACCES her; de hoppes over.
        let Ok(fds) = std::fs::read_dir(e.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            let Some(inode) = std::fs::read_link(fd.path()).ok().and_then(|p| N::socket_inode(&p.to_string_lossy())) else { continue };
            if let Some(&i) = want.get(&inode)
                && s.listeners[i].pid.is_none()
            {
                s.listeners[i].pid = Some(pid);
                s.listeners[i].process = txt(&e.path().join("comm"));
                found += 1;
            }
        }
        if found == want.len() {
            break;
        }
    }
}

/// Alt unntatt kilden, fra en rot, IPv4-adressene og NetworkManager.
pub fn read_with(root: &Path, now: i64, v4: &[(String, IpAddr)], nm: Option<&super::nm::NmData>, pci: PciNames) -> NetInfo {
    let dir = root.join("sys/class/net");
    let mut names: Vec<String> = std::fs::read_dir(&dir).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.sort();
    let mut ifaces: Vec<NetIface> = names.iter().map(|n| read_iface(&dir, n, pci)).collect();
    // Adressene.
    let v6 = N::parse_if_inet6(&std::fs::read_to_string(root.join("proc/net/if_inet6")).unwrap_or_default());
    for i in &mut ifaces {
        i.ipv4 = v4.iter().filter(|(n, _)| *n == i.name).map(|(_, a)| a.clone()).collect();
        i.ipv6 = v6.iter().filter(|(n, _)| *n == i.name).map(|(_, a)| a.clone()).collect();
        // Globale først, lenkelokale sist.
        i.ipv6.sort_by_key(|a| (a.scope != "global", a.temporary));
    }
    // Rutene.
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_default();
    let routes = N::Routes { gateway4: N::parse_route(&read("proc/net/route")), gateway6: N::parse_ipv6_route(&read("proc/net/ipv6_route")) };
    for i in &mut ifaces {
        i.default_route = routes.gateway4.as_ref().is_some_and(|g| g.iface == i.name) || routes.gateway6.as_ref().is_some_and(|g| g.iface == i.name);
    }
    // Wi-Fi: signalet fra /proc, resten fra NetworkManager.
    let wl = N::parse_proc_wireless(&read("proc/net/wireless"));
    for i in ifaces.iter_mut().filter(|i| i.kind == "wifi") {
        let mut w = Wifi::default();
        if let Some((_, q, dbm)) = wl.iter().find(|(n, _, _)| *n == i.name) {
            w.signal_dbm = *dbm;
            w.signal_pct = *q;
        }
        i.wifi = Some(w);
    }
    // NetworkManager: tilkoblingsnavn, driver, fastvare og Wi-Fi-detaljene.
    let mut dns = N::Dns::default();
    if let Some(nm) = nm {
        let mut order: Vec<&super::nm::NmDevice> = nm.devices.iter().collect();
        order.sort_by_key(|d| Some(&d.iface) != nm.primary.as_ref());
        for d in order {
            for s in &d.dns {
                if !dns.servers.contains(s) {
                    dns.servers.push(s.clone());
                }
            }
            for s in &d.domains {
                if !dns.domains.contains(s) {
                    dns.domains.push(s.clone());
                }
            }
            let Some(i) = ifaces.iter_mut().find(|i| i.name == d.iface) else { continue };
            i.connection = d.connection.clone();
            i.firmware = d.firmware.clone();
            if i.driver.is_none() {
                i.driver = d.driver.clone();
            }
            let connected = i.connected();
            if let (Some(w), Some(nw)) = (i.wifi.as_mut(), &d.wifi)
                && connected
            {
                w.ssid = nw.ssid.clone();
                w.bssid = nw.bssid.clone();
                w.freq_mhz = nw.freq_mhz;
                w.bitrate_mbps = nw.bitrate_mbps;
                w.security = nw.security.clone();
                if nw.signal_pct.is_some() {
                    w.signal_pct = nw.signal_pct;
                }
            }
        }
        if !dns.servers.is_empty() {
            dns.source = Some("networkmanager".into());
        }
    }
    for w in ifaces.iter_mut().filter_map(|i| i.wifi.as_mut()) {
        if w.signal_pct.is_none() {
            w.signal_pct = w.signal_dbm.map(N::dbm_pct);
        }
    }
    let (etc_servers, etc_domains) = N::parse_resolv_conf(&read("etc/resolv.conf"));
    if dns.servers.is_empty() {
        let (s, d) = N::parse_resolv_conf(&read("run/systemd/resolve/resolv.conf"));
        if !s.is_empty() {
            (dns.servers, dns.domains, dns.source) = (s, d, Some("resolved".into()));
        } else if !etc_servers.is_empty() {
            (dns.servers, dns.domains, dns.source) = (etc_servers.clone(), etc_domains.clone(), Some("resolv.conf".into()));
        }
    }
    if dns.domains.is_empty() {
        dns.domains = etc_domains;
    }
    // Lokal mellomtjener (systemd-resolved) foran de egentlige tjenerne.
    if dns.source.as_deref() != Some("resolv.conf") {
        dns.stub = etc_servers.iter().find(|s| N::is_stub(s)).cloned();
    }
    // Socketene.
    let socks = |p: &str, v6: bool| N::parse_sockets(&read(p), v6);
    let mut sockets = N::summarize(&socks("proc/net/tcp", false), &socks("proc/net/tcp6", true), &socks("proc/net/udp", false), &socks("proc/net/udp6", true));
    N::drop_ephemeral_udp(&mut sockets, N::port_range(&read("proc/sys/net/ipv4/ip_local_port_range")));
    owners(root, &mut sockets);
    N::sort_ifaces(&mut ifaces);
    NetInfo {
        t: now,
        source: String::new(),
        nm: nm.is_some(),
        hostname: txt(&root.join("proc/sys/kernel/hostname")),
        ifaces,
        routes,
        dns,
        sockets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nett")
    }

    fn names(_: &str, d: &str) -> (Option<String>, Option<String>) {
        match d {
            "1539" => (Some("Intel".into()), Some("I211 Gigabit Network Connection".into())),
            _ => (None, None),
        }
    }

    #[test]
    fn grensesnittene_fra_fixtures() {
        let root = fixtures();
        let info = read_with(&root, 1000, &read_v4_fixture(&root), None, &mut names);
        let by = |n: &str| info.ifaces.iter().find(|i| i.name == n).unwrap();
        let order: Vec<&str> = info.ifaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(order, vec!["wlo1", "enp24s0", "docker0", "lo"], "standardruten først, loopback sist");
        let wl = by("wlo1");
        assert_eq!(wl.kind, "wifi");
        assert!(wl.default_route && wl.connected());
        assert_eq!(wl.mac.as_deref(), Some("02:00:5E:24:21:C9"));
        assert_eq!(wl.ipv4[0].cidr(), "192.0.2.42/24");
        assert_eq!(wl.ipv6.len(), 2);
        assert_eq!(wl.ipv6[0].scope, "global", "globale før lenkelokale");
        assert_eq!(wl.speed_mbps, None, "Wi-Fi har ingen speed-fil");
        assert_eq!(wl.bus.as_deref(), Some("PCI"));
        assert_eq!(wl.driver.as_deref(), Some("iwlwifi"));
        assert_eq!(wl.bus_addr.as_deref(), Some("0000:17:00.0"));
        let w = wl.wifi.as_ref().unwrap();
        assert_eq!((w.signal_dbm, w.signal_pct), (Some(-52.0), Some(86.0)));
        assert_eq!(w.ssid, None, "uten NetworkManager er SSID ukjent");
        assert_eq!(wl.stats.rx_bytes, Some(123_456_789));
        assert_eq!(wl.stats.rx_dropped, Some(12));
        let eth = by("enp24s0");
        assert_eq!(eth.kind, "ethernet");
        assert_eq!(eth.operstate, "down");
        assert_eq!(eth.carrier, Some(false));
        assert_eq!((eth.speed_mbps, eth.duplex.as_deref()), (None, None), "nede: hastigheten er ukjent");
        assert_eq!(eth.ids.as_deref(), Some("8086:1539"));
        assert_eq!(eth.model.as_deref(), Some("I211 Gigabit Network Connection"));
        assert_eq!(by("docker0").kind, "bridge");
        assert_eq!(by("lo").kind, "loopback");
        assert_eq!(by("lo").mac, None);
        let g = info.routes.gateway4.as_ref().unwrap();
        assert_eq!((g.addr.as_str(), g.iface.as_str()), ("192.0.2.1", "wlo1"));
        assert_eq!(info.dns.servers, vec!["192.0.2.1"]);
        assert_eq!(info.dns.source.as_deref(), Some("resolved"));
        assert_eq!(info.dns.stub.as_deref(), Some("127.0.0.53"));
        assert_eq!(info.dns.domains, vec!["hjemme.lan"]);
        assert_eq!((info.sockets.tcp_established, info.sockets.tcp_listen), (1, 2));
        let l = &info.sockets.listeners[0];
        assert_eq!((l.proto.as_str(), l.port, l.user.as_deref()), ("tcp", 22, Some("root")));
        assert_eq!(info.hostname.as_deref(), Some("rigg-test"));
        assert!(!info.nm);
    }

    #[test]
    fn networkmanager_fyller_inn() {
        let root = fixtures();
        let nm = super::super::nm::NmData {
            primary: Some("wlo1".into()),
            devices: vec![super::super::nm::NmDevice {
                iface: "wlo1".into(),
                connection: Some("Rigg-lab".into()),
                dns: vec!["192.0.2.53".into()],
                domains: vec!["lab.lan".into()],
                wifi: Some(Wifi { ssid: Some("Rigg-lab".into()), freq_mhz: Some(5180), signal_pct: Some(80.0), security: Some("wpa3".into()), ..Default::default() }),
                ..Default::default()
            }],
        };
        let info = read_with(&root, 1000, &[], Some(&nm), &mut names);
        let wl = info.ifaces.iter().find(|i| i.name == "wlo1").unwrap();
        let w = wl.wifi.as_ref().unwrap();
        assert_eq!(w.ssid.as_deref(), Some("Rigg-lab"));
        assert_eq!(w.signal_pct, Some(80.0), "NetworkManagers styrke vinner");
        assert_eq!(w.signal_dbm, Some(-52.0), "dBm fra /proc");
        assert_eq!(wl.connection.as_deref(), Some("Rigg-lab"));
        assert_eq!(info.dns.servers, vec!["192.0.2.53"]);
        assert_eq!(info.dns.source.as_deref(), Some("networkmanager"));
        assert!(info.nm);
    }
}
