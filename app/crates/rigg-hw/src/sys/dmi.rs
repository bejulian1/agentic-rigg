//! Hovedkort og BIOS fra `/sys/class/dmi/id` (port av `readBoard` i
//! `server/system.js`). Serienumrene og `product_uuid` er 0400 root og
//! **åpnes ikke**; de står bare som «krever root».
//!
//! Minnemodulene: SPD-brikkene (`ee1004`) telles ut fra navnene på enhetene
//! driveren har bundet (`/sys/bus/i2c/drivers/ee1004/<buss>-<adresse>`).
//! Selve EEPROM-fila leses **ikke**: det er I2C-trafikk på hovedkortets
//! SMBus og krever brukerens godkjenning (maskinvare-revisjon kap. 10).

use serde_json::{Map, Value, json};
use std::path::Path;

/// Filene som leses. Serienumre og `product_uuid` står bevisst ikke her.
pub const DMI_FILES: &[&str] = &[
    "sys_vendor",
    "product_name",
    "product_version",
    "product_family",
    "product_sku",
    "board_vendor",
    "board_name",
    "board_version",
    "bios_vendor",
    "bios_version",
    "bios_date",
    "bios_release",
    "chassis_vendor",
    "chassis_type",
    "chassis_version",
];

/// Filene som bare root kan lese. De åpnes ikke; navnene står i rapporten.
pub const ROOT_ONLY: &[&str] = &["board_serial", "chassis_serial", "product_serial", "product_uuid"];

/// Plassholdere fra produsenten er ikke informasjon.
pub fn placeholder(s: &str) -> bool {
    let l = s.trim().to_ascii_lowercase();
    l.is_empty()
        || matches!(
            l.as_str(),
            "to be filled by o.e.m." | "default string" | "system product name" | "system version" | "not applicable" | "none" | "n/a" | "0123456789" | "type1productconfigid"
        )
}

/// SMBIOS-kabinettypene som er vanlige på skrivebordet.
pub fn chassis_name(t: u32) -> Option<&'static str> {
    Some(match t {
        3 => "Stasjonær",
        4 => "Lavprofil stasjonær",
        5 => "Pizzaboks",
        6 => "Minitårn",
        7 => "Tårn",
        8 | 9 | 10 | 14 => "Bærbar",
        11 => "Håndholdt",
        13 => "Alt-i-ett",
        15 => "Plassbesparende",
        17 => "Server (hovedenhet)",
        23 => "Rackmontert",
        30 => "Nettbrett",
        31 | 32 => "Konvertibel",
        35 => "Mini-PC",
        36 => "Stick-PC",
        _ => return None,
    })
}

/// Antall SPD-brikker driveren `ee1004` (DDR4) eller `spd5118` (DDR5) har bundet.
fn spd_chips(root: &Path) -> (usize, Option<&'static str>) {
    for drv in ["ee1004", "spd5118"] {
        let dir = root.join("sys/bus/i2c/drivers").join(drv);
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        // Bare navnene («7-0050»), ikke innholdet.
        let n = rd
            .flatten()
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                n.split_once('-').is_some_and(|(b, a)| b.chars().all(|c| c.is_ascii_digit()) && a.len() == 4)
            })
            .count();
        if n > 0 {
            return (n, Some(drv));
        }
    }
    (0, None)
}

pub fn read(root: &Path) -> Value {
    let dir = root.join("sys/class/dmi/id");
    let mut out = Map::new();
    for f in DMI_FILES {
        let v = match std::fs::read_to_string(dir.join(f)) {
            Ok(s) if !placeholder(&s) => Value::String(s.trim().to_string()),
            Ok(_) => Value::Null,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => json!({"root": true}),
            Err(_) => Value::Null,
        };
        out.insert((*f).to_string(), v);
    }
    if let Some(t) = out.get("chassis_type").and_then(Value::as_str).and_then(|s| s.parse::<u32>().ok()) {
        out.insert("chassis".into(), json!(chassis_name(t).map(str::to_string).unwrap_or_else(|| format!("Type {t}"))));
    }
    // Hvilke root-filer som finnes (uten å åpne dem).
    let root_files: Vec<&str> = ROOT_ONLY.iter().copied().filter(|f| dir.join(f).exists()).collect();
    out.insert("rootOnly".into(), json!(root_files));
    let (n, drv) = spd_chips(root);
    out.insert("spd".into(), json!({"chips": n, "driver": drv}));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plassholdere_og_kabinett() {
        assert!(placeholder("To be filled by O.E.M."));
        assert!(placeholder("  "));
        assert!(!placeholder("MS-7B85"));
        assert_eq!(chassis_name(3), Some("Stasjonær"));
        assert_eq!(chassis_name(99), None);
    }

    #[test]
    fn falsk_rot_uten_serienumre() {
        let root = std::env::temp_dir().join(format!("rigg-dmi-{}", std::process::id()));
        let d = root.join("sys/class/dmi/id");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("board_vendor"), "Micro-Star International Co., Ltd.\n").unwrap();
        std::fs::write(d.join("board_name"), "B450 GAMING PRO CARBON AC (MS-7B85)\n").unwrap();
        std::fs::write(d.join("product_family"), "To be filled by O.E.M.\n").unwrap();
        std::fs::write(d.join("chassis_type"), "3\n").unwrap();
        std::fs::write(d.join("board_serial"), "HEMMELIG\n").unwrap();
        let s = root.join("sys/bus/i2c/drivers/ee1004");
        for a in ["7-0050", "7-0051", "bind", "module"] {
            std::fs::create_dir_all(s.join(a)).unwrap();
        }
        let v = read(&root);
        assert_eq!(v["board_name"], "B450 GAMING PRO CARBON AC (MS-7B85)");
        assert!(v["product_family"].is_null());
        assert_eq!(v["chassis"], "Stasjonær");
        assert_eq!(v["spd"]["chips"], 2);
        assert_eq!(v["rootOnly"], json!(["board_serial"]));
        assert!(!v.to_string().contains("HEMMELIG"), "serienummeret skal aldri leses");
        let _ = std::fs::remove_dir_all(&root);
    }
}
