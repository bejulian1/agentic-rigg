//! Separat, kortbundet RGB-identifikasjon. Ingen tjener, konfigurasjon,
//! grunnverdifil eller administratoraktivering startes fra denne stien.

use rigg_hw::gpu::palit_rgb;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::Path;

const FLAG: &str = "--rgb-identifiser";

#[derive(Debug, PartialEq, Eq)]
struct ProbeArgs {
    pci: String,
    uuid: String,
}

/// Hele kommandolinjen valideres før noen fil eller enhet undersøkes.
/// Vanlige tjenerargumenter, hjelpeflagg og gjentatte argumenter avvises.
fn parse_args(args: &[OsString]) -> Result<ProbeArgs, String> {
    let mut seen_probe = false;
    let mut pci = None;
    let mut uuid = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let arg = arg.to_str().ok_or("Argumentet er ikke gyldig UTF-8")?;
        match arg {
            FLAG if !seen_probe => seen_probe = true,
            "--pci" if pci.is_none() => {
                pci = Some(value(&mut it, "--pci")?);
            }
            "--uuid" if uuid.is_none() => {
                uuid = Some(value(&mut it, "--uuid")?);
            }
            FLAG | "--pci" | "--uuid" => {
                return Err(format!("Argumentet kan bare brukes én gang: {arg}"));
            }
            _ => {
                return Err(format!(
                    "Argumentet er ikke tillatt ved RGB-identifikasjon: {arg}"
                ));
            }
        }
    }
    if !seen_probe {
        return Err(format!("Mangler {FLAG}"));
    }
    let pci = pci.ok_or("RGB-identifikasjon krever --pci")?;
    let uuid = uuid.ok_or("RGB-identifikasjon krever --uuid")?;
    if !valid_pci(&pci) {
        return Err("--pci må være en full PCI-adresse, for eksempel 0000:1c:00.0".into());
    }
    if !valid_uuid(&uuid) {
        return Err("--uuid må være kortets fullstendige GPU-UUID".into());
    }
    Ok(ProbeArgs { pci, uuid })
}

fn value<'a>(it: &mut impl Iterator<Item = &'a OsString>, flag: &str) -> Result<String, String> {
    let value = it
        .next()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && !s.starts_with('-'))
        .ok_or_else(|| format!("{flag} trenger en gyldig verdi"))?;
    Ok(value.into())
}

fn valid_pci(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && b[8] <= b'1'
        && (b'0'..=b'7').contains(&b[11])
        && b.iter()
            .enumerate()
            .filter(|(i, _)| ![4, 7, 10].contains(i))
            .all(|(_, c)| c.is_ascii_hexdigit())
}

fn valid_uuid(value: &str) -> bool {
    let Some(body) = value.strip_prefix("GPU-") else {
        return false;
    };
    body.len() == 36
        && body.as_bytes().iter().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn failure(code: &str, message: &str) -> Value {
    json!({
        "ok": false,
        "operation": "rgb_identify",
        "error": { "code": code, "message": message },
    })
}

/// Injisert funksjon gjør at rettighetssperren testes uten vertens sysfs,
/// NVML eller I2C. Ingen forsøk på pkexec/sudo gjøres av hjelperen selv.
fn run_with(
    args: &ProbeArgs,
    euid: u32,
    identify: impl FnOnce(&ProbeArgs) -> Result<Value, Value>,
) -> (i32, Value) {
    if euid != 0 {
        return (
            1,
            failure(
                "permission_required",
                "RGB-identifikasjon trenger en separat administratorgodkjenning. Ingen enhet er åpnet.",
            ),
        );
    }
    match identify(args) {
        Ok(report) => (
            0,
            json!({"ok": true, "operation": "rgb_identify", "report": report}),
        ),
        Err(error) => (1, error),
    }
}

pub(super) fn dispatch() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if !args.iter().any(|arg| arg == FLAG) {
        return None;
    }
    let (code, output) = match parse_args(&args) {
        Err(message) => (2, failure("invalid_arguments", &message)),
        Ok(args) => run_with(&args, unsafe { libc::geteuid() }, |args| {
            // Roten er fast. Det finnes ingen CLI-vei til vilkårlig buss,
            // enhetsfil, I2C-adresse, register eller testfixture.
            let report = palit_rgb::identify(Path::new("/"), &args.pci, &args.uuid).map_err(
                |e| json!({"ok": false, "operation": "rgb_identify", "error": e.to_json()}),
            )?;
            serde_json::to_value(report)
                .map_err(|e| failure("report_serialization_failed", &e.to_string()))
        }),
    };
    println!("{output}");
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "GPU-01234567-89ab-cdef-0123-456789abcdef";

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn valid_args() -> Vec<OsString> {
        args(&[FLAG, "--pci", "0000:1c:00.0", "--uuid", UUID])
    }

    #[test]
    fn krever_entydig_kort_og_avviser_tjenerargumenter() {
        assert_eq!(
            parse_args(&valid_args()).unwrap(),
            ProbeArgs {
                pci: "0000:1c:00.0".into(),
                uuid: UUID.into()
            },
        );
        for extra in [
            FLAG,
            "--pci",
            "--uuid",
            "--backend",
            "--socket",
            "--config",
            "--baseline",
            "--tilbakestill",
            "--version",
            "--help",
            "--root",
            "--bus",
            "--register",
        ] {
            let mut mixed = valid_args();
            mixed.push(extra.into());
            assert!(parse_args(&mixed).is_err(), "Godtok {extra}");
        }
        assert!(parse_args(&args(&[FLAG])).is_err());
        assert!(parse_args(&args(&[FLAG, "--pci", "--uuid", UUID])).is_err());
        assert!(parse_args(&args(&[FLAG, "--pci", "0000:1c:00.0", "--uuid"])).is_err());
    }

    #[test]
    fn avviser_stier_og_ufullstendig_identitet() {
        for pci in [
            "../0000:1c:00.0",
            "1c:00.0",
            "0000:1c:00.8",
            "0000:1c:20.0",
            "0000:1c:00.0/",
        ] {
            assert!(parse_args(&args(&[FLAG, "--pci", pci, "--uuid", UUID])).is_err());
        }
        for uuid in [
            "",
            "GPU-fake",
            "GPU-../../",
            "01234567-89ab-cdef-0123-456789abcdef",
        ] {
            assert!(parse_args(&args(&[FLAG, "--pci", "0000:1c:00.0", "--uuid", uuid])).is_err());
        }
    }

    #[test]
    fn rotkrav_stopper_for_enhetstilgang() {
        let parsed = parse_args(&valid_args()).unwrap();
        let (code, output) = run_with(&parsed, 1000, |_| panic!("Ingen maskinvare skal åpnes"));
        assert_eq!(code, 1);
        assert_eq!(output["error"]["code"], "permission_required");
        let (code, output) = run_with(&parsed, 0, |received| {
            assert_eq!(received, &parsed);
            Ok(json!({"fixture": true}))
        });
        assert_eq!(code, 0);
        assert_eq!(output["report"]["fixture"], true);
    }
}
