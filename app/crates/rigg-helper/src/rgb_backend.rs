//! Privilegert RGB-adapter. Oppstart leser bare private filer og tar en lås;
//! ingen grunnverdier fanges automatisk og ingen journal gjenopprettes da.

use crate::rgb_store::{Operation, Phase, PrivateStore, StoredJournal, load_authorization};
use rigg_core::rgb::{
    Baseline, BaselineSource, Binding, Discovery, DiscoveryState, Observed, RecoveryReadback,
    Request,
};
use rigg_core::rgb_auth::{self, Authorization};
use rigg_core::rgb_txn::{Backend, Book, Journal, WriteError};
use rigg_hw::gpu::palit_rgb;
use std::path::Path;

trait Hardware: Send {
    fn read(&mut self, binding: &Binding) -> Result<Discovery, String>;
    fn apply(
        &mut self,
        binding: &Binding,
        request: &Request,
        before: &Observed,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError>;
    fn recover(
        &mut self,
        binding: &Binding,
        request: &Request,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError>;
}

struct Physical;

fn hardware_result(
    result: Result<palit_rgb::ApplyReadback, palit_rgb::ApplyFailure>,
    expected: &Binding,
) -> Result<Observed, WriteError> {
    match result {
        Ok(report) if &report.binding == expected => Ok(report.observed),
        Ok(_) => Err(WriteError {
            message: "RGB-transportens tilbakelesing har feil kortbinding.".into(),
            write_attempted: true,
        }),
        Err(e) => Err(WriteError {
            message: e.to_string(),
            // Et feilet første ioctl kan allerede ha påvirket kontrolleren.
            write_attempted: e.writes_attempted != 0,
        }),
    }
}

impl Hardware for Physical {
    fn read(&mut self, binding: &Binding) -> Result<Discovery, String> {
        palit_rgb::identify(Path::new("/"), &binding.pci, &binding.uuid).map_err(|e| e.to_string())
    }

    fn apply(
        &mut self,
        binding: &Binding,
        request: &Request,
        before: &Observed,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        hardware_result(
            palit_rgb::apply(Path::new("/"), binding, request, baseline, before),
            binding,
        )
    }

    fn recover(
        &mut self,
        binding: &Binding,
        request: &Request,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        hardware_result(
            palit_rgb::recover(Path::new("/"), binding, request, baseline),
            binding,
        )
    }
}

pub(crate) struct NativeBackend {
    authorization: Authorization,
    baseline: Baseline,
    store: PrivateStore,
    record: Option<StoredJournal>,
    own_write_responsibility: bool,
    last_recovery: Option<Observed>,
    hardware: Box<dyn Hardware>,
    production: bool,
}

impl NativeBackend {
    /// Fast produksjonsoppsett, uten maskinvareåpning. Manglende baseline eller
    /// enhver eksisterende journal sperrer oppstart. Prepared beviser ikke
    /// at kortet er urørt, og utløser heller ikke automatisk gjenoppretting.
    pub fn open_production() -> Result<(Authorization, Baseline, Self), String> {
        // SAFETY: ren avlesning; ingen rettighetsøkning eller fallback.
        if unsafe { libc::geteuid() } != 0 {
            return Err("RGB-arbeideren krever separat administratoraktivering.".into());
        }
        let authorization = load_authorization(Path::new(rgb_auth::CONFIG_PATH))?;
        let store = PrivateStore::open_production()?;
        let baseline = store.baseline()?;
        let backend = Self::from_parts(
            authorization.clone(),
            baseline.clone(),
            store,
            Box::new(Physical),
            true,
        )?;
        Ok((authorization, baseline, backend))
    }

    fn from_parts(
        authorization: Authorization,
        baseline: Baseline,
        store: PrivateStore,
        hardware: Box<dyn Hardware>,
        production: bool,
    ) -> Result<Self, String> {
        baseline.binding().validate()?;
        Book::new(baseline.clone()).map_err(|e| e.to_string())?;
        if production && baseline.source() != BaselineSource::UserConfirmedStatic {
            return Err(
                "Palit-transporten krever et særskilt brukerbekreftet statisk grunnopptak.".into(),
            );
        }
        if !rgb_auth::compatible_build(authorization.build())
            || !authorization.allows(authorization.uid(), baseline.binding())
        {
            return Err("RGB-tillatelsen omfatter ikke grunnopptakets kort og hjelperbygg.".into());
        }
        if store.baseline()? != baseline {
            return Err("Det lagrede RGB-grunnopptaket samsvarer ikke.".into());
        }
        if let Some(record) = store.journal(&baseline)? {
            return Err(format!(
                "RecoveryRequired: eksisterende RGB-journal ({:?}); separat avklaring kreves. Ingen maskinvare er åpnet.",
                record.phase
            ));
        }
        Ok(Self {
            authorization,
            baseline,
            store,
            record: None,
            own_write_responsibility: false,
            last_recovery: None,
            hardware,
            production,
        })
    }

    fn binding(&self, binding: &Binding, baseline: &Baseline) -> Result<(), String> {
        if binding != self.authorization.binding() || baseline != &self.baseline {
            return Err(
                "RGB-operasjonen bruker ikke den autoriserte, opprinnelige kortbindingen.".into(),
            );
        }
        Ok(())
    }

    fn current_authorization(&self) -> Result<(), String> {
        if self.production {
            let current = load_authorization(Path::new(rgb_auth::CONFIG_PATH))?;
            if current != self.authorization {
                return Err("RGB-tillatelsen er endret eller tilbakekalt.".into());
            }
        }
        Ok(())
    }

    fn persist(&mut self, next: StoredJournal) -> Result<(), String> {
        let result = self
            .store
            .write_journal(&self.baseline, self.record.as_ref(), &next);
        if result.is_ok() {
            self.record = Some(next);
        } else if let Ok(on_disk) = self.store.journal(&self.baseline) {
            // rename kan ha lykkes før fsync av mappen feilet. Bevar et kjent
            // gammelt/nytt dokument, men rapporter fortsatt I/O-feilen.
            if on_disk.as_ref() == Some(&next) {
                self.record = on_disk;
            }
        }
        result
    }

    fn finish_write(
        &mut self,
        result: Result<Observed, WriteError>,
        recovery: bool,
    ) -> Result<Observed, WriteError> {
        let attempted = result
            .as_ref()
            .map(|_| true)
            .unwrap_or_else(|e| e.write_attempted);
        self.own_write_responsibility |= attempted;
        let mut next = self.record.clone().ok_or_else(|| WriteError {
            message: "RGB-skriving mangler journal.".into(),
            write_attempted: attempted,
        })?;
        next.phase = if attempted {
            Phase::Attempted
        } else {
            Phase::RejectedBeforeWrite
        };
        if let Err(error) = self.persist(next) {
            let hardware_error = result
                .err()
                .map(|e| format!("; {}", e.message))
                .unwrap_or_default();
            return Err(WriteError {
                message: format!(
                    "RGB-journalen kunne ikke oppdateres etter operasjonen: {error}{hardware_error}"
                ),
                write_attempted: attempted,
            });
        }
        if recovery && let Ok(observed) = &result {
            self.last_recovery = Some(observed.clone());
        }
        result
    }
}

fn no_write(message: impl Into<String>) -> WriteError {
    WriteError {
        message: message.into(),
        write_attempted: false,
    }
}

impl Backend for NativeBackend {
    fn read(&mut self, binding: &Binding) -> Result<Observed, String> {
        self.binding(binding, &self.baseline)?;
        self.current_authorization()?;
        self.store
            .validate_state(&self.baseline, self.record.as_ref())?;
        let report = self.hardware.read(binding)?;
        if report.state != DiscoveryState::IdentifiedReadOnly
            || report.binding.as_ref() != Some(binding)
        {
            return Err("Fersk RGB-identifikasjon samsvarer ikke med hele kortbindingen.".into());
        }
        Ok(report.observed)
    }

    fn mark_dirty(&mut self, journal: &Journal) -> Result<(), String> {
        self.current_authorization()?;
        journal.validate(self.authorization.binding())?;
        if journal.baseline != self.baseline {
            return Err("RGB-journalen kan ikke erstatte det opprinnelige grunnopptaket.".into());
        }
        let previous = self.own_write_responsibility
            || self
                .record
                .as_ref()
                .is_some_and(StoredJournal::has_recorded_write_responsibility);
        self.last_recovery = None;
        self.persist(StoredJournal {
            version: 1,
            journal: journal.clone(),
            operation: Operation::Apply,
            target: journal.requested.clone(),
            phase: Phase::Prepared,
            prior_write_responsibility: previous,
        })
    }

    fn apply(
        &mut self,
        binding: &Binding,
        request: &Request,
        expected_before: &Observed,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        self.binding(binding, baseline).map_err(no_write)?;
        let record = self
            .record
            .as_ref()
            .ok_or_else(|| no_write("RGB-skriving mangler varig skrivehensikt."))?;
        if record.phase != Phase::Prepared
            || record.operation != Operation::Apply
            || &record.target != request
            || &record.journal.before_observed != expected_before
        {
            return Err(no_write(
                "RGB-skriving samsvarer ikke med den forberedte journalen.",
            ));
        }
        self.last_recovery = None;
        let preflight = self.current_authorization().and_then(|_| {
            self.store
                .validate_state(&self.baseline, self.record.as_ref())
        });
        let result = match preflight {
            Ok(()) => self
                .hardware
                .apply(binding, request, expected_before, baseline),
            Err(e) => Err(no_write(e)),
        };
        self.finish_write(result, false)
    }

    fn recover(
        &mut self,
        binding: &Binding,
        request: &Request,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        self.binding(binding, baseline).map_err(no_write)?;
        let mut next = self
            .record
            .clone()
            .ok_or_else(|| no_write("RGB-gjenoppretting mangler egen journal."))?;
        let previous = self.own_write_responsibility || next.has_recorded_write_responsibility();
        next.prior_write_responsibility |= previous;
        next.operation = Operation::Recover;
        next.target = request.clone();
        next.phase = Phase::Prepared;
        next.validate(baseline).map_err(no_write)?;
        self.last_recovery = None;
        // Tilbakekalt tillatelse må ikke sperre gjenoppretting av egne endringer.
        // Kort/UUID/signatur kontrolleres ferskt av den egne recover-transporten.
        self.persist(next).map_err(no_write)?;
        let result = self.hardware.recover(binding, request, baseline);
        self.finish_write(result, true)
    }

    fn clear_dirty(&mut self, binding: &Binding) -> Result<(), String> {
        self.binding(binding, &self.baseline)?;
        let verified = self.last_recovery.as_ref().is_some_and(|observed| {
            self.baseline.recovery_readback(binding, observed) == RecoveryReadback::Verified
        });
        if !verified {
            return Err(
                "RGB-journalen beholdes: opprinnelig modus og grunnverdier er ikke verifisert."
                    .into(),
            );
        }
        let record = self.record.as_ref().ok_or("RGB-journalen mangler.")?;
        self.store.clear_journal(binding, &self.baseline, record)?;
        self.record = None;
        self.own_write_responsibility = false;
        self.last_recovery = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgb_store::fixtures::{
        self, Fixture, authorization, baseline, binding, journal, observed,
    };
    use rigg_core::rgb::{Mode, Signature};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    struct FakeState {
        actions: Vec<&'static str>,
        binding: Binding,
        result: Result<Observed, WriteError>,
        fail_persistence_after_write: Option<Arc<AtomicBool>>,
    }

    struct Fake(Arc<Mutex<FakeState>>);

    impl Hardware for Fake {
        fn read(&mut self, _: &Binding) -> Result<Discovery, String> {
            let mut state = self.0.lock().unwrap();
            state.actions.push("read");
            Discovery::identified(state.binding.clone(), observed())
        }

        fn apply(
            &mut self,
            _: &Binding,
            _: &Request,
            _: &Observed,
            _: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.write("apply")
        }

        fn recover(
            &mut self,
            _: &Binding,
            _: &Request,
            _: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.write("recover")
        }
    }

    impl Fake {
        fn write(&self, name: &'static str) -> Result<Observed, WriteError> {
            let mut state = self.0.lock().unwrap();
            state.actions.push(name);
            if let Some(fault) = &state.fail_persistence_after_write {
                fault.store(true, Ordering::Relaxed);
            }
            state.result.clone()
        }
    }

    fn hardware() -> (Box<dyn Hardware>, Arc<Mutex<FakeState>>) {
        let state = Arc::new(Mutex::new(FakeState {
            actions: vec![],
            binding: binding(),
            result: Ok(Observed {
                color: Some([0, 0, 0]),
                brightness_raw: Some(100),
                mode: None,
            }),
            fail_persistence_after_write: None,
        }));
        (Box::new(Fake(state.clone())), state)
    }

    fn backend(baseline: &Baseline) -> (Fixture, NativeBackend, Arc<Mutex<FakeState>>) {
        let f = Fixture::new();
        let store = f.store(baseline);
        let (hardware, state) = hardware();
        let backend =
            NativeBackend::from_parts(authorization(), baseline.clone(), store, hardware, false)
                .unwrap();
        (f, backend, state)
    }

    #[test]
    fn oppstart_uten_journal_eller_auto_capture_apner_aldri_maskinvare() {
        let (_f, _backend, state) = backend(&baseline());
        assert!(state.lock().unwrap().actions.is_empty());
        let f = Fixture::new();
        let store = PrivateStore::open_fixture(&f.path).unwrap();
        let (hardware, state) = hardware();
        assert!(
            NativeBackend::from_parts(authorization(), baseline(), store, hardware, false).is_err()
        );
        assert!(!f.path.join("baseline.json").exists());
        assert!(state.lock().unwrap().actions.is_empty());
    }

    #[test]
    fn alle_journalfaser_krever_avklaring_ved_restart_uten_setter() {
        for phase in [
            Phase::Prepared,
            Phase::Attempted,
            Phase::RejectedBeforeWrite,
        ] {
            let f = Fixture::new();
            let baseline = baseline();
            let store = f.store(&baseline);
            let mut record = fixtures::record(&baseline);
            record.phase = phase;
            store.write_journal(&baseline, None, &record).unwrap();
            let before = std::fs::read(f.path.join("journal.json")).unwrap();
            let (hardware, state) = hardware();
            let result =
                NativeBackend::from_parts(authorization(), baseline, store, hardware, false);
            assert!(matches!(result, Err(e) if e.starts_with("RecoveryRequired:")));
            assert!(state.lock().unwrap().actions.is_empty());
            assert_eq!(std::fs::read(f.path.join("journal.json")).unwrap(), before);
        }
    }

    #[test]
    fn tillatelse_og_hele_ferske_bindingen_ma_samsvare() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let mut other = binding();
        other.signature = Signature::Palit100;
        assert!(backend.read(&other).is_err());
        assert!(state.lock().unwrap().actions.is_empty());
        state.lock().unwrap().binding = other.clone();
        assert!(backend.read(&binding()).is_err());
        assert_eq!(state.lock().unwrap().actions, ["read"]);
        let f = Fixture::new();
        let store = f.store(&baseline);
        let auth = Authorization::new(1000, other, rgb_auth::MIN_COMPATIBLE_BUILD).unwrap();
        let (hardware, state) = hardware();
        assert!(NativeBackend::from_parts(auth, baseline, store, hardware, false).is_err());
        assert!(state.lock().unwrap().actions.is_empty());
    }

    #[test]
    fn apply_krever_varig_matchende_journal_og_bevarer_ukjent_modus() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let request = Request::Off {};
        assert!(
            !backend
                .apply(&binding(), &request, &observed(), &baseline)
                .unwrap_err()
                .write_attempted
        );
        assert!(state.lock().unwrap().actions.is_empty());
        backend.mark_dirty(&journal(&baseline)).unwrap();
        assert_eq!(
            backend.store.journal(&baseline).unwrap().unwrap().phase,
            Phase::Prepared
        );
        let mut changed = observed();
        changed.color = Some([1, 1, 1]);
        assert!(
            !backend
                .apply(&binding(), &request, &changed, &baseline)
                .unwrap_err()
                .write_attempted
        );
        assert!(state.lock().unwrap().actions.is_empty());
        let result = backend
            .apply(&binding(), &request, &observed(), &baseline)
            .unwrap();
        assert_eq!(result.mode, None);
        assert_eq!(state.lock().unwrap().actions, ["apply"]);
        assert_eq!(
            backend.store.journal(&baseline).unwrap().unwrap().phase,
            Phase::Attempted
        );
        assert!(backend.clear_dirty(&binding()).is_err());
        assert!(backend.store.journal(&baseline).unwrap().is_some());
    }

    #[test]
    fn avvisning_og_feilet_forste_setter_skilte_faser() {
        for attempted in [false, true] {
            let baseline = baseline();
            let (_f, mut backend, state) = backend(&baseline);
            backend.mark_dirty(&journal(&baseline)).unwrap();
            state.lock().unwrap().result = Err(WriteError {
                message: "simulert transportfeil".into(),
                write_attempted: attempted,
            });
            let error = backend
                .apply(&binding(), &Request::Off {}, &observed(), &baseline)
                .unwrap_err();
            assert_eq!(error.write_attempted, attempted);
            let record = backend.store.journal(&baseline).unwrap().unwrap();
            assert_eq!(
                record.phase,
                if attempted {
                    Phase::Attempted
                } else {
                    Phase::RejectedBeforeWrite
                }
            );
            assert_eq!(record.has_recorded_write_responsibility(), attempted);
            assert_eq!(backend.own_write_responsibility, attempted);
        }
    }

    #[test]
    fn ny_avvisning_beholder_tidligere_eget_skriveansvar() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        backend.mark_dirty(&journal(&baseline)).unwrap();
        let off = backend
            .apply(&binding(), &Request::Off {}, &observed(), &baseline)
            .unwrap();
        let mut next = journal(&baseline);
        next.txn = 2;
        next.before = Request::Off {};
        next.before_observed = off.clone();
        next.requested = journal(&baseline).before;
        backend.mark_dirty(&next).unwrap();
        state.lock().unwrap().result = Err(no_write("Ekstern drift før setter."));
        assert!(
            !backend
                .apply(&binding(), &next.requested, &off, &baseline)
                .unwrap_err()
                .write_attempted
        );
        let record = backend.store.journal(&baseline).unwrap().unwrap();
        assert_eq!(record.phase, Phase::RejectedBeforeWrite);
        assert!(record.prior_write_responsibility);
        assert!(record.has_recorded_write_responsibility());
    }

    #[test]
    fn io_feil_etter_setter_bevarer_prepared_og_rapporterer_mulig_endring() {
        let baseline = baseline();
        let (f, mut backend, state) = backend(&baseline);
        backend.mark_dirty(&journal(&baseline)).unwrap();
        let prepared = std::fs::read(f.path.join("journal.json")).unwrap();
        state.lock().unwrap().fail_persistence_after_write =
            Some(backend.store.fail_before_rename.clone());
        let error = backend
            .apply(&binding(), &Request::Off {}, &observed(), &baseline)
            .unwrap_err();
        assert!(error.write_attempted);
        assert!(backend.own_write_responsibility);
        assert_eq!(
            std::fs::read(f.path.join("journal.json")).unwrap(),
            prepared
        );
        assert_eq!(
            backend.store.journal(&baseline).unwrap().unwrap().phase,
            Phase::Prepared
        );
        // En feil i etterjournalen låser ikke recover permanent. Den må få
        // journalført sitt nye mål før den egne, avgrensede recovery-stien.
        state.lock().unwrap().fail_persistence_after_write = None;
        backend
            .store
            .fail_before_rename
            .store(false, Ordering::Relaxed);
        state.lock().unwrap().result = Ok(observed());
        backend
            .recover(&binding(), &journal(&baseline).before, &baseline)
            .unwrap();
        let record = backend.store.journal(&baseline).unwrap().unwrap();
        assert_eq!(record.operation, Operation::Recover);
        assert!(record.prior_write_responsibility);
        assert_eq!(state.lock().unwrap().actions, ["apply", "recover"]);
    }

    #[test]
    fn journalfeil_for_setter_gir_ingen_maskinvarekall() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        backend
            .store
            .fail_before_rename
            .store(true, Ordering::Relaxed);
        assert!(backend.mark_dirty(&journal(&baseline)).is_err());
        assert!(backend.record.is_none());
        assert!(
            !backend
                .apply(&binding(), &Request::Off {}, &observed(), &baseline)
                .unwrap_err()
                .write_attempted
        );
        assert!(state.lock().unwrap().actions.is_empty());
    }

    #[test]
    fn mappefsync_feil_etter_rename_beholder_ny_journal_uten_suksess() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        backend.mark_dirty(&journal(&baseline)).unwrap();
        backend
            .store
            .fail_directory_sync
            .store(true, Ordering::Relaxed);
        let error = backend
            .apply(&binding(), &Request::Off {}, &observed(), &baseline)
            .unwrap_err();
        assert!(error.write_attempted);
        let disk = backend.store.journal(&baseline).unwrap().unwrap();
        assert_eq!(disk.phase, Phase::Attempted);
        assert_eq!(backend.record.as_ref(), Some(&disk));
        assert_eq!(state.lock().unwrap().actions, ["apply"]);
    }

    #[test]
    fn transportfeil_teller_forsok_ikke_vellykkede_kall() {
        let f = Fixture::new();
        // Kun metadata i en uttrykkelig fixture; aldri identify/apply/recover
        // mot verten, heller ikke når testkjøreren tilfeldigvis er root.
        let cause = palit_rgb::metadata_preview(&f.path, "ugyldig", "ugyldig").unwrap_err();
        for attempted in [0, 1, 5] {
            let error = palit_rgb::ApplyFailure {
                cause: cause.clone(),
                writes_attempted: attempted,
                writes_completed: 0,
                observed_after: None,
            };
            assert_eq!(
                hardware_result(Err(error), &binding())
                    .unwrap_err()
                    .write_attempted,
                attempted != 0
            );
        }
    }

    #[test]
    fn recover_krever_journal_target_og_sletter_aldri_ved_ukjent_modus() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let initial = journal(&baseline).before;
        assert!(
            !backend
                .recover(&binding(), &initial, &baseline)
                .unwrap_err()
                .write_attempted
        );
        backend.mark_dirty(&journal(&baseline)).unwrap();
        let unknown_target = Request::Static {
            color: [1, 1, 1],
            brightness: 50u8.try_into().unwrap(),
        };
        assert!(
            !backend
                .recover(&binding(), &unknown_target, &baseline)
                .unwrap_err()
                .write_attempted
        );
        assert!(state.lock().unwrap().actions.is_empty());
        state.lock().unwrap().result = Ok(observed());
        backend.recover(&binding(), &initial, &baseline).unwrap();
        assert_eq!(state.lock().unwrap().actions, ["recover"]);
        assert!(backend.clear_dirty(&binding()).is_err());
        assert!(backend.store.journal(&baseline).unwrap().is_some());
    }

    #[test]
    fn bare_full_verifisering_kan_rydde_journalen_i_kontrakttest() {
        // Fysisk Palit rapporterer aldri modus. Denne falske transporten
        // prøver bare clear_dirty-kontrakten for en faktisk kjent modus.
        let known = Observed {
            mode: Some(Mode::Static),
            ..observed()
        };
        let baseline = Baseline::from_observed(binding(), known.clone()).unwrap();
        let (_f, mut backend, state) = backend(&baseline);
        let journal = journal(&baseline);
        backend.mark_dirty(&journal).unwrap();
        state.lock().unwrap().result = Ok(known);
        backend
            .recover(&binding(), &journal.before, &baseline)
            .unwrap();
        let mut wrong = binding();
        wrong.uuid = "GPU-other-fixture".into();
        assert!(backend.clear_dirty(&wrong).is_err());
        assert!(backend.store.journal(&baseline).unwrap().is_some());
        backend.clear_dirty(&binding()).unwrap();
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert_eq!(backend.store.baseline().unwrap(), baseline);
    }
}
