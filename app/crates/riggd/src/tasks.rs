//! Oppgavebehandling: avgrenset IPC over prøvetrådens prosessbilder.
//! Ingen ny tråd eller historikkbuffer; SIGTERM bruker aldri GPU-hjelperen.

use rigg_core::{tr, trf};
use crate::daemon::Daemon;
use rigg_core::process::{MAX_PAGE_ROWS, ProcessIdentity, ProcessQuery, ProcessSnapshot};
use rigg_core::proto::{ErrorCode, OpResult, ProtoError};
use serde_json::{Value, json};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const TASK_INTERVAL_MS: u32 = 2_000;
pub const TASK_KEEP: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct TaskSnapshot {
    pub processes: ProcessSnapshot,
    pub collected_at: Instant,
    pub gpu_sampled_at_ms: Option<i64>,
    pub gpu_source: &'static str,
    pub gpu_note: String,
}

/// NVML-lista er den samme som brukes av Grafikkort-siden. Ikke koble
/// simulerte GPU-PID-er til ekte /proc, eller presenter GPU-utnyttelse som
/// om NVMLs minneoversikt målte det.
pub fn with_gpu(mut processes: ProcessSnapshot, gpu_lists: &[(String, Value)], nvml: bool, expected_gpu_count: usize, root: &std::path::Path, collected_at: Instant, now_ms: i64) -> TaskSnapshot {
    let valid = nvml
        && !gpu_lists.is_empty()
        // Ellers blir summen feil når for eksempel GPU 2 er mistet mens
        // GPU 1 fortsatt kan rapportere minne for samme prosess.
        && gpu_lists.len() == expected_gpu_count
        && gpu_lists
            .iter()
            .all(|(_, list)| list.get("error").is_none_or(Value::is_null) && list["t"].as_i64().is_some_and(|sampled| (0..=5_000).contains(&now_ms.saturating_sub(sampled))));
    let mut gpu_sampled_at_ms = None;
    if valid {
        let mut vram = std::collections::BTreeMap::<u32, Option<u64>>::new();
        for (_, list) in gpu_lists {
            gpu_sampled_at_ms = Some(gpu_sampled_at_ms.map_or(list["t"].as_i64().unwrap_or(now_ms), |last: i64| last.min(list["t"].as_i64().unwrap_or(now_ms))));
            for process in list["list"].as_array().into_iter().flatten() {
                let Some(pid) = process["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok()) else { continue };
                let bytes = process["vramMiB"]
                    .as_f64()
                    .filter(|mib| mib.is_finite() && *mib >= 0.0 && *mib < (u64::MAX / (1024 * 1024)) as f64)
                    .map(|mib| (mib * 1024.0 * 1024.0) as u64);
                let entry = vram.entry(pid).or_insert(Some(0));
                // Ett ukjent kortbidrag gjør hele PID-summen ukjent, også
                // dersom et senere kort har en gyldig verdi for samme PID.
                *entry = entry.and_then(|total| total.checked_add(bytes?));
            }
        }
        for process in &mut processes.rows {
            if let Some(Some(bytes)) = vram.get(&process.identity.pid)
                && rigg_hw::sys::process::read_identity(root, process.identity.pid) == Some(process.identity)
            {
                process.gpu_vram_bytes = Some(*bytes);
            }
        }
    }
    TaskSnapshot {
        processes,
        collected_at,
        gpu_sampled_at_ms,
        gpu_source: if valid { "nvml" } else { "unavailable" },
        gpu_note: if valid {
            tr!("GPU-minne fra NVML (avrundet til MiB). GPU-bruk per prosess er ikke tilgjengelig.")
        } else {
            tr!("GPU-minne og GPU-bruk per prosess er ikke tilgjengelig.")
        }
        .into(),
    }
}

fn invalid(message: &str) -> ProtoError {
    ProtoError::msg(ErrorCode::InvalidArgument, message)
}

impl Daemon {
    pub(crate) fn op_get_tasks(&mut self, sid: u64, args: &Value) -> OpResult {
        let mut query: ProcessQuery = serde_json::from_value(args.clone()).map_err(|_| invalid(tr!("Ugyldig søk eller sortering av oppgaver.")))?;
        if query.filter.chars().count() > 128 || query.limit == 0 || query.limit > MAX_PAGE_ROWS || query.offset > 1_000_000 {
            return Err(invalid(tr!("Høyst 128 tegn i søket og 1–100 oppgaver per side.")));
        }
        let snapshot_arg = |key| match args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_u64().map(Some).ok_or_else(|| {
                invalid(&trf!("{key} må være et positivt heltall.", key = key))
            }),
        };
        let requested_id = snapshot_arg("snapshotId")?;
        let newer_than = snapshot_arg("newerThan")?;
        if requested_id.is_some() && newer_than.is_some() {
            return Err(invalid(tr!("Velg enten et eksisterende eller et nyere prosessbilde.")));
        }

        let now = Instant::now();
        let expired = self.tasks_until.is_none_or(|until| now >= until);
        let retained = requested_id.and_then(|id| {
            self.sessions.get(&sid).into_iter().flat_map(|s| &s.task_snapshots)
                .chain(self.tasks_previous.iter().filter(|_| !expired))
                .find(|(stored_id, _)| *stored_id == id)
                .map(|(id, snapshot)| (*id, Rc::clone(snapshot)))
                .or_else(|| {
                    self.tasks.as_ref()
                        .filter(|_| !expired && id == self.tasks_snapshot_id)
                        .map(|snapshot| (id, Rc::clone(snapshot)))
                })
        });
        let selected = if retained.is_some() {
            // Navigasjon i et lagret bilde forlenger ikke prosessinnsamlingen.
            retained
        } else {
            if expired {
                self.tasks_generation = self.tasks_generation.wrapping_add(1);
                self.tasks = None;
                self.tasks_previous.clear();
            }
            let until = now + TASK_KEEP;
            self.tasks_until = Some(until);
            self.sampler.send(crate::sampler::Cmd::Tasks {
                generation: self.tasks_generation,
                until,
            });
            self.reschedule();
            self.tasks.as_ref()
                .filter(|_| newer_than.is_none_or(|id| self.tasks_snapshot_id > id))
                .map(|snapshot| (self.tasks_snapshot_id, Rc::clone(snapshot)))
        };
        let snapshot_id = selected.as_ref().map(|(id, _)| *id);
        let snapshot_changed = requested_id.is_some() && requested_id != snapshot_id;
        if snapshot_changed {
            query.offset = 0;
        }
        if let Some((id, snapshot)) = selected.as_ref()
            && let Some(session) = self.sessions.get_mut(&sid)
        {
            session.task_snapshots.retain(|(stored_id, _)| stored_id != id);
            session.task_snapshots.push_back((*id, Rc::clone(snapshot)));
            if session.task_snapshots.len() > 2 {
                session.task_snapshots.pop_front();
            }
        }
        let snapshot = selected.as_ref().map(|(_, snapshot)| snapshot.as_ref());
        let empty = ProcessSnapshot::default();
        let mut page = snapshot.map_or(&empty, |s| &s.processes).page(&query);
        // Klientlisten kan ha endret seg etter prøven. Beskytt alle tilkoblede
        // app-/CLI-prosesser også i knappenes tilgjengelighetsstatus.
        let protected = rigg_hw::sys::process::protected_ancestors(&self.opts.root, &self.task_protected_pids());
        for row in &mut page.rows {
            if let Err(ref reason) = protected {
                row.can_terminate = false;
                row.protected_reason = Some(reason.clone());
            } else if protected.as_ref().is_ok_and(|pids| pids.contains(&row.identity.pid)) {
                row.can_terminate = false;
                row.protected_reason = Some(tr!("Agentic Rigg eller en tilkoblet klient").into());
            }
        }
        let age = snapshot.map(|s| now.saturating_duration_since(s.collected_at).as_millis() as u64);
        let mut value = serde_json::to_value(page).map_err(|_| ProtoError::new(ErrorCode::Internal))?;
        value["snapshotId"] = json!(snapshot_id);
        value["snapshotChanged"] = json!(snapshot_changed);
        value["ageMs"] = json!(age);
        value["loading"] = json!(snapshot.is_none());
        value["stale"] = json!(age.is_none_or(|age| age > 5_000));
        value["intervalMs"] = json!(TASK_INTERVAL_MS);
        value["gpuSampledAtMs"] = json!(snapshot.and_then(|s| s.gpu_sampled_at_ms));
        value["gpuSource"] = json!(snapshot.map_or("unavailable", |s| s.gpu_source));
        value["gpuNote"] = json!(snapshot.map_or(tr!("GPU-bruk per prosess er ikke tilgjengelig."), |s| s.gpu_note.as_str()));
        Ok(value)
    }

    pub(crate) fn op_terminate_task(&mut self, args: &Value) -> OpResult {
        if args.get("confirmed").and_then(Value::as_bool) != Some(true) {
            return Err(invalid(tr!("Avslutning krever en egen bekreftelse.")));
        }
        let pid = args
            .get("pid")
            .and_then(Value::as_u64)
            .filter(|pid| *pid > 1 && *pid <= i32::MAX as u64)
            .ok_or_else(|| invalid(tr!("Ugyldig prosess-ID.")))? as u32;
        let start_ticks = args
            .get("expectedStartTicks")
            .and_then(Value::as_u64)
            .filter(|ticks| *ticks > 0)
            .ok_or_else(|| invalid(tr!("Prosessens starttid må følge med.")))?;
        let protected = self.task_protected_pids();
        if protected.contains(&pid) {
            return Err(ProtoError::msg(ErrorCode::PermissionDenied, tr!("Agentic Rigg og tilkoblede klienter er beskyttet.")));
        }
        rigg_hw::sys::process::terminate_with_protected(&self.opts.root, ProcessIdentity { pid, start_ticks }, &protected).map_err(|message| ProtoError::msg(ErrorCode::PermissionDenied, message))?;
        Ok(json!({"pid": pid, "signal": "SIGTERM", "sent": true}))
    }

    fn task_protected_pids(&self) -> Vec<u32> {
        let mut pids = vec![std::process::id()];
        pids.extend(self.sessions.values().filter_map(|session| session.pid.and_then(|pid| u32::try_from(pid).ok())));
        pids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, start: u64) -> ProcessSnapshot {
        serde_json::from_value(
            json!({"sampledAtMs": 10_000, "rows": [{"pid": pid, "startTicks": start, "ppid": 1, "name": "test", "state": "S", "canTerminate": false}], "unreadable": 0, "error": null}),
        )
        .unwrap()
    }

    #[test]
    fn gpu_minne_krever_fersk_nvml_og_uendret_identitet() {
        let root = std::env::temp_dir().join(format!("riggd-gpu-task-{}", std::process::id()));
        let proc = root.join("proc/123");
        std::fs::create_dir_all(&proc).unwrap();
        let mut fields = vec!["0"; 22];
        fields[0] = "S";
        fields[1] = "1";
        fields[19] = "42";
        fields[21] = "1";
        std::fs::write(proc.join("stat"), format!("123 (test) {}", fields.join(" "))).unwrap();
        let gpu = vec![("gpu0".into(), json!({"t": 10_000, "error": null, "list": [{"pid": 123, "vramMiB": 12.0}]}))];
        let sample = with_gpu(process(123, 42), &gpu, true, 1, &root, Instant::now(), 10_001);
        assert_eq!(sample.processes.rows[0].gpu_vram_bytes, Some(12 * 1024 * 1024));
        assert_eq!(sample.gpu_source, "nvml");
        assert_eq!(
            with_gpu(process(123, 41), &gpu, true, 1, &root, Instant::now(), 10_001).processes.rows[0].gpu_vram_bytes,
            None,
            "gjenbrukt PID skal ikke kobles"
        );
        for (nvml, now) in [(false, 10_001), (true, 15_001)] {
            let sample = with_gpu(process(123, 42), &gpu, nvml, 1, &root, Instant::now(), now);
            assert_eq!(sample.processes.rows[0].gpu_vram_bytes, None, "simulert eller gammel NVML er ukjent");
            assert_eq!(sample.gpu_source, "unavailable");
        }
        let missing_card = with_gpu(process(123, 42), &gpu, true, 2, &root, Instant::now(), 10_001);
        assert_eq!(
            missing_card.processes.rows[0].gpu_vram_bytes, None,
            "en tilgjengelig GPU av to gir aldri en tilsynelatende fullstendig minnesum"
        );
        assert_eq!(missing_card.gpu_source, "unavailable");
        assert_eq!(missing_card.gpu_sampled_at_ms, None);
        let unknown = ("gpu1".into(), json!({"t": 10_000, "error": null, "list": [{"pid": 123, "vramMiB": null}]}));
        for lists in [vec![gpu[0].clone(), unknown.clone()], vec![unknown, gpu[0].clone()]] {
            let sample = with_gpu(process(123, 42), &lists, true, 2, &root, Instant::now(), 10_001);
            assert_eq!(sample.processes.rows[0].gpu_vram_bytes, None, "ukjent minne på ett kort gjør prosessens samlede GPU-minne ukjent");
        }
        let known = vec![gpu[0].clone(), ("gpu1".into(), json!({"t": 10_000, "error": null, "list": [{"pid": 123, "vramMiB": 8.0}]}))];
        assert_eq!(
            with_gpu(process(123, 42), &known, true, 2, &root, Instant::now(), 10_001).processes.rows[0].gpu_vram_bytes,
            Some(20 * 1024 * 1024),
            "fullstendige kortbidrag summeres"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
