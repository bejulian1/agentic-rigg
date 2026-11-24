//! Privilegert RGB-adapter. Oppstart leser bare private filer og tar en lås;
//! ingen grunnverdier fanges automatisk og ingen journal gjenopprettes da.

use crate::rgb_store::{
    EvidenceContext, Operation, Phase, PrivateStore, StoredJournal, VisualEvidence, fields_match,
    load_authorization, load_recovery_authorization, new_recovery_id,
};
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
    last_read: Option<Observed>,
    // Kun siste vellykkede egne apply/frame, aldri en vilkårlig read. Dette
    // live-beviset kan ikke rekonstrueres fra en journal etter restart.
    last_apply: Option<Observed>,
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
            last_read: None,
            last_apply: None,
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
            let current = load_recovery_authorization()?;
            if current != self.authorization {
                return Err("RGB-tillatelsen er endret eller tilbakekalt.".into());
            }
        }
        Ok(())
    }

    /// Bare eksplisitt root-CLI. Ingen startup-recovery: vanlig konstruktør
    /// avviser fremdeles enhver journal før maskinvareåpning.
    fn open_resolution(uid: u32, pci: &str, uuid: &str, confirmed: bool) -> Result<Self, String> {
        // SAFETY: ren identitetslesing. Ingen fixture- eller brukerfallback.
        if !confirmed || unsafe { libc::geteuid() } != 0 {
            return Err("RGB-restartavklaring krever eksplisitt administratorbekreftelse.".into());
        }
        let authorization = load_recovery_authorization()?;
        if uid == 0
            || uid != authorization.uid()
            || pci != authorization.binding().pci
            || uuid != authorization.binding().uuid
        {
            return Err(
                "RGB-avklaringen gjelder ikke den autoriserte brukeren og det valgte kortet."
                    .into(),
            );
        }
        Self::from_existing(
            authorization,
            PrivateStore::open_production()?,
            Box::new(Physical),
            true,
        )
    }

    fn from_existing(
        authorization: Authorization,
        store: PrivateStore,
        hardware: Box<dyn Hardware>,
        production: bool,
    ) -> Result<Self, String> {
        let baseline = store.baseline()?;
        if baseline.source() != BaselineSource::UserConfirmedStatic
            || baseline.binding() != authorization.binding()
        {
            return Err(
                "RGB-restartavklaring mangler samme brukerbekreftede original og kortbinding."
                    .into(),
            );
        }
        Book::new(baseline.clone()).map_err(|e| e.to_string())?;
        let record = store
            .journal(&baseline)?
            .ok_or("Det finnes ingen RGB-journal å avklare.")?;
        let own_write_responsibility = record.has_recorded_write_responsibility();
        Ok(Self {
            authorization,
            baseline,
            store,
            record: Some(record),
            own_write_responsibility,
            last_recovery: None,
            last_read: None,
            last_apply: None,
            hardware,
            production,
        })
    }

    /// Ingen setter. Brukeren bekrefter at ORIGINALEN allerede vises, og én
    /// ny identifikasjon må gi samme binding og komplette originale felt.
    pub fn resolve_production(
        uid: u32,
        pci: &str,
        uuid: &str,
        recovery_id: &str,
    ) -> Result<VisualEvidence, String> {
        Self::open_resolution(uid, pci, uuid, true)?.resolve_for(recovery_id)
    }

    pub fn inspect_production(
        uid: u32,
        pci: &str,
        uuid: &str,
    ) -> Result<(Request, String), String> {
        Self::open_resolution(uid, pci, uuid, true)?.inspect_original()
    }

    fn inspect_original(&mut self) -> Result<(Request, String), String> {
        let mut record = self.record.clone().ok_or("RGB-journalen mangler.")?;
        let id = match record.recovery_id.clone() {
            Some(id) => id,
            None => {
                // Journal fra protokoll 1. Ingen feltlesing eller setter.
                let id = new_recovery_id()?;
                record.recovery_id = Some(id.clone());
                self.persist(record)?;
                id
            }
        };
        let target = Book::new(self.baseline.clone())
            .map_err(|error| error.to_string())?
            .snapshot()
            .confirmed;
        Ok((target, id))
    }

    fn resolve_for(&mut self, recovery_id: &str) -> Result<VisualEvidence, String> {
        if self
            .record
            .as_ref()
            .and_then(|record| record.recovery_id.as_deref())
            != Some(recovery_id)
        {
            return Err("RGB-bekreftelsen gjelder et tidligere forsøk. Hent utgangspunktet og se på kortet på nytt.".into());
        }
        self.resolve_original()
    }

    fn resolve_original(&mut self) -> Result<VisualEvidence, String> {
        let target = Book::new(self.baseline.clone())
            .map_err(|e| e.to_string())?
            .snapshot()
            .confirmed;
        let binding = self.baseline.binding().clone();
        let observed = self.read(&binding)?;
        if !fields_match(&target, &observed) {
            return Err(
                "Ferske RGB-felt samsvarer ikke med originalen; journalen er beholdt.".into(),
            );
        }
        let record = self.record.as_ref().ok_or("RGB-journalen mangler.")?;
        let evidence = VisualEvidence::new(
            self.authorization.uid(),
            &self.baseline,
            record,
            &target,
            &observed,
            EvidenceContext::RestartBaseline,
        )?;
        self.store
            .acknowledge_visual(&self.baseline, record, &evidence)?;
        self.record = None;
        self.own_write_responsibility = false;
        Ok(evidence)
    }

    /// Eksplisitt root-CLI for den lagrede ORIGINALEN, også når aktuelle
    /// felt ikke kan leses. Én journalført recover; aldri automatisk oppstart,
    /// journalrydding eller påstand om verifisert modus etter denne skrivingen.
    pub fn restore_production(
        uid: u32,
        pci: &str,
        uuid: &str,
        restore_original: bool,
    ) -> Result<Observed, String> {
        Self::open_resolution(uid, pci, uuid, restore_original)?.restore_original()
    }

    fn restore_original(&mut self) -> Result<Observed, String> {
        let baseline = self.baseline.clone();
        let target = Book::new(baseline.clone())
            .map_err(|e| e.to_string())?
            .snapshot()
            .confirmed;
        let observed = self
            .recover(baseline.binding(), &target, &baseline)
            .map_err(|e| {
                format!(
                    "RGB-originalen er ikke avklart: {}; setterforsøk={}",
                    e.message, e.write_attempted
                )
            })?;
        if !fields_match(&target, &observed) {
            return Err("Etter gjenopprettingsforsøket samsvarer ikke komplette RGB-felt med originalen; journalen er beholdt.".into());
        }
        Ok(observed)
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
        if !recovery
            && let Ok(observed) = &result
            && self
                .record
                .as_ref()
                .is_some_and(|record| fields_match(&record.target, observed))
        {
            self.last_apply = Some(observed.clone());
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
        self.last_read = None;
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
        self.last_read = Some(report.observed.clone());
        Ok(report.observed)
    }

    fn mark_dirty(&mut self, journal: &Journal) -> Result<(), String> {
        self.current_authorization()?;
        journal.validate(self.authorization.binding())?;
        if journal.baseline != self.baseline {
            return Err("RGB-journalen kan ikke erstatte det opprinnelige grunnopptaket.".into());
        }
        self.store.visual_evidence(&self.baseline)?;
        let previous = self.own_write_responsibility
            || self
                .record
                .as_ref()
                .is_some_and(StoredJournal::has_recorded_write_responsibility);
        self.last_recovery = None;
        self.last_read = None;
        self.last_apply = None;
        self.persist(StoredJournal {
            version: 1,
            journal: journal.clone(),
            operation: Operation::Apply,
            target: journal.requested.clone(),
            phase: Phase::Prepared,
            prior_write_responsibility: previous,
            recovery_id: Some(new_recovery_id()?),
        })
    }

    fn journal_present(&self) -> Option<bool> {
        Some(self.record.is_some() || !matches!(self.store.journal(&self.baseline), Ok(None)))
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
        self.last_read = None;
        self.last_apply = None;
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

    fn animation_frame(
        &mut self,
        binding: &Binding,
        request: &Request,
        expected_before: &Observed,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        self.last_recovery = None;
        self.last_read = None;
        let result = (|| {
            self.binding(binding, baseline).map_err(no_write)?;
            let record = self
                .record
                .as_ref()
                .ok_or_else(|| no_write("Animasjonen mangler en varig RGB-journal."))?;
            if record.phase != Phase::Attempted
                || record.operation != Operation::Apply
                || !self.own_write_responsibility
                || !matches!(record.target, Request::Static { .. })
                || !matches!(request, Request::Static { .. })
                || self.last_apply.as_ref() != Some(expected_before)
            {
                return Err(no_write(
                    "Animasjonen mangler samme journalførte startvalg og siste egne bilde.",
                ));
            }
            self.current_authorization().map_err(no_write)?;
            self.store
                .validate_state(&self.baseline, self.record.as_ref())
                .map_err(no_write)?;
            // Samme avgrensede statiske transport kontrollerer fersk binding,
            // signatur og expected_before på samme fd før første setter.
            // Book har allerede krevd Behold og samme eier. Journalen for
            // første setter er varig og beholdes uendret gjennom hele effekten;
            // et krasj kan aldri gjenoppta effekten eller rydde journalen.
            self.hardware
                .apply(binding, request, expected_before, baseline)
        })();
        // Ingen persist/finish_write her: allerede varig Attempted og eget
        // ansvar dekker også en mulig delvis frame. En feil bryter kjeden;
        // eksplisitt ny transaksjon/recovery kreves før flere bilder.
        self.last_apply = result
            .as_ref()
            .ok()
            .filter(|observed| fields_match(request, observed))
            .cloned();
        result
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
        next.recovery_id = Some(new_recovery_id().map_err(no_write)?);
        next.validate(baseline).map_err(no_write)?;
        self.last_recovery = None;
        self.last_read = None;
        self.last_apply = None;
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
        self.last_apply = None;
        Ok(())
    }

    fn adopt_baseline(&mut self, binding: &Binding, next: &Baseline) -> Result<(), String> {
        self.binding(binding, &self.baseline.clone())?;
        self.current_authorization()?;
        // Bare nøyaktig feltene hjelperen nettopp leste fra kortet.
        if self.last_read.as_ref() != Some(next.observed()) {
            return Err("Det varige RGB-valget mangler en fersk, samsvarende avlesning.".into());
        }
        self.store.adopt_baseline(&self.baseline, self.record.as_ref(), next)?;
        self.baseline = next.clone();
        self.record = None;
        self.own_write_responsibility = false;
        self.last_recovery = None;
        self.last_apply = None;
        Ok(())
    }

    fn settle_after_animation(&mut self, binding: &Binding) -> Result<(), String> {
        self.binding(binding, &self.baseline.clone())?;
        let restored = self.last_recovery.as_ref().is_some_and(|observed| {
            matches!(
                self.baseline.recovery_readback(binding, observed),
                RecoveryReadback::ReadableFieldsMatch | RecoveryReadback::Verified
            )
        });
        if !restored {
            return Err("RGB-journalen beholdes: utgangspunktet ble ikke lest tilbake likt.".into());
        }
        let record = self.record.as_ref().ok_or("RGB-journalen mangler.")?;
        self.store.clear_journal(binding, &self.baseline, record)?;
        self.record = None;
        self.own_write_responsibility = false;
        self.last_recovery = None;
        self.last_apply = None;
        Ok(())
    }

    fn factory_baseline(&self) -> Option<Baseline> {
        self.store.factory(self.baseline.binding()).ok().flatten()
    }

    fn ack_recovery(
        &mut self,
        binding: &Binding,
        target: &Request,
        observed: &Observed,
        baseline: &Baseline,
    ) -> Result<(), String> {
        self.binding(binding, baseline)?;
        self.current_authorization()?;
        if self.last_read.as_ref() != Some(observed)
            || !self
                .last_recovery
                .as_ref()
                .is_some_and(|value| fields_match(target, value))
        {
            return Err(
                "RGB-bekreftelsen mangler fersk lesing av samme gjenopprettede mål.".into(),
            );
        }
        let record = self.record.as_ref().ok_or("RGB-journalen mangler.")?;
        let evidence = VisualEvidence::new(
            self.authorization.uid(),
            baseline,
            record,
            target,
            observed,
            EvidenceContext::LiveRecovery,
        )?;
        let result = self.store.acknowledge_visual(baseline, record, &evidence);
        let original = Book::new(baseline.clone())
            .map_err(|e| e.to_string())?
            .snapshot()
            .confirmed;
        if target == &original
            && (result.is_ok()
                || (matches!(self.store.journal(baseline), Ok(None))
                    && self
                        .store
                        .visual_evidence(baseline)
                        .as_ref()
                        .is_ok_and(|current| current.as_ref() == Some(&evidence))))
        {
            self.record = None;
            self.own_write_responsibility = false;
            self.last_recovery = None;
            self.last_read = None;
            self.last_apply = None;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgb_store::fixtures::{
        self, Fixture, authorization, baseline, binding, journal, observed, record,
    };
    use rigg_core::rgb::{Mode, Signature};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    struct FakeState {
        actions: Vec<&'static str>,
        binding: Binding,
        result: Result<Observed, WriteError>,
        read_observed: Observed,
        expected_before: Vec<Observed>,
        fail_persistence_after_write: Option<Arc<AtomicBool>>,
    }

    struct Fake(Arc<Mutex<FakeState>>);

    impl Hardware for Fake {
        fn read(&mut self, _: &Binding) -> Result<Discovery, String> {
            let mut state = self.0.lock().unwrap();
            state.actions.push("read");
            Discovery::identified(state.binding.clone(), state.read_observed.clone())
        }

        fn apply(
            &mut self,
            binding: &Binding,
            _: &Request,
            before: &Observed,
            _: &Baseline,
        ) -> Result<Observed, WriteError> {
            {
                let mut state = self.0.lock().unwrap();
                state.expected_before.push(before.clone());
                if &state.binding != binding || &state.read_observed != before {
                    return Err(no_write("Simulert binding-/feltendring før setter."));
                }
            }
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
            if let Ok(value) = &state.result {
                state.read_observed = value.clone();
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
            read_observed: observed(),
            expected_before: vec![],
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

    fn static_request(color: [u8; 3]) -> Request {
        Request::Static {
            color,
            brightness: 80u8.try_into().unwrap(),
        }
    }

    fn frame_observed(color: [u8; 3]) -> Observed {
        Observed {
            color: Some(color),
            brightness_raw: Some(80),
            mode: None,
        }
    }

    fn start_static(
        backend: &mut NativeBackend,
        state: &Arc<Mutex<FakeState>>,
        baseline: &Baseline,
    ) -> Observed {
        let mut initial = journal(baseline);
        initial.requested = static_request([8, 9, 10]);
        backend.mark_dirty(&initial).unwrap();
        state.lock().unwrap().result = Ok(frame_observed([8, 9, 10]));
        backend
            .apply(&binding(), &initial.requested, &observed(), baseline)
            .unwrap()
    }

    #[test]
    fn bilder_gjenbruker_varig_journal_uten_fsync_og_sammenligner_siste_egne_felt() {
        let baseline = baseline();
        let (f, mut backend, state) = backend(&baseline);
        let mut before = start_static(&mut backend, &state, &baseline);
        let persisted = std::fs::read(f.path.join("journal.json")).unwrap();
        // Dersom en frame forsøker ny journalføring, skal denne kontrollerte
        // feilen slå ut. Vanlig validate_state leser bare den private fixturen.
        backend
            .store
            .fail_before_rename
            .store(true, Ordering::Relaxed);
        backend
            .store
            .fail_directory_sync
            .store(true, Ordering::Relaxed);
        for color in [[18, 19, 20], [28, 29, 30]] {
            state.lock().unwrap().result = Ok(frame_observed(color));
            before = backend
                .animation_frame(&binding(), &static_request(color), &before, &baseline)
                .unwrap();
            assert_eq!(before, frame_observed(color));
            assert_eq!(before.mode, None);
            assert_eq!(
                std::fs::read(f.path.join("journal.json")).unwrap(),
                persisted
            );
        }
        assert_eq!(state.lock().unwrap().actions, ["apply", "apply", "apply"]);
        assert_eq!(
            state.lock().unwrap().expected_before,
            [
                observed(),
                frame_observed([8, 9, 10]),
                frame_observed([18, 19, 20])
            ]
        );
        let record = backend.store.journal(&baseline).unwrap().unwrap();
        assert_eq!(record.phase, Phase::Attempted);
        assert_eq!(record.target, static_request([8, 9, 10]));
        assert!(backend.own_write_responsibility);
        assert!(backend.clear_dirty(&binding()).is_err());
    }

    #[test]
    fn baselinefarget_effekt_beholder_journal_frem_til_fersk_visuell_avklaring() {
        let baseline = baseline();
        let (f, mut backend, state) = backend(&baseline);
        let mut book = Book::new(baseline.clone()).unwrap();
        let initial = book.snapshot().confirmed;
        state.lock().unwrap().result = Ok(observed());
        let pending = book
            .apply_for_effect(&mut backend, 7, initial.clone(), 0, 10_000)
            .unwrap();
        book.keep(&mut backend, 7, pending.txn.unwrap(), 1).unwrap();
        assert!(book.snapshot().dirty && book.snapshot().may_have_written);
        assert!(backend.store.journal(&baseline).unwrap().is_some());
        assert!(!f.path.join("visual-confirmation.json").exists());
        state.lock().unwrap().result = Ok(frame_observed([18, 19, 20]));
        book.animation_frame(&mut backend, 7, static_request([18, 19, 20]))
            .unwrap();
        assert_eq!(book.snapshot().confirmed, initial);
        // Fra 1.7.0: frigivelse etter en effekt skriver utgangspunktet med den
        // faste sekvensen; leses alle felt likt, ryddes journalen uten en ny
        // visuell kvittering (ingen uavklart økt hver gang appen lukkes).
        state.lock().unwrap().result = Ok(observed());
        book.release(&mut backend).unwrap();
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert!(!f.path.join("visual-confirmation.json").exists());
        assert!(!book.snapshot().dirty && !book.snapshot().may_have_written);
        assert_eq!(backend.store.baseline().unwrap(), baseline);
    }

    #[test]
    fn effekt_som_stoppes_med_avvikende_felt_beholder_journalen() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let mut book = Book::new(baseline.clone()).unwrap();
        let initial = book.snapshot().confirmed;
        state.lock().unwrap().result = Ok(observed());
        let pending = book.apply_for_effect(&mut backend, 7, initial, 0, 10_000).unwrap();
        book.keep(&mut backend, 7, pending.txn.unwrap(), 1).unwrap();
        state.lock().unwrap().result = Ok(frame_observed([18, 19, 20]));
        book.animation_frame(&mut backend, 7, static_request([18, 19, 20])).unwrap();
        // Gjenopprettingen leses ikke likt: journalen og bekreftelseskravet står.
        state.lock().unwrap().result = Ok(frame_observed([1, 1, 1]));
        book.release(&mut backend).unwrap();
        assert!(backend.store.journal(&baseline).unwrap().is_some());
        assert!(book.snapshot().dirty);
    }

    #[test]
    fn varig_valg_sikrer_fabrikklyset_og_bytter_utgangspunktet_atomisk() {
        let baseline = baseline();
        let (f, mut backend, state) = backend(&baseline);
        let mut book = Book::new(baseline.clone()).unwrap();
        let blue = static_request([0, 0, 255]);
        state.lock().unwrap().result = Ok(frame_observed([0, 0, 255]));
        let pending = book.apply(&mut backend, 7, blue.clone(), 0, 10_000).unwrap();
        book.keep(&mut backend, 7, pending.txn.unwrap(), 1).unwrap();
        assert!(backend.store.journal(&baseline).unwrap().is_some());
        book.persist(&mut backend, 7, 2).unwrap();
        // Det nye utgangspunktet er lagret, journalen ryddet og fabrikklyset sikret.
        let current = backend.store.baseline().unwrap();
        assert_eq!(Book::new(current.clone()).unwrap().baseline_request(), &blue);
        assert!(backend.store.journal(&current).unwrap().is_none());
        assert_eq!(backend.factory_baseline(), Some(baseline.clone()));
        assert!(!f.path.join("baseline-neste.json").exists());
        // Et nytt varig valg beholder det opprinnelige fabrikklyset.
        let red = static_request([255, 0, 0]);
        state.lock().unwrap().result = Ok(frame_observed([255, 0, 0]));
        let pending = book.apply(&mut backend, 7, red.clone(), 3, 10_000).unwrap();
        book.keep(&mut backend, 7, pending.txn.unwrap(), 4).unwrap();
        book.persist(&mut backend, 7, 5).unwrap();
        assert_eq!(backend.factory_baseline(), Some(baseline));
        assert_eq!(Book::new(backend.store.baseline().unwrap()).unwrap().baseline_request(), &red);
    }

    #[test]
    fn varig_valg_avvises_uten_fersk_samsvarende_lesing() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let mut book = Book::new(baseline.clone()).unwrap();
        state.lock().unwrap().result = Ok(frame_observed([0, 0, 255]));
        let pending = book.apply(&mut backend, 7, static_request([0, 0, 255]), 0, 10_000).unwrap();
        book.keep(&mut backend, 7, pending.txn.unwrap(), 1).unwrap();
        let next = Baseline::user_confirmed_static(binding(), frame_observed([9, 9, 9])).unwrap();
        assert!(backend.adopt_baseline(&binding(), &next).is_err());
        assert_eq!(backend.store.baseline().unwrap(), baseline);
        assert!(backend.store.journal(&baseline).unwrap().is_some());
    }

    #[test]
    fn bilde_krever_live_vellykket_setter_ikke_bare_en_lagret_journal() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let request = static_request([18, 19, 20]);
        assert!(
            !backend
                .animation_frame(&binding(), &request, &observed(), &baseline)
                .unwrap_err()
                .write_attempted
        );
        let mut initial = journal(&baseline);
        initial.requested = static_request([8, 9, 10]);
        backend.mark_dirty(&initial).unwrap();
        assert!(
            !backend
                .animation_frame(&binding(), &request, &observed(), &baseline)
                .unwrap_err()
                .write_attempted
        );
        assert!(state.lock().unwrap().actions.is_empty());

        let f = Fixture::new();
        let store = f.store(&baseline);
        let mut record = fixtures::record(&baseline);
        record.phase = Phase::Attempted;
        record.journal.requested = initial.requested.clone();
        record.target = initial.requested;
        store.write_journal(&baseline, None, &record).unwrap();
        let (hardware, state) = hardware();
        let mut restarted =
            NativeBackend::from_existing(authorization(), store, hardware, false).unwrap();
        assert!(
            !restarted
                .animation_frame(&binding(), &request, &observed(), &baseline)
                .unwrap_err()
                .write_attempted
        );
        assert!(state.lock().unwrap().actions.is_empty());
        assert!(restarted.store.journal(&baseline).unwrap().is_some());
    }

    #[test]
    fn bilde_avviser_endret_journal_las_binding_eller_egne_forfelt_for_setter() {
        for case in [
            "journal",
            "lock",
            "binding",
            "before",
            "request",
            "external",
            "signature",
        ] {
            let baseline = baseline();
            let (f, mut backend, state) = backend(&baseline);
            let mut before = start_static(&mut backend, &state, &baseline);
            let mut selected = binding();
            let mut request = static_request([18, 19, 20]);
            match case {
                "journal" => {
                    let mut replaced = backend.record.clone().unwrap();
                    replaced.journal.txn += 1;
                    std::fs::write(
                        f.path.join("journal.json"),
                        serde_json::to_vec(&replaced).unwrap(),
                    )
                    .unwrap();
                }
                "lock" => {
                    std::fs::rename(f.path.join("worker.lock"), f.path.join("old.lock")).unwrap();
                    f.write("worker.lock", b"");
                }
                "binding" => selected.signature = Signature::Palit100,
                "before" => before.color = Some([98, 97, 96]),
                "request" => request = Request::Off {},
                "external" => state.lock().unwrap().read_observed.color = Some([98, 97, 96]),
                "signature" => state.lock().unwrap().binding.signature = Signature::Palit100,
                _ => unreachable!(),
            }
            let persisted = std::fs::read(f.path.join("journal.json")).unwrap();
            let error = backend
                .animation_frame(&selected, &request, &before, &baseline)
                .unwrap_err();
            assert!(!error.write_attempted, "{case}");
            assert_eq!(state.lock().unwrap().actions, ["apply"], "{case}");
            assert_eq!(
                std::fs::read(f.path.join("journal.json")).unwrap(),
                persisted
            );
            assert!(backend.own_write_responsibility);
            assert!(backend.last_apply.is_none());
        }
    }

    #[test]
    fn framefeil_beholder_skriveansvar_og_stanser_kjeden_uten_journalendring() {
        for attempted in [false, true] {
            let baseline = baseline();
            let (f, mut backend, state) = backend(&baseline);
            let before = start_static(&mut backend, &state, &baseline);
            let persisted = std::fs::read(f.path.join("journal.json")).unwrap();
            state.lock().unwrap().result = Err(WriteError {
                message: "simulert framefeil".into(),
                write_attempted: attempted,
            });
            assert_eq!(
                backend
                    .animation_frame(
                        &binding(),
                        &static_request([18, 19, 20]),
                        &before,
                        &baseline
                    )
                    .unwrap_err()
                    .write_attempted,
                attempted
            );
            state.lock().unwrap().result = Ok(frame_observed([18, 19, 20]));
            assert!(
                !backend
                    .animation_frame(
                        &binding(),
                        &static_request([18, 19, 20]),
                        &before,
                        &baseline
                    )
                    .unwrap_err()
                    .write_attempted
            );
            assert_eq!(state.lock().unwrap().actions, ["apply", "apply"]);
            assert!(backend.own_write_responsibility);
            assert_eq!(
                std::fs::read(f.path.join("journal.json")).unwrap(),
                persisted
            );
            // Book bestemmer om recovery er automatisk eller bare eksplisitt.
            // Backend må fortsatt kunne gjenopprette originalen etter feilen.
            state.lock().unwrap().result = Ok(observed());
            backend
                .recover(&binding(), &journal(&baseline).before, &baseline)
                .unwrap();
            assert_eq!(state.lock().unwrap().actions, ["apply", "apply", "recover"]);
            assert!(backend.store.journal(&baseline).unwrap().is_some());
        }
    }

    #[test]
    fn mismatchet_frame_kan_ikke_brukes_som_nytt_forbilde() {
        let baseline = baseline();
        let (_f, mut backend, state) = backend(&baseline);
        let before = start_static(&mut backend, &state, &baseline);
        state.lock().unwrap().result = Ok(frame_observed([98, 97, 96]));
        let mismatch = backend
            .animation_frame(
                &binding(),
                &static_request([18, 19, 20]),
                &before,
                &baseline,
            )
            .unwrap();
        assert!(backend.last_apply.is_none());
        assert!(
            !backend
                .animation_frame(
                    &binding(),
                    &static_request([28, 29, 30]),
                    &mismatch,
                    &baseline
                )
                .unwrap_err()
                .write_attempted
        );
        assert_eq!(state.lock().unwrap().actions, ["apply", "apply"]);
        assert!(backend.own_write_responsibility);
    }

    #[test]
    fn live_visuell_kvittering_krever_fersk_lesing_og_bevarer_ukjent_modus() {
        let baseline = baseline();
        let (f, mut backend, state) = backend(&baseline);
        backend.mark_dirty(&journal(&baseline)).unwrap();
        state.lock().unwrap().result = Ok(observed());
        let initial = journal(&baseline).before;
        backend.recover(&binding(), &initial, &baseline).unwrap();
        assert!(
            backend
                .ack_recovery(&binding(), &initial, &observed(), &baseline)
                .is_err()
        );
        let actual = backend.read(&binding()).unwrap();
        backend
            .ack_recovery(&binding(), &initial, &actual, &baseline)
            .unwrap();
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert_eq!(actual.mode, None);
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(f.path.join("visual-confirmation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["source"], "userVisual");
        assert_eq!(receipt["context"], "liveRecovery");
        assert!(receipt["observed"]["mode"].is_null());
        assert_eq!(state.lock().unwrap().actions, ["recover", "read"]);
    }

    #[test]
    fn endret_binding_eller_felt_gir_ingen_visuell_rydding() {
        for changed_binding in [false, true] {
            let baseline = baseline();
            let (_f, mut backend, state) = backend(&baseline);
            backend.mark_dirty(&journal(&baseline)).unwrap();
            state.lock().unwrap().result = Ok(observed());
            let initial = journal(&baseline).before;
            backend.recover(&binding(), &initial, &baseline).unwrap();
            backend.read(&binding()).unwrap();
            if changed_binding {
                state.lock().unwrap().binding.signature = Signature::Palit100;
                assert!(backend.read(&binding()).is_err());
                assert!(
                    backend
                        .ack_recovery(&binding(), &initial, &observed(), &baseline)
                        .is_err()
                );
            } else {
                state.lock().unwrap().read_observed.color = Some([1, 1, 1]);
                let changed = backend.read(&binding()).unwrap();
                assert!(
                    backend
                        .ack_recovery(&binding(), &initial, &changed, &baseline)
                        .is_err()
                );
            }
            assert!(backend.store.journal(&baseline).unwrap().is_some());
        }
    }

    #[test]
    fn restartavklaring_er_bare_lesing_og_alltid_originalen() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        store
            .write_journal(&baseline, None, &fixtures::record(&baseline))
            .unwrap();
        let (hardware, state) = hardware();
        let mut backend =
            NativeBackend::from_existing(authorization(), store, hardware, false).unwrap();
        let evidence = backend.resolve_original().unwrap();
        assert_eq!(state.lock().unwrap().actions, ["read"]);
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert_eq!(backend.store.baseline().unwrap(), baseline);
        let receipt = serde_json::to_value(evidence).unwrap();
        assert_eq!(receipt["source"], "userVisual");
        assert_eq!(receipt["context"], "restartBaseline");
        assert_eq!(receipt["originalBaseline"], true);
    }

    #[test]
    fn eksplisitt_originalrestore_etter_restart_skiller_skriving_fra_avklaring() {
        for phase in [
            Phase::Prepared,
            Phase::RejectedBeforeWrite,
            Phase::Attempted,
        ] {
            let f = Fixture::new();
            let baseline = baseline();
            let store = f.store(&baseline);
            let mut record = fixtures::record(&baseline);
            record.phase = phase;
            store.write_journal(&baseline, None, &record).unwrap();
            let (hardware, state) = hardware();
            state.lock().unwrap().read_observed.color = Some([99, 98, 97]);
            let mut backend =
                NativeBackend::from_existing(authorization(), store, hardware, false).unwrap();
            assert!(backend.resolve_original().is_err());
            assert_eq!(state.lock().unwrap().actions, ["read"]);
            assert!(backend.store.journal(&baseline).unwrap().is_some());
            state.lock().unwrap().result = Ok(observed());
            let result = backend.restore_original().unwrap();
            assert_eq!(result.mode, None);
            assert_eq!(state.lock().unwrap().actions, ["read", "recover"]);
            let persisted = backend.store.journal(&baseline).unwrap().unwrap();
            assert_eq!(persisted.operation, Operation::Recover);
            assert_eq!(persisted.phase, Phase::Attempted);
            assert_eq!(persisted.target, journal(&baseline).before);
            assert!(!f.path.join("visual-confirmation.json").exists());
            backend.resolve_original().unwrap();
            assert!(backend.store.journal(&baseline).unwrap().is_none());
            assert_eq!(state.lock().unwrap().actions, ["read", "recover", "read"]);
        }
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
    fn forste_journal_synlig_etter_mappefsync_feil_sperrer_automatisk_setter() {
        let baseline = baseline();
        let (_fixture, mut backend, hardware) = backend(&baseline);
        let mut book = Book::new(baseline.clone()).unwrap();
        backend
            .store
            .fail_directory_sync
            .store(true, Ordering::Relaxed);
        book.apply(&mut backend, 7, Request::Off {}, 10, 10_000)
            .unwrap_err();
        let disk = backend.store.journal(&baseline).unwrap().unwrap();
        assert_eq!(disk.phase, Phase::Prepared);
        assert_eq!(backend.record.as_ref(), Some(&disk));
        let view = book.snapshot();
        assert!(view.dirty);
        assert!(view.error.is_some());
        assert!(!view.may_have_written);
        assert!(view.pending.is_none());
        assert!(book.tick(&mut backend, 100_000).unwrap().is_none());
        assert!(book.release(&mut backend).is_none());
        assert_eq!(hardware.lock().unwrap().actions, ["read"]);

        backend
            .store
            .fail_directory_sync
            .store(false, Ordering::Relaxed);
        hardware.lock().unwrap().result = Ok(observed());
        assert!(book.restore_baseline(&mut backend).is_some());
        assert_eq!(hardware.lock().unwrap().actions, ["read", "recover"]);
    }

    #[test]
    fn ugyldig_eldre_kvittering_sperrer_forste_setter_og_journal() {
        let baseline = baseline();
        let (fixture, mut backend, hardware) = backend(&baseline);
        fixture.write("visual-confirmation.json", b"{}");
        let mut book = Book::new(baseline.clone()).unwrap();
        let error = book
            .apply(&mut backend, 7, Request::Off {}, 10, 10_000)
            .unwrap_err();
        assert!(error.to_string().contains("Ugyldig visuell RGB-kvittering"));
        assert!(!book.snapshot().dirty);
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert_eq!(hardware.lock().unwrap().actions, ["read"]);
    }

    #[test]
    fn synlig_journalrydding_med_fsync_feil_svarer_feil_uten_dirty_status() {
        let baseline = baseline();
        let (_fixture, mut backend, hardware) = backend(&baseline);
        let mut book = Book::new(baseline.clone()).unwrap();
        let txn = book
            .apply(&mut backend, 7, Request::Off {}, 10, 10_000)
            .unwrap()
            .txn
            .unwrap();
        hardware.lock().unwrap().result = Ok(observed());
        book.revert(&mut backend, 7, txn, 20).unwrap();
        backend
            .store
            .fail_sync_after_unlink
            .store(true, Ordering::Relaxed);
        assert!(book.ack_recovery(&mut backend, 7, 30).is_err());
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert!(backend.store.visual_evidence(&baseline).unwrap().is_some());
        assert!(!book.snapshot().dirty);
        assert!(book.snapshot().error.is_some());
        assert!(book.release(&mut backend).is_none());
        assert_eq!(
            hardware.lock().unwrap().actions,
            ["read", "apply", "recover", "read"]
        );
    }

    #[test]
    fn avklaring_uten_arbeider_bindes_til_journalforsok_og_kjent_diskskjema() {
        let baseline = baseline();
        let f = Fixture::new();
        let store = f.store(&baseline);
        let initial = record(&baseline);
        store.write_journal(&baseline, None, &initial).unwrap();
        f.write("visual-confirmation.json", b"{}");
        let (device, hardware) = hardware();
        let auth = Authorization::new(1000, binding(), rigg_core::version::build() + 1).unwrap();
        let mut backend = NativeBackend::from_existing(auth, store, device, false).unwrap();
        let (expected, old_id) = backend.inspect_original().unwrap();
        assert!(hardware.lock().unwrap().actions.is_empty());

        hardware.lock().unwrap().result = Ok(observed());
        backend.restore_original().unwrap();
        let (new_target, new_id) = backend.inspect_original().unwrap();
        assert_eq!(expected, new_target);
        assert_ne!(old_id, new_id);
        assert!(backend.resolve_for(&old_id).is_err());
        assert_eq!(hardware.lock().unwrap().actions, ["recover"]);
        backend.resolve_for(&new_id).unwrap();
        assert_eq!(hardware.lock().unwrap().actions, ["recover", "read"]);
        assert!(backend.store.journal(&baseline).unwrap().is_none());
        assert!(backend.store.visual_evidence(&baseline).unwrap().is_some());
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
