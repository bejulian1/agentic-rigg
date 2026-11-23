//! NetworkManager over systembussen (`org.freedesktop.NetworkManager`), uten
//! root og bare lesing: `Properties.GetAll` på enhetene, Wi-Fi-tilgangspunktet,
//! IP-oppsettet og den aktive tilkoblingen. Vi kaller aldri metoder som
//! endrer noe (`ActivateConnection`, `RequestScan` og lignende).
//!
//! Svarene gjøres om til JSON (`udisks::variant_json`) før de tolkes, så
//! tolkingen ([`device_from`]) kan testes uten systembuss.

use super::udisks::variant_json;
use rigg_core::net::{self as N, Wifi};
use serde_json::Value;

const BUS: &str = "org.freedesktop.NetworkManager";
const PATH: &str = "/org/freedesktop/NetworkManager";
const IF_NM: &str = "org.freedesktop.NetworkManager";
const IF_DEV: &str = "org.freedesktop.NetworkManager.Device";
const IF_WIFI: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const IF_AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const IF_IP4: &str = "org.freedesktop.NetworkManager.IP4Config";
const IF_IP6: &str = "org.freedesktop.NetworkManager.IP6Config";
const IF_ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
/// Høyst så lenge ett kall kan holde lesetråden (ms).
const TIMEOUT_MS: i32 = 1500;

/// Det vi bruker fra én enhet i NetworkManager.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NmDevice {
    pub iface: String,
    pub driver: Option<String>,
    pub firmware: Option<String>,
    /// Navnet på den aktive tilkoblingen.
    pub connection: Option<String>,
    pub dns: Vec<String>,
    pub domains: Vec<String>,
    pub wifi: Option<Wifi>,
}

/// Svaret fra NetworkManager.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NmData {
    pub devices: Vec<NmDevice>,
    /// Grensesnittet til den primære tilkoblingen.
    pub primary: Option<String>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty() && *x != "/").map(str::to_string)
}

fn u(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(Value::as_u64)
}

/// Én enhet fra egenskapene (alle som JSON). `ip6_dns` er IPv6-tjenerne,
/// som NetworkManager gir som rå byte (`aay`).
pub fn device_from(dev: &Value, wireless: Option<&Value>, ap: Option<&Value>, ip4: Option<&Value>, ip6_dns: Vec<String>, active: Option<&Value>) -> NmDevice {
    let mut d = NmDevice {
        iface: s(dev, "Interface").unwrap_or_default(),
        driver: s(dev, "Driver"),
        firmware: s(dev, "FirmwareVersion").filter(|f| f != "N/A"),
        connection: active.and_then(|a| s(a, "Id")),
        ..Default::default()
    };
    if let Some(ip4) = ip4 {
        if let Some(list) = ip4.get("NameserverData").and_then(Value::as_array) {
            d.dns.extend(list.iter().filter_map(|x| s(x, "address")));
        }
        for k in ["Domains", "Searches"] {
            for x in ip4.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                if !d.domains.iter().any(|y| y == x) {
                    d.domains.push(x.to_string());
                }
            }
        }
    }
    for a in ip6_dns {
        if !d.dns.contains(&a) {
            d.dns.push(a);
        }
    }
    if let Some(w) = wireless {
        let mut wifi = Wifi { bitrate_mbps: u(w, "Bitrate").filter(|b| *b > 0).map(|b| b as f64 / 1000.0), ..Default::default() };
        if let Some(ap) = ap {
            wifi.ssid = ap.get("Ssid").and_then(Value::as_str).and_then(|x| N::ssid_text(x.as_bytes()));
            wifi.bssid = s(ap, "HwAddress");
            wifi.signal_pct = u(ap, "Strength").map(|v| v.min(100) as f64);
            wifi.freq_mhz = u(ap, "Frequency").filter(|f| *f > 0).map(|f| f as u32);
            let flag = |k: &str| u(ap, k).unwrap_or(0) as u32;
            wifi.security = Some(N::wifi_security(flag("Flags"), flag("WpaFlags"), flag("RsnFlags")).to_string());
        }
        d.wifi = Some(wifi);
    }
    d
}

/// IPv6-adresser fra `aay` (16 byte per adresse).
fn ip6_list(v: &glib::Variant) -> Vec<String> {
    (0..v.n_children())
        .filter_map(|i| {
            let c = v.child_value(i);
            let b = c.fixed_array::<u8>().ok()?;
            let a: [u8; 16] = b.try_into().ok()?;
            Some(std::net::Ipv6Addr::from(a).to_string())
        })
        .collect()
}

pub struct Nm {
    conn: Option<gio::DBusConnection>,
}

impl Default for Nm {
    fn default() -> Self {
        Self::new()
    }
}

impl Nm {
    pub fn new() -> Nm {
        Nm { conn: None }
    }

    fn conn(&mut self) -> Option<gio::DBusConnection> {
        if let Some(c) = &self.conn
            && !c.is_closed()
        {
            return Some(c.clone());
        }
        let c = gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>).ok()?;
        self.conn = Some(c.clone());
        Some(c)
    }

    fn raw(&mut self, path: &str, method: &str, args: &glib::Variant) -> Option<glib::Variant> {
        let c = self.conn()?;
        c.call_sync(Some(BUS), path, "org.freedesktop.DBus.Properties", method, Some(args), None, gio::DBusCallFlags::NO_AUTO_START, TIMEOUT_MS, None::<&gio::Cancellable>)
            .ok()
            .map(|v| v.child_value(0))
    }

    fn all(&mut self, path: &str, iface: &str) -> Option<Value> {
        if path.is_empty() || path == "/" {
            return None;
        }
        self.raw(path, "GetAll", &(iface,).into()).map(|v| variant_json(&v))
    }

    /// Alle enhetene med IP-oppsett, eller `None` når NetworkManager ikke svarer.
    pub fn read(&mut self) -> Option<NmData> {
        let top = self.all(PATH, IF_NM)?;
        let mut data = NmData::default();
        let primary = s(&top, "PrimaryConnection");
        for p in top.get("Devices").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            let Some(dev) = self.all(p, IF_DEV) else { continue };
            let active_path = s(&dev, "ActiveConnection");
            let active = active_path.as_deref().and_then(|a| self.all(a, IF_ACTIVE));
            let ip4 = s(&dev, "Ip4Config").and_then(|c| self.all(&c, IF_IP4));
            let ip6_dns = s(&dev, "Ip6Config")
                .and_then(|c| self.raw(&c, "Get", &(IF_IP6, "Nameservers").into()))
                .map(|v| ip6_list(&v.as_variant().unwrap_or(v)))
                .unwrap_or_default();
            // DeviceType 2 = Wi-Fi.
            let wireless = (u(&dev, "DeviceType") == Some(2)).then(|| self.all(p, IF_WIFI)).flatten();
            let ap = wireless.as_ref().and_then(|w| s(w, "ActiveAccessPoint")).and_then(|a| self.all(&a, IF_AP));
            let d = device_from(&dev, wireless.as_ref(), ap.as_ref(), ip4.as_ref(), ip6_dns, active.as_ref());
            if primary.is_some() && active_path == primary {
                data.primary = Some(d.iface.clone());
            }
            data.devices.push(d);
        }
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enhet_fra_egenskapene() {
        let dev = json!({"Interface": "wlo1", "Driver": "iwlwifi", "FirmwareVersion": "89.202a2f7b.0 9260-th-b0-jf-b0-", "DeviceType": 2, "Ip4Config": "/org/freedesktop/NetworkManager/IP4Config/3"});
        let w = json!({"Bitrate": 866700, "ActiveAccessPoint": "/org/freedesktop/NetworkManager/AccessPoint/7"});
        let ap = json!({"Ssid": "Rigg-lab", "HwAddress": "02:00:5E:10:00:01", "Strength": 82, "Frequency": 5180, "Flags": 1, "WpaFlags": 0, "RsnFlags": 0x188});
        let ip4 = json!({"NameserverData": [{"address": "192.0.2.1"}, {"address": "192.0.2.2"}], "Domains": ["hjemme.lan"], "Searches": ["hjemme.lan", "lab.lan"]});
        let active = json!({"Id": "Rigg-lab"});
        let d = device_from(&dev, Some(&w), Some(&ap), Some(&ip4), vec!["2001:db8::1".into(), "192.0.2.1".into()], Some(&active));
        assert_eq!(d.iface, "wlo1");
        assert_eq!(d.driver.as_deref(), Some("iwlwifi"));
        assert_eq!(d.connection.as_deref(), Some("Rigg-lab"));
        assert_eq!(d.dns, vec!["192.0.2.1", "192.0.2.2", "2001:db8::1"]);
        assert_eq!(d.domains, vec!["hjemme.lan", "lab.lan"]);
        let wifi = d.wifi.unwrap();
        assert_eq!(wifi.ssid.as_deref(), Some("Rigg-lab"));
        assert_eq!(wifi.signal_pct, Some(82.0));
        assert_eq!(wifi.freq_mhz, Some(5180));
        assert_eq!(wifi.bitrate_mbps, Some(866.7));
        assert_eq!(wifi.security.as_deref(), Some("wpa2"));
        // Kablet uten tilkobling: ingen Wi-Fi, «/» betyr ingenting.
        let e = device_from(&json!({"Interface": "enp24s0", "Driver": "igb", "FirmwareVersion": "N/A", "Ip4Config": "/"}), None, None, None, vec![], None);
        assert_eq!((e.firmware, e.connection, e.wifi), (None, None, None));
    }
}
