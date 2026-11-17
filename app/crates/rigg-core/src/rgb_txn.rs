//! Avgrenset RGB-transaksjon uten klokke, disk, socket eller maskinvaretilgang.
//!
//! Kalleren må kontrollere særskilt, vedvarende RGB-autorisasjon, klientens UID
//! og kortbinding før `apply`/`keep`. En `Baseline`, `Book` eller eksisterende
//! GPU-autorisasjon gir aldri denne tillatelsen. Ved tilbakekalling må kalleren
//! fortsatt forsøke gjenoppretting av egne endringer.
//!
//! Backend får bare typede fargevalg. Journalen skal være varig før første
//! skriving. Alle forsøkte settere kan ha gitt delvis endring. Ukjent modus
//! forblir ukjent; samsvarende fargeregistre er ikke bekreftet gjenoppretting.

use crate::rgb::{Baseline, BaselineSource, Binding, Mode, Observed, RecoveryReadback, Request};
use serde::{Deserialize, Serialize};

pub const MIN_TIMEOUT_MS: u64 = 10_000;
pub const MAX_TIMEOUT_MS: u64 = 60_000;

/// Datagrunnlaget journalføreren må lagre før den svarer `Ok`.
/// Grunnopptaket inneholder full kortbinding og modusens opprinnelse.
/// Dette er en skrivehensikt, ikke bevis på en forsøkt setter eller tillatelse
/// til automatisk recovery etter restart. En senere diskadapter må bevare
/// avvisning før første setter og tidligere eget recoveryansvar separat.
/// Denne modulen tilbyr ingen automatisk innlasting/gjenopptak fra journalen.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Journal {
    pub version: u32,
    pub baseline: Baseline,
    pub before: Request,
    pub before_observed: Observed,
    pub requested: Request,
    pub owner: u64,
    pub txn: u64,
}

impl Journal {
    /// Kalles også etter lesing fra beskyttet disk. Dette erstatter ikke
    /// journalførerens eier-/modus-/sti- og autorisasjonskontroll.
    pub fn validate(&self, expected: &Binding) -> Result<(), String> {
        if self.version != 1 || self.owner == 0 || self.txn == 0 {
            return Err("RGB-journalen har ugyldig versjon, klient eller transaksjon.".into());
        }
        if self.baseline.binding() != expected {
            return Err("RGB-journalen tilhører et annet kort.".into());
        }
        Book::new(self.baseline.clone()).map_err(|e| e.to_string())?;
        if !matched(request_readback(&self.before, &self.before_observed)) {
            return Err("RGB-journalens forrige valg mangler samsvarende avlesning.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteError {
    pub message: String,
    /// `false` er et løfte om at ingen setter ble forsøkt. Også første
    /// setter som returnerer feil, krever `true`: kallet kan ha hatt effekt.
    pub write_attempted: bool,
}

/// Adapterkontrakt for senere hjelperintegrasjon, og typed fakes i testene.
pub trait Backend {
    /// Fersk lesing etter ny kontroll av forventet identitet. Ingen fallback.
    fn read(&mut self, binding: &Binding) -> Result<Observed, String>;
    /// Atomisk, privat og varig før `Ok`; skal bevare tidligere dirty-tilstand.
    fn mark_dirty(&mut self, journal: &Journal) -> Result<(), String>;
    /// Sammenlign `expected_before` med fersk avlesning på samme bundne
    /// enhet rett før første setter. Samme fargeregistre kan ikke utelukke en
    /// ekstern effektendring når modus ikke kan leses. `Ok` er fersk avlesning.
    fn apply(
        &mut self,
        binding: &Binding,
        request: &Request,
        expected_before: &Observed,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError>;
    /// Egen recovery-sti: behold binding-/signaturkontroll, men ikke krev
    /// at en mulig delvis endring fortsatt er lik forrige bekreftede valg.
    fn recover(
        &mut self,
        binding: &Binding,
        request: &Request,
        baseline: &Baseline,
    ) -> Result<Observed, WriteError>;
    /// Kalles bare etter full tilbakelesing av det opprinnelige grunnopptaket.
    fn clear_dirty(&mut self, binding: &Binding) -> Result<(), String>;
    /// Særskilt menneskelig bekreftelse etter fersk feltavlesning. Backend
    /// lagrer varig userVisual-bevis før eventuell rydding av journalen.
    /// Bare et mål lik originalbaseline rydder eget skriveansvar. Dette er
    /// aldri maskinvarebevis på effektmodus eller en erstatning for read.
    fn ack_recovery(
        &mut self,
        _binding: &Binding,
        _target: &Request,
        _observed: &Observed,
        _baseline: &Baseline,
    ) -> Result<(), String> {
        Err("Denne RGB-adapteren støtter ikke lagret brukerbekreftelse.".into())
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryConfirmationSource {
    UserVisual,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub txn: u64,
    pub owner: u64,
    pub deadline_ms: u64,
    pub before: Request,
    pub before_observed: Observed,
    pub requested: Request,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub baseline_source: BaselineSource,
    /// Bekreftet appvalg. Dette er ikke en påstand om avlest effektmodus.
    pub confirmed: Request,
    pub observed: Observed,
    pub pending: Option<Pending>,
    pub owner: Option<u64>,
    pub dirty: bool,
    /// Journalført hensikt alene er ikke bevis på en forsøkt setter.
    pub may_have_written: bool,
    pub recovery: Option<RecoveryReadback>,
    pub recovery_needs_confirmation: bool,
    pub recovery_confirmation_source: Option<RecoveryConfirmationSource>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Unchanged,
    Pending,
    Kept,
    Reverted,
    BaselineRestored,
    RecoveryNeedsConfirmation,
    RecoveryFailed,
    UserConfirmed,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Apply,
    Keep,
    Revert,
    Timeout,
    Disconnect,
    Release,
    Reset,
    ApplyFailed,
    KeepFailed,
    ClockReversed,
    UserConfirmed,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub txn: Option<u64>,
    pub status: Status,
    pub reason: Reason,
    pub readback: RecoveryReadback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidOwner,
    InvalidTimeout,
    Overflow,
    ClockReversed,
    Busy,
    NotOwner,
    UnknownTransaction,
    RecoveryRequired,
    UnsupportedBaseline,
    Readback(RecoveryReadback),
    Backend {
        operation: &'static str,
        message: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOwner => write!(
                f,
                "RGB-endringen trenger en klientidentitet større enn null."
            ),
            Self::InvalidTimeout => write!(f, "RGB-fristen må være mellom 10 og 60 sekunder."),
            Self::Overflow => write!(f, "RGB-fristen eller transaksjonsnummeret er for stort."),
            Self::ClockReversed => write!(
                f,
                "Den monotone RGB-klokken gikk bakover. Nye endringer er sperret."
            ),
            Self::Busy => write!(
                f,
                "En RGB-endring eller en annen klient holder lysstyringen."
            ),
            Self::NotOwner => write!(f, "RGB-endringen tilhører en annen klient."),
            Self::UnknownTransaction => {
                write!(f, "RGB-transaksjonen finnes ikke eller fristen er ute.")
            }
            Self::RecoveryRequired => {
                write!(f, "Gjenoppretting av lyset må avklares før en ny endring.")
            }
            Self::UnsupportedBaseline => write!(
                f,
                "Grunnopptaket kan ikke gjenopprettes med de avgrensede lysvalgene."
            ),
            Self::Readback(value) => {
                write!(f, "Fersk RGB-tilbakelesing samsvarer ikke: {value:?}.")
            }
            Self::Backend { operation, message } => write!(f, "RGB {operation}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Book {
    baseline: Baseline,
    initial: Request,
    confirmed: Request,
    observed: Observed,
    pending: Option<Pending>,
    owner: Option<u64>,
    dirty: bool,
    may_have_written: bool,
    recovery: Option<RecoveryReadback>,
    recovery_confirmation_source: Option<RecoveryConfirmationSource>,
    error: Option<String>,
    next_txn: u64,
    last_ms: Option<u64>,
}

fn request_readback(request: &Request, observed: &Observed) -> RecoveryReadback {
    let (mode, color, brightness) = match request {
        Request::Static { color, brightness } => (Mode::Static, *color, brightness.get()),
        Request::Off {} => (Mode::Off, [0, 0, 0], 100),
    };
    if observed.color.is_some_and(|v| v != color)
        || observed.brightness_raw.is_some_and(|v| v != brightness)
        || observed.mode.is_some_and(|v| v != mode)
    {
        return RecoveryReadback::Mismatch;
    }
    if observed.color.is_none() || observed.brightness_raw.is_none() {
        return RecoveryReadback::Unknown;
    }
    if observed.mode.is_some() {
        RecoveryReadback::Verified
    } else {
        RecoveryReadback::ReadableFieldsMatch
    }
}

fn matched(readback: RecoveryReadback) -> bool {
    matches!(
        readback,
        RecoveryReadback::Verified | RecoveryReadback::ReadableFieldsMatch
    )
}

impl Book {
    /// Validerer kun grunnopptakets reversible modell, ikke autorisasjon.
    /// Ingen I/O. Et ukjent utgangspunkt må allerede være eksplisitt
    /// `UserConfirmedStatic`; det kan ikke konstrueres av denne boka.
    pub fn new(baseline: Baseline) -> Result<Self, Error> {
        let observed = baseline.observed().clone();
        let initial = match baseline.expected_mode() {
            Mode::Static => Request::Static {
                color: observed.color.ok_or(Error::UnsupportedBaseline)?,
                brightness: observed
                    .brightness_percent()
                    .ok_or(Error::UnsupportedBaseline)?,
            },
            Mode::Off
                if observed.color == Some([0, 0, 0]) && observed.brightness_raw == Some(100) =>
            {
                Request::Off {}
            }
            Mode::Off => return Err(Error::UnsupportedBaseline),
        };
        Ok(Self {
            baseline,
            confirmed: initial.clone(),
            initial,
            observed,
            pending: None,
            owner: None,
            dirty: false,
            may_have_written: false,
            recovery: None,
            recovery_confirmation_source: None,
            error: None,
            next_txn: 1,
            last_ms: None,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            baseline_source: self.baseline.source(),
            confirmed: self.confirmed.clone(),
            observed: self.observed.clone(),
            pending: self.pending.clone(),
            owner: self.owner,
            dirty: self.dirty,
            may_have_written: self.may_have_written,
            recovery: self.recovery,
            recovery_needs_confirmation: self.recovery
                == Some(RecoveryReadback::ReadableFieldsMatch)
                && self.recovery_confirmation_source.is_none(),
            recovery_confirmation_source: self.recovery_confirmation_source,
            error: self.error.clone(),
        }
    }

    fn blocked(&self) -> bool {
        self.error.is_some()
            || self.recovery.is_some_and(|r| {
                r != RecoveryReadback::Verified && self.recovery_confirmation_source.is_none()
            })
    }

    fn observe<B: Backend>(&mut self, backend: &mut B) -> Result<(), Error> {
        match backend.read(self.baseline.binding()) {
            Ok(observed) => {
                if self.recovery_confirmation_source.is_some()
                    && !matched(request_readback(&self.confirmed, &observed))
                {
                    self.recovery_confirmation_source = None;
                    self.recovery = Some(request_readback(&self.confirmed, &observed));
                    self.error =
                        Some("Lyset har endret seg etter den visuelle bekreftelsen.".into());
                }
                self.observed = observed;
                Ok(())
            }
            Err(message) => {
                self.observed = Observed::default();
                if self.recovery_confirmation_source.take().is_some() {
                    self.recovery = Some(RecoveryReadback::Unknown);
                    self.error = Some("Fersk lesing etter visuell bekreftelse feilet.".into());
                }
                Err(Error::Backend {
                    operation: "lesing",
                    message,
                })
            }
        }
    }

    /// `now_ms` er monoton tid levert av kalleren, aldri veggklokke. Produksjon
    /// må inkludere hviletid, for eksempel Linux CLOCK_BOOTTIME, slik at en
    /// prøve som utløper under suspend, rulles tilbake straks ved oppvåkning.
    pub fn apply<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        request: Request,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Result<Outcome, Error> {
        self.tick(backend, now_ms)?;
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(Error::InvalidTimeout);
        }
        let deadline_ms = now_ms.checked_add(timeout_ms).ok_or(Error::Overflow)?;
        let next = self.next_txn.checked_add(1).ok_or(Error::Overflow)?;
        if self.blocked() {
            return Err(Error::RecoveryRequired);
        }
        if self.pending.is_some() || self.owner.is_some_and(|held| held != owner) {
            return Err(Error::Busy);
        }
        self.observe(backend)?;
        let before_match = request_readback(&self.confirmed, &self.observed);
        if !matched(before_match) {
            return Err(Error::Readback(before_match));
        }
        if request == self.confirmed {
            return Ok(Outcome {
                txn: None,
                status: Status::Unchanged,
                reason: Reason::Apply,
                readback: before_match,
            });
        }
        let pending = Pending {
            txn: self.next_txn,
            owner,
            deadline_ms,
            before: self.confirmed.clone(),
            before_observed: self.observed.clone(),
            requested: request.clone(),
        };
        let journal = Journal {
            version: 1,
            baseline: self.baseline.clone(),
            before: pending.before.clone(),
            before_observed: pending.before_observed.clone(),
            requested: request.clone(),
            owner,
            txn: pending.txn,
        };
        backend
            .mark_dirty(&journal)
            .map_err(|message| Error::Backend {
                operation: "journalføring",
                message,
            })?;
        self.dirty = true;
        self.owner = Some(owner);
        self.next_txn = next;
        self.pending = Some(pending.clone());
        self.recovery = None;
        self.recovery_confirmation_source = None;
        let result = backend.apply(
            self.baseline.binding(),
            &request,
            &pending.before_observed,
            &self.baseline,
        );
        let failure = match result {
            Ok(observed) => {
                self.may_have_written = true;
                self.observed = observed;
                let readback = request_readback(&request, &self.observed);
                if matched(readback) {
                    return Ok(Outcome {
                        txn: Some(pending.txn),
                        status: Status::Pending,
                        reason: Reason::Apply,
                        readback,
                    });
                }
                Error::Readback(readback)
            }
            Err(failure) => {
                self.observed = Observed::default();
                if !failure.write_attempted {
                    // En annen aktør kan ha endret kortet siden førlesingen.
                    // Ingen egen setter ble forsøkt: ikke overskriv aktøren
                    // med en automatisk rollback. Journalen må avklares.
                    self.pending = None;
                    self.recovery = Some(RecoveryReadback::Unknown);
                    self.error = Some(format!(
                        "Endringen ble avvist før første setter: {}",
                        failure.message
                    ));
                    return Err(Error::Backend {
                        operation: "endring",
                        message: failure.message,
                    });
                }
                self.may_have_written = true;
                Error::Backend {
                    operation: "endring",
                    message: failure.message,
                }
            }
        };
        // Også første feilende setter kan ha virket. Ett rollback-forsøk;
        // senere tick skal aldri fortsette en automatisk recovery-løkke.
        self.recover(
            backend,
            pending.before,
            Some(pending.txn),
            Reason::ApplyFailed,
        );
        Err(failure)
    }

    fn owned_pending(&self, owner: u64, txn: u64) -> Result<Pending, Error> {
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        let pending = self
            .pending
            .as_ref()
            .filter(|p| p.txn == txn)
            .ok_or(Error::UnknownTransaction)?;
        if pending.owner != owner {
            return Err(Error::NotOwner);
        }
        Ok(pending.clone())
    }

    pub fn keep<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        txn: u64,
        now_ms: u64,
    ) -> Result<Outcome, Error> {
        self.tick(backend, now_ms)?;
        let pending = self.owned_pending(owner, txn)?;
        if let Err(error) = self.observe(backend) {
            self.recover(backend, pending.before, Some(txn), Reason::KeepFailed);
            return Err(error);
        }
        let readback = request_readback(&pending.requested, &self.observed);
        if !matched(readback) {
            self.recover(backend, pending.before, Some(txn), Reason::KeepFailed);
            return Err(Error::Readback(readback));
        }
        self.confirmed = pending.requested;
        self.pending = None;
        Ok(Outcome {
            txn: Some(txn),
            status: Status::Kept,
            reason: Reason::Keep,
            readback,
        })
    }

    pub fn revert<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        txn: u64,
        now_ms: u64,
    ) -> Result<Outcome, Error> {
        self.tick(backend, now_ms)?;
        let pending = self.owned_pending(owner, txn)?;
        Ok(self.recover(backend, pending.before, Some(txn), Reason::Revert))
    }

    /// Utløp gjør høyst ett gjenopprettingsforsøk. Etterpå beholdes en
    /// uavklart dirty-tilstand uten nye skrivinger fra timeren.
    pub fn tick<B: Backend>(
        &mut self,
        backend: &mut B,
        now_ms: u64,
    ) -> Result<Option<Outcome>, Error> {
        if self.last_ms.is_some_and(|previous| now_ms < previous) {
            if !self.blocked() && self.dirty && self.may_have_written {
                let (target, txn) = self
                    .pending
                    .as_ref()
                    .map(|p| (p.before.clone(), Some(p.txn)))
                    .unwrap_or_else(|| (self.initial.clone(), None));
                self.recover(backend, target, txn, Reason::ClockReversed);
            }
            self.error = Some("Den monotone RGB-klokken gikk bakover.".into());
            return Err(Error::ClockReversed);
        }
        self.last_ms = Some(now_ms);
        let Some(pending) = self
            .pending
            .as_ref()
            .filter(|p| now_ms >= p.deadline_ms)
            .cloned()
        else {
            return Ok(None);
        };
        Ok(Some(self.recover(
            backend,
            pending.before,
            Some(pending.txn),
            Reason::Timeout,
        )))
    }

    /// Hjelperen bruker dette ved klienttap. En annen klient påvirker ingenting.
    pub fn disconnect<B: Backend>(&mut self, backend: &mut B, owner: u64) -> Option<Outcome> {
        if self.owner != Some(owner) || owner == 0 || !self.dirty {
            return None;
        }
        if !self.may_have_written {
            self.owner = None;
            return None;
        }
        let outcome = self.recover(
            backend,
            self.initial.clone(),
            self.pending.as_ref().map(|p| p.txn),
            Reason::Disconnect,
        );
        self.owner = None;
        Some(outcome)
    }

    /// Automatisk frigivelse ved tilbakekalt tillatelse eller nedstenging.
    /// En journalført hensikt som ble avvist før første setter, gir ingen
    /// skriving her. Uavklart journal beholdes for særskilt avklaring.
    pub fn release<B: Backend>(&mut self, backend: &mut B) -> Option<Outcome> {
        let owner = self.owner?;
        self.disconnect(backend, owner).map(|mut outcome| {
            outcome.reason = Reason::Release;
            outcome
        })
    }

    /// Særskilt, eksplisitt autorisert recovery/reset; ikke shutdown eller en
    /// periodisk retry. Kan også gjenopprette etter en avvist skrivehensikt.
    /// Kalleren må velge dette bevisst etter å ha vist den uavklarte tilstanden.
    pub fn restore_baseline<B: Backend>(&mut self, backend: &mut B) -> Option<Outcome> {
        self.dirty.then(|| {
            self.recover(
                backend,
                self.initial.clone(),
                self.pending.as_ref().map(|p| p.txn),
                Reason::Reset,
            )
        })
    }

    /// Kalles bare etter eksplisitt visuell bekreftelse fra autorisert UID.
    /// En eier må være samme forbindelse; en forlatt recovery kan bekreftes
    /// av en ny forbindelse for samme UID, som runtime kontrollerer.
    pub fn ack_recovery<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        now_ms: u64,
    ) -> Result<Outcome, Error> {
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        if self.owner.is_some_and(|held| held != owner) {
            return Err(Error::NotOwner);
        }
        if !self.dirty
            || self.pending.is_some()
            || self.error.is_some()
            || self.recovery != Some(RecoveryReadback::ReadableFieldsMatch)
            || self.recovery_confirmation_source.is_some()
        {
            return Err(Error::RecoveryRequired);
        }
        self.tick(backend, now_ms)?;
        if let Err(error) = self.observe(backend) {
            self.recovery = Some(RecoveryReadback::Unknown);
            self.error = Some(error.to_string());
            return Err(error);
        }
        let original = self.confirmed == self.initial;
        let readback = if original {
            self.baseline
                .recovery_readback(self.baseline.binding(), &self.observed)
        } else {
            request_readback(&self.confirmed, &self.observed)
        };
        if readback != RecoveryReadback::ReadableFieldsMatch {
            self.recovery = Some(readback);
            self.error = Some("Lyset endret seg før brukerbekreftelsen kunne lagres.".into());
            return Err(Error::Readback(readback));
        }
        if let Err(message) = backend.ack_recovery(
            self.baseline.binding(),
            &self.confirmed,
            &self.observed,
            &self.baseline,
        ) {
            self.error = Some(format!("Brukerbekreftelsen kunne ikke lagres: {message}"));
            return Err(Error::Backend {
                operation: "brukerbekreftelse",
                message,
            });
        }
        self.recovery_confirmation_source = Some(RecoveryConfirmationSource::UserVisual);
        if original {
            self.dirty = false;
            self.may_have_written = false;
            self.owner = None;
        } else {
            self.owner = Some(owner);
        }
        Ok(Outcome {
            txn: None,
            status: Status::UserConfirmed,
            reason: Reason::UserConfirmed,
            readback,
        })
    }

    fn recover<B: Backend>(
        &mut self,
        backend: &mut B,
        target: Request,
        txn: Option<u64>,
        reason: Reason,
    ) -> Outcome {
        self.pending = None;
        self.recovery = Some(RecoveryReadback::Unknown);
        self.recovery_confirmation_source = None;
        let baseline_target = target == self.initial;
        // `dirty` var satt etter varig journalføring, før opprinnelig apply.
        let readback = match backend.recover(self.baseline.binding(), &target, &self.baseline) {
            Err(failure) => {
                self.may_have_written |= failure.write_attempted;
                self.observed = Observed::default();
                self.error = Some(format!("Gjenoppretting feilet: {}", failure.message));
                RecoveryReadback::Unknown
            }
            Ok(observed) => {
                self.may_have_written = true;
                self.observed = observed;
                let readback = if baseline_target {
                    self.baseline
                        .recovery_readback(self.baseline.binding(), &self.observed)
                } else {
                    request_readback(&target, &self.observed)
                };
                if matched(readback) {
                    self.confirmed = target;
                    self.error = None;
                } else {
                    self.error =
                        Some("Gjenoppretting samsvarer ikke med fersk tilbakelesing.".into());
                }
                if readback == RecoveryReadback::Verified && baseline_target {
                    match backend.clear_dirty(self.baseline.binding()) {
                        Ok(()) => {
                            self.dirty = false;
                            self.may_have_written = false;
                            self.owner = None;
                        }
                        Err(message) => {
                            self.error = Some(format!(
                                "Gjenopprettet, men journalen kunne ikke ryddes: {message}"
                            ));
                        }
                    }
                }
                readback
            }
        };
        self.recovery = Some(readback);
        let status = match readback {
            RecoveryReadback::Verified if self.error.is_none() && baseline_target => {
                Status::BaselineRestored
            }
            RecoveryReadback::Verified if self.error.is_none() => Status::Reverted,
            RecoveryReadback::ReadableFieldsMatch => Status::RecoveryNeedsConfirmation,
            _ => Status::RecoveryFailed,
        };
        Outcome {
            txn,
            status,
            reason,
            readback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgb::{AdapterIdentity, Brightness, PciIdentity, Signature};

    fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-test-identity".into(),
            pci_ids: PciIdentity {
                vendor: 0x10de,
                device: 0x1e87,
                subsystem_vendor: 0x10de,
                subsystem_device: 0x1e87,
            },
            adapter: AdapterIdentity {
                port: 1,
                name: "NVIDIA i2c adapter 1 at 1c:00.0".into(),
                parent_pci: "0000:1c:00.0".into(),
                driver: "nvidia".into(),
            },
            address: 0x49,
            signature: Signature::Palit150,
        }
    }

    fn color(rgb: [u8; 3]) -> Request {
        Request::Static {
            color: rgb,
            brightness: Brightness::try_from(80).unwrap(),
        }
    }

    fn observation(request: &Request, known_mode: bool) -> Observed {
        let (mode, color, brightness) = match request {
            Request::Static { color, brightness } => (Mode::Static, *color, brightness.get()),
            Request::Off {} => (Mode::Off, [0, 0, 0], 100),
        };
        Observed {
            color: Some(color),
            brightness_raw: Some(brightness),
            mode: known_mode.then_some(mode),
        }
    }

    #[derive(Default)]
    struct Fake {
        observed: Observed,
        known_mode: bool,
        calls: Vec<&'static str>,
        journal: Option<Journal>,
        writes: Vec<Request>,
        fail_mark: bool,
        fail_write_numbers: Vec<usize>,
        mismatch_write_numbers: Vec<usize>,
        fail_clear: bool,
        drift_after_read: bool,
        fail_ack: bool,
        fail_read: bool,
        acknowledgments: Vec<Request>,
    }

    impl Fake {
        fn write(
            &mut self,
            expected: &Binding,
            request: &Request,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            assert_eq!(expected, baseline.binding());
            assert!(
                self.journal.is_some(),
                "journalen må komme før alle skrivinger"
            );
            self.calls.push("write");
            self.writes.push(request.clone());
            self.observed = observation(request, self.known_mode);
            if self.fail_write_numbers.contains(&self.writes.len()) {
                // Simuler feilsvar etter at én setter allerede endret fargen.
                self.observed.color = Some([17, 18, 19]);
                return Err(WriteError {
                    message: "mulig delvis skriving".into(),
                    write_attempted: true,
                });
            }
            if self.mismatch_write_numbers.contains(&self.writes.len()) {
                self.observed.color = Some([99, 98, 97]);
            }
            Ok(self.observed.clone())
        }
    }

    impl Backend for Fake {
        fn read(&mut self, expected: &Binding) -> Result<Observed, String> {
            assert_eq!(expected, &binding());
            self.calls.push("read");
            if self.fail_read {
                return Err("simulert lesefeil".into());
            }
            let result = self.observed.clone();
            if self.drift_after_read {
                self.observed.color = Some([42, 43, 44]);
            }
            Ok(result)
        }
        fn mark_dirty(&mut self, journal: &Journal) -> Result<(), String> {
            self.calls.push("dirty");
            if self.fail_mark {
                return Err("full disk".into());
            }
            self.journal = Some(journal.clone());
            Ok(())
        }
        fn apply(
            &mut self,
            expected: &Binding,
            request: &Request,
            before: &Observed,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            if self.observed != *before {
                return Err(WriteError {
                    message: "Kortet ble endret etter førlesingen.".into(),
                    write_attempted: false,
                });
            }
            self.write(expected, request, baseline)
        }
        fn recover(
            &mut self,
            expected: &Binding,
            request: &Request,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.write(expected, request, baseline)
        }
        fn clear_dirty(&mut self, _: &Binding) -> Result<(), String> {
            self.calls.push("clear");
            if self.fail_clear {
                return Err("journalen kan ikke ryddes".into());
            }
            self.journal = None;
            Ok(())
        }
        fn ack_recovery(
            &mut self,
            expected: &Binding,
            target: &Request,
            observed: &Observed,
            baseline: &Baseline,
        ) -> Result<(), String> {
            assert_eq!(expected, baseline.binding());
            self.calls.push("visual");
            if self.fail_ack || observed != &self.observed {
                return Err("visuell kvittering kunne ikke lagres".into());
            }
            self.acknowledgments.push(target.clone());
            if target == &Book::new(baseline.clone()).unwrap().initial {
                self.journal = None;
            }
            Ok(())
        }
    }

    fn setup(known_mode: bool) -> (Book, Fake) {
        let observed = observation(&color([1, 2, 3]), known_mode);
        let baseline = if known_mode {
            Baseline::from_observed(binding(), observed.clone())
        } else {
            Baseline::user_confirmed_static(binding(), observed.clone())
        }
        .unwrap();
        (
            Book::new(baseline).unwrap(),
            Fake {
                observed,
                known_mode,
                ..Default::default()
            },
        )
    }

    #[test]
    fn visuell_originalbekreftelse_er_fersk_og_ikke_maskinvaremodus() {
        let (mut book, mut hw) = setup(false);
        assert_eq!(
            book.ack_recovery(&mut hw, 7, 0),
            Err(Error::RecoveryRequired)
        );
        let initial = book.baseline.clone();
        let pending = book
            .apply(&mut hw, 7, color([9, 8, 7]), 10, 10_000)
            .unwrap();
        book.revert(&mut hw, 7, pending.txn.unwrap(), 20).unwrap();
        let calls = hw.calls.len();
        assert_eq!(book.ack_recovery(&mut hw, 8, 25), Err(Error::NotOwner));
        assert_eq!(hw.calls.len(), calls);
        let result = book.ack_recovery(&mut hw, 7, 30).unwrap();
        assert_eq!(result.status, Status::UserConfirmed);
        assert_eq!(result.reason, Reason::UserConfirmed);
        assert_eq!(result.readback, RecoveryReadback::ReadableFieldsMatch);
        assert_eq!(&hw.calls[calls..], ["read", "visual"]);
        assert_eq!(book.snapshot().observed.mode, None);
        assert_eq!(
            book.snapshot().recovery_confirmation_source,
            Some(RecoveryConfirmationSource::UserVisual)
        );
        assert!(!book.snapshot().recovery_needs_confirmation);
        assert!(!book.snapshot().dirty);
        assert!(!book.snapshot().may_have_written);
        assert_eq!(book.snapshot().owner, None);
        assert_eq!(book.baseline, initial);
        assert!(hw.journal.is_none());
        assert_eq!(hw.writes.len(), 2);
        assert_eq!(
            book.ack_recovery(&mut hw, 7, 31),
            Err(Error::RecoveryRequired)
        );
    }

    #[test]
    fn visuell_forrigebekreftelse_beholder_ansvar_og_apner_ny_apply() {
        let (mut book, mut hw) = setup(false);
        let kept = color([10, 20, 30]);
        let first = book.apply(&mut hw, 7, kept.clone(), 10, 10_000).unwrap();
        book.keep(&mut hw, 7, first.txn.unwrap(), 20).unwrap();
        let second = book
            .apply(&mut hw, 7, color([40, 50, 60]), 30, 10_000)
            .unwrap();
        book.revert(&mut hw, 7, second.txn.unwrap(), 40).unwrap();
        book.ack_recovery(&mut hw, 7, 50).unwrap();
        assert_eq!(book.snapshot().confirmed, kept);
        assert!(book.snapshot().dirty && book.snapshot().may_have_written);
        assert_eq!(book.snapshot().owner, Some(7));
        assert!(hw.journal.is_some());
        book.apply(&mut hw, 7, color([70, 80, 90]), 60, 10_000)
            .unwrap();
        assert_eq!(book.snapshot().recovery_confirmation_source, None);
        book.release(&mut hw).unwrap();
        assert_eq!(book.snapshot().recovery_confirmation_source, None);
        assert!(book.snapshot().recovery_needs_confirmation);
    }

    #[test]
    fn forlatt_recovery_kan_bekreftes_av_ny_autorisert_forbindelse() {
        let (mut book, mut hw) = setup(false);
        book.apply(&mut hw, 7, color([9, 8, 7]), 10, 10_000)
            .unwrap();
        book.disconnect(&mut hw, 7).unwrap();
        assert_eq!(book.snapshot().owner, None);
        let writes = hw.writes.len();
        book.ack_recovery(&mut hw, 8, 20).unwrap();
        assert_eq!(hw.writes.len(), writes);
        assert!(!book.snapshot().dirty);
    }

    #[test]
    fn drift_ufullstendig_lesing_og_io_gir_ingen_visuell_kvittering() {
        for fault in 0..4 {
            let (mut book, mut hw) = setup(false);
            let pending = book
                .apply(&mut hw, 7, color([9, 8, 7]), 10, 10_000)
                .unwrap();
            book.revert(&mut hw, 7, pending.txn.unwrap(), 20).unwrap();
            match fault {
                0 => hw.observed.color = Some([5, 5, 5]),
                1 => hw.observed.brightness_raw = None,
                2 => hw.fail_read = true,
                _ => hw.fail_ack = true,
            }
            assert!(book.ack_recovery(&mut hw, 7, 30).is_err());
            assert!(book.snapshot().dirty);
            assert!(book.snapshot().may_have_written);
            assert_eq!(book.snapshot().recovery_confirmation_source, None);
            assert!(hw.journal.is_some());
            assert!(hw.acknowledgments.is_empty());
            assert_eq!(hw.writes.len(), 2);
        }
    }

    #[test]
    fn dirty_foer_write_og_behold_er_ikke_modusbevis() {
        let (mut book, mut hw) = setup(false);
        let request = color([9, 8, 7]);
        let pending = book
            .apply(&mut hw, 7, request.clone(), 100, 10_000)
            .unwrap();
        assert_eq!(hw.calls, ["read", "dirty", "write"]);
        let journal = hw.journal.as_ref().unwrap();
        assert_eq!(journal.before, color([1, 2, 3]));
        assert_eq!(journal.requested, request);
        assert_eq!(
            journal.baseline.source(),
            BaselineSource::UserConfirmedStatic
        );
        let result = book.keep(&mut hw, 7, pending.txn.unwrap(), 200).unwrap();
        assert_eq!(result.readback, RecoveryReadback::ReadableFieldsMatch);
        assert_eq!(book.snapshot().observed.mode, None);
        assert_eq!(book.snapshot().confirmed, request);
        assert!(book.snapshot().dirty);
        assert_eq!(hw.writes.len(), 1, "Behold skriver ikke igjen");
    }

    #[test]
    fn journalfeil_og_ugyldige_argumenter_gir_ingen_skriving() {
        let (mut book, mut hw) = setup(false);
        assert_eq!(
            book.apply(&mut hw, 0, color([9, 8, 7]), 0, 10_000),
            Err(Error::InvalidOwner)
        );
        assert_eq!(
            book.apply(&mut hw, 1, color([9, 8, 7]), 0, 9_999),
            Err(Error::InvalidTimeout)
        );
        assert_eq!(
            book.apply(&mut hw, 1, color([9, 8, 7]), u64::MAX, 10_000),
            Err(Error::Overflow)
        );
        let (mut book, mut hw2) = setup(false);
        hw2.fail_mark = true;
        assert!(matches!(
            book.apply(&mut hw2, 1, color([9, 8, 7]), 0, 10_000),
            Err(Error::Backend {
                operation: "journalføring",
                ..
            })
        ));
        assert!(hw.writes.is_empty() && hw2.writes.is_empty());
        assert!(book.snapshot().pending.is_none());
    }

    #[test]
    fn bare_eieren_kan_beholde_rulle_tilbake_eller_fortsette() {
        let (mut book, mut hw) = setup(false);
        let txn = book
            .apply(&mut hw, 7, color([9, 8, 7]), 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        assert_eq!(book.keep(&mut hw, 8, txn, 10), Err(Error::NotOwner));
        assert_eq!(book.revert(&mut hw, 8, txn, 20), Err(Error::NotOwner));
        assert!(book.disconnect(&mut hw, 8).is_none());
        book.keep(&mut hw, 7, txn, 30).unwrap();
        assert_eq!(
            book.apply(&mut hw, 8, color([6, 5, 4]), 40, 10_000),
            Err(Error::Busy)
        );
        assert_eq!(hw.writes.len(), 1);
    }

    #[test]
    fn revert_bruker_forrige_valg_disconnect_opprinnelig_grunnopptak() {
        let (mut book, mut hw) = setup(false);
        let txn = book
            .apply(&mut hw, 7, color([9, 8, 7]), 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        let txn = book
            .apply(&mut hw, 7, Request::Off {}, 2, 10_000)
            .unwrap()
            .txn
            .unwrap();
        let result = book.revert(&mut hw, 7, txn, 3).unwrap();
        assert_eq!(result.status, Status::RecoveryNeedsConfirmation);
        assert_eq!(hw.writes.last(), Some(&color([9, 8, 7])));
        assert!(book.snapshot().recovery_needs_confirmation);
        assert_eq!(
            book.apply(&mut hw, 7, Request::Off {}, 4, 10_000),
            Err(Error::RecoveryRequired)
        );
        book.disconnect(&mut hw, 7).unwrap();
        assert_eq!(hw.writes.last(), Some(&color([1, 2, 3])));
        assert_eq!(
            book.snapshot().baseline_source,
            BaselineSource::UserConfirmedStatic
        );
        assert!(book.snapshot().dirty && book.snapshot().recovery_needs_confirmation);
        let count = hw.writes.len();
        assert!(book.disconnect(&mut hw, 7).is_none());
        assert_eq!(hw.writes.len(), count);
    }

    #[test]
    fn utloept_behold_gir_ett_rollback_og_aldri_automatisk_retry() {
        let (mut book, mut hw) = setup(false);
        let txn = book
            .apply(&mut hw, 7, color([9, 8, 7]), 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        assert_eq!(
            book.keep(&mut hw, 7, txn, 10_000),
            Err(Error::UnknownTransaction)
        );
        assert_eq!(hw.writes.len(), 2);
        assert!(book.snapshot().recovery_needs_confirmation);
        for now in [10_001, 20_000, 60_000] {
            assert_eq!(book.tick(&mut hw, now), Ok(None));
        }
        assert_eq!(hw.writes.len(), 2);
        assert!(!hw.calls.contains(&"clear"));
    }

    #[test]
    fn delvis_skriving_og_mislykket_recovery_beholder_dirty() {
        let (mut book, mut hw) = setup(false);
        hw.fail_write_numbers = vec![1, 2];
        assert!(book.apply(&mut hw, 7, color([9, 8, 7]), 0, 10_000).is_err());
        assert_eq!(hw.writes.len(), 2);
        let state = book.snapshot();
        assert!(state.dirty && state.error.is_some());
        assert_eq!(state.recovery, Some(RecoveryReadback::Unknown));
        assert_eq!(state.observed, Observed::default());
        assert_eq!(book.tick(&mut hw, 20_000), Ok(None));
        assert_eq!(
            book.apply(&mut hw, 7, Request::Off {}, 20_001, 10_000),
            Err(Error::RecoveryRequired)
        );
        assert_eq!(hw.writes.len(), 2);
        assert!(hw.journal.is_some());
    }

    #[test]
    fn feil_tilbakelesing_proever_rollback_uten_aa_fjerne_journalen() {
        let (mut book, mut hw) = setup(false);
        hw.mismatch_write_numbers = vec![1, 2];
        assert_eq!(
            book.apply(&mut hw, 7, Request::Off {}, 0, 10_000),
            Err(Error::Readback(RecoveryReadback::Mismatch))
        );
        assert_eq!(book.snapshot().recovery, Some(RecoveryReadback::Mismatch));
        assert!(book.snapshot().dirty);
        assert_eq!(hw.writes.len(), 2);
        assert!(!hw.calls.contains(&"clear"));
    }

    #[test]
    fn bare_helt_verifisert_grunnopptak_kan_rydde_dirty() {
        let (mut book, mut hw) = setup(true);
        let txn = book
            .apply(&mut hw, 7, Request::Off {}, 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        let result = book.revert(&mut hw, 7, txn, 1).unwrap();
        assert_eq!(result.readback, RecoveryReadback::Verified);
        assert!(!book.snapshot().dirty);
        assert_eq!(book.snapshot().owner, None);
        assert_eq!(hw.calls.last(), Some(&"clear"));
        assert!(book.restore_baseline(&mut hw).is_none());
    }

    #[test]
    fn journalryddefeil_og_baklengs_klokke_sperrer_videre_endringer() {
        let (mut book, mut hw) = setup(true);
        hw.fail_clear = true;
        let txn = book
            .apply(&mut hw, 7, Request::Off {}, 100, 10_000)
            .unwrap()
            .txn
            .unwrap();
        book.revert(&mut hw, 7, txn, 101).unwrap();
        assert!(book.snapshot().dirty && book.snapshot().error.is_some());
        assert_eq!(
            book.apply(&mut hw, 7, Request::Off {}, 102, 10_000),
            Err(Error::RecoveryRequired)
        );
        let (mut book, mut hw) = setup(false);
        book.apply(&mut hw, 7, Request::Off {}, 100, 10_000)
            .unwrap();
        assert_eq!(book.tick(&mut hw, 99), Err(Error::ClockReversed));
        assert_eq!(hw.writes.len(), 2);
        assert_eq!(book.tick(&mut hw, 98), Err(Error::ClockReversed));
        assert_eq!(hw.writes.len(), 2);
    }

    #[test]
    fn ukjent_mode_og_ikke_reversibel_av_baseline_avvises() {
        assert!(Baseline::from_observed(binding(), observation(&color([1, 2, 3]), false)).is_err());
        let mut obs = observation(&color([1, 2, 3]), true);
        obs.mode = Some(Mode::Off);
        let baseline = Baseline::from_observed(binding(), obs).unwrap();
        assert!(matches!(
            Book::new(baseline),
            Err(Error::UnsupportedBaseline)
        ));
        let (mut book, mut hw) = setup(false);
        assert!(book.restore_baseline(&mut hw).is_none());
        assert!(hw.calls.is_empty());
        hw.observed.color = Some([8, 8, 8]);
        assert_eq!(
            book.apply(&mut hw, 7, Request::Off {}, 0, 10_000),
            Err(Error::Readback(RecoveryReadback::Mismatch))
        );
        assert!(hw.writes.is_empty());
    }

    #[test]
    fn endring_mellom_lesing_og_apply_utloeser_ingen_rollback_skriving() {
        let (mut book, mut hw) = setup(false);
        hw.drift_after_read = true;
        assert!(book.apply(&mut hw, 7, Request::Off {}, 0, 10_000).is_err());
        assert_eq!(hw.observed.color, Some([42, 43, 44]));
        assert!(
            hw.writes.is_empty(),
            "guard-feilen må ikke omgås av automatisk recovery"
        );
        assert!(hw.journal.is_some() && book.snapshot().dirty);
        assert_eq!(book.snapshot().recovery, Some(RecoveryReadback::Unknown));
        assert!(book.snapshot().error.is_some());
        assert_eq!(book.tick(&mut hw, 10_000), Ok(None));
        assert!(!book.snapshot().may_have_written);
        assert!(book.disconnect(&mut hw, 7).is_none());
        assert!(book.release(&mut hw).is_none());
        assert!(hw.writes.is_empty());
    }

    #[test]
    fn journalen_rundtures_og_validerer_kort_eier_og_foertilstand() {
        let (mut book, mut hw) = setup(false);
        book.apply(&mut hw, 7, Request::Off {}, 0, 10_000).unwrap();
        let journal = hw.journal.as_ref().unwrap();
        let wire = serde_json::to_value(journal).unwrap();
        let parsed: Journal = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(parsed, *journal);
        assert!(parsed.validate(&binding()).is_ok());
        let mut bad = parsed.clone();
        bad.owner = 0;
        assert!(bad.validate(&binding()).is_err());
        let mut bad = parsed.clone();
        bad.before_observed.color = None;
        assert!(bad.validate(&binding()).is_err());
        let mut other = binding();
        other.uuid = "GPU-other-card".into();
        assert!(parsed.validate(&other).is_err());
        let mut unknown = wire;
        unknown["path"] = serde_json::json!("/dev/i2c-8");
        assert!(serde_json::from_value::<Journal>(unknown).is_err());
    }

    #[test]
    fn recoveryfeil_har_aldri_en_vellykket_status() {
        for fail in [true, false] {
            let (mut book, mut hw) = setup(false);
            book.apply(&mut hw, 7, Request::Off {}, 0, 10_000).unwrap();
            if fail {
                hw.fail_write_numbers = vec![2];
            } else {
                hw.mismatch_write_numbers = vec![2];
            }
            let outcome = book.restore_baseline(&mut hw).unwrap();
            assert_eq!(outcome.status, Status::RecoveryFailed);
            assert!(book.snapshot().dirty);
        }
    }

    #[test]
    fn avvist_ny_hensikt_beholder_ansvar_for_tidligere_egne_endringer() {
        let (mut book, mut hw) = setup(false);
        let txn = book
            .apply(&mut hw, 7, color([9, 8, 7]), 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        hw.drift_after_read = true;
        assert!(book.apply(&mut hw, 7, Request::Off {}, 2, 10_000).is_err());
        assert_eq!(hw.writes.len(), 1);
        assert!(book.snapshot().may_have_written);
        let result = book.release(&mut hw).unwrap();
        assert_eq!(result.reason, Reason::Release);
        assert_eq!(result.status, Status::RecoveryNeedsConfirmation);
        assert_eq!(hw.writes.last(), Some(&color([1, 2, 3])));
        assert_eq!(hw.writes.len(), 2);
        assert!(book.release(&mut hw).is_none());
        assert_eq!(hw.writes.len(), 2);
    }
}
