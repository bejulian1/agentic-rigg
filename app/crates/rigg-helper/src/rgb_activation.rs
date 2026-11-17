//! Avgrensede administratorkommandoer. Ingen tjenestestart eller NVML.
//! Vanlig oppstart kommer aldri hit. Tester injiserer utføreren før HW.

use crate::rgb_backend::NativeBackend;
use crate::{rgb_probe, rgb_store};
use rigg_core::rgb::Baseline;
use rigg_core::rgb_auth::Authorization;
use rigg_hw::gpu::palit_rgb;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::Path;

const FLAGS: &[&str] = &[
    "--rgb-forbered",
    "--rgb-avklar",
    "--rgb-gjenopprett",
    "--rgb-adapter",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Prepare,
    Resolve,
    Restore,
    Metadata,
}

#[derive(Debug)]
struct Args {
    mode: Mode,
    pci: String,
    uuid: String,
    uid: u32,
}

fn parse(args: &[OsString]) -> Result<Args, String> {
    let mut mode = None;
    let mut pci = None;
    let mut uuid = None;
    let mut uid = None;
    let mut static_confirmed = false;
    let mut restore_original = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let arg = arg.to_str().ok_or("Ugyldig UTF-8 i RGB-argument.")?;
        match arg {
            flag if FLAGS.contains(&flag) && mode.is_none() => {
                mode = Some(match flag {
                    "--rgb-forbered" => Mode::Prepare,
                    "--rgb-avklar" => Mode::Resolve,
                    "--rgb-gjenopprett" => Mode::Restore,
                    _ => Mode::Metadata,
                });
            }
            "--pci" if pci.is_none() => pci = Some(value(&mut it)?),
            "--uuid" if uuid.is_none() => uuid = Some(value(&mut it)?),
            "--uid" if uid.is_none() => {
                uid = Some(
                    value(&mut it)?
                        .parse::<u32>()
                        .map_err(|_| "Ugyldig RGB-bruker.")?,
                );
            }
            "--static-confirmed" if !static_confirmed => static_confirmed = true,
            "--restore-original" if !restore_original => restore_original = true,
            _ => {
                return Err(format!(
                    "Ukjent, duplisert eller blandet RGB-argument: {arg}"
                ));
            }
        }
    }
    let mode = mode.ok_or("Mangler RGB-administratorkommando.")?;
    let pci = pci.ok_or("Mangler --pci.")?.to_ascii_lowercase();
    let uuid = uuid.ok_or("Mangler --uuid.")?;
    let uid = uid
        .filter(|uid| *uid != 0 && *uid != u32::MAX)
        .ok_or("Mangler gyldig --uid.")?;
    if !rgb_probe::valid_pci(&pci) || !pci.ends_with(".0") || !rgb_probe::valid_uuid(&uuid) {
        return Err("RGB trenger valgt grafikkfunksjon og full GPU-UUID.".into());
    }
    let valid = match mode {
        Mode::Prepare | Mode::Resolve => static_confirmed && !restore_original,
        Mode::Restore => restore_original && !static_confirmed,
        Mode::Metadata => !static_confirmed && !restore_original,
    };
    if !valid {
        return Err("RGB-handlingen mangler riktig uttrykkelig bekreftelse.".into());
    }
    Ok(Args {
        mode,
        pci,
        uuid,
        uid,
    })
}

fn value<'a>(it: &mut impl Iterator<Item = &'a OsString>) -> Result<String, String> {
    it.next()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .map(str::to_owned)
        .ok_or_else(|| "RGB-argumentet mangler verdi.".into())
}

fn operation(mode: Mode) -> &'static str {
    match mode {
        Mode::Prepare => "rgb_prepare",
        Mode::Resolve => "rgb_resolve",
        Mode::Restore => "rgb_restore",
        Mode::Metadata => "rgb_metadata",
    }
}

fn run_with(
    args: &[OsString],
    euid: u32,
    execute: impl FnOnce(Args) -> Result<Value, String>,
) -> Option<(Value, i32)> {
    if !args.iter().any(|arg| FLAGS.iter().any(|flag| arg == flag)) {
        return None;
    }
    let result = parse(args).and_then(|args| {
        if euid != 0 {
            return Err("RGB-administrasjon krever eksplisitt administratorgodkjenning.".into());
        }
        let op = operation(args.mode);
        let report = execute(args)?;
        Ok(json!({"ok": true, "operation": op, "report": report}))
    });
    Some(match result {
        Ok(report) => (report, 0),
        Err(message) => (json!({"ok": false, "error": message}), 1),
    })
}

fn execute(args: Args) -> Result<Value, String> {
    if let Some(uid) = std::env::var_os("PKEXEC_UID") {
        let invoking_uid = uid.to_str().and_then(|s| s.parse::<u32>().ok());
        if invoking_uid != Some(args.uid) {
            return Err("RGB-brukeren samsvarer ikke med administratorforespørselen.".into());
        }
    }
    match args.mode {
        Mode::Prepare => {
            let before = palit_rgb::metadata_preview(Path::new("/"), &args.pci, &args.uuid)
                .map_err(|e| e.to_string())?;
            let discovery = palit_rgb::identify(Path::new("/"), &args.pci, &args.uuid)
                .map_err(|e| e.to_string())?;
            let after = palit_rgb::metadata_preview(Path::new("/"), &args.pci, &args.uuid)
                .map_err(|e| e.to_string())?;
            if before != after {
                return Err("RGB-adapteren ble endret under identifikasjonen.".into());
            }
            let binding = discovery
                .binding
                .ok_or("RGB-kontrolleren ble ikke identifisert.")?;
            let baseline = Baseline::user_confirmed_static(binding.clone(), discovery.observed)?;
            let authorization = Authorization::new(args.uid, binding, rigg_core::version::build())?;
            rgb_store::prepare_production(&authorization, &baseline)?;
            Ok(
                json!({"metadata": after.to_json(), "authorization": authorization, "baseline": baseline}),
            )
        }
        Mode::Resolve => {
            let evidence =
                NativeBackend::resolve_production(args.uid, &args.pci, &args.uuid, true)?;
            serde_json::to_value(evidence).map_err(|e| e.to_string())
        }
        Mode::Restore => {
            let observed =
                NativeBackend::restore_production(args.uid, &args.pci, &args.uuid, true)?;
            Ok(
                json!({"observed": observed, "readback": "readable_fields_match",
                "needsVisualConfirmation": true}),
            )
        }
        Mode::Metadata => {
            let metadata = palit_rgb::metadata_preview(Path::new("/"), &args.pci, &args.uuid)
                .map_err(|e| e.to_string())?;
            Ok(json!({"metadata": metadata.to_json()}))
        }
    }
}

pub fn dispatch() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // SAFETY: leser kun eksisterende prosessrettigheter. Ingen reserve til
    // testrot eller alternativ enhet finnes i administratorstien.
    let (report, code) = run_with(&args, unsafe { libc::geteuid() }, execute)?;
    println!("{report}");
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(flag: &str) -> Vec<OsString> {
        [
            flag,
            "--pci",
            "0000:1c:00.0",
            "--uuid",
            "GPU-11111111-2222-3333-4444-555555555555",
            "--uid",
            "1000",
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }

    #[test]
    fn egen_mode_root_og_bekreftelse_kreves_for_utforer() {
        assert!(run_with(&["--version".into()], 0, |_| panic!()).is_none());
        let mut valid = args("--rgb-forbered");
        valid.push("--static-confirmed".into());
        assert_eq!(
            run_with(&valid, 1000, |_| panic!("Ingen HW uten root"))
                .unwrap()
                .1,
            1
        );
        assert_eq!(
            run_with(&valid, 0, |args| {
                assert_eq!(args.uid, 1000);
                Ok(json!({"fake": true}))
            })
            .unwrap()
            .1,
            0
        );
        assert!(parse(&args("--rgb-forbered")).is_err());
    }

    #[test]
    fn vilkarlige_stier_modeblanding_duplikater_og_falsk_bekreftelse_avvises() {
        for extra in [
            vec!["--static-confirmed", "--root", "/"],
            vec!["--static-confirmed", "--pci", "0000:1c:00.0"],
            vec!["--static-confirmed", "--rgb-server"],
            vec!["--static-confirmed", "--rgb-avklar"],
            vec!["--restore-original"],
        ] {
            let mut test = args("--rgb-forbered");
            test.extend(extra.into_iter().map(OsString::from));
            assert!(parse(&test).is_err());
        }
        let mut restore = args("--rgb-gjenopprett");
        restore.push("--restore-original".into());
        assert_eq!(parse(&restore).unwrap().mode, Mode::Restore);
        let mut metadata = args("--rgb-adapter");
        assert!(parse(&metadata).is_ok());
        metadata.push("--static-confirmed".into());
        assert!(parse(&metadata).is_err());
    }
}
