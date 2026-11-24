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

use crate::rgb::{Baseline, BaselineSource, Binding, Brightness, Mode, Observed, RecoveryReadback, Request};
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
        let initial = Book::new(self.baseline.clone()).map_err(|e| e.to_string())?.initial;
        // En gjenoppretting av utgangspunktet etter avvik kortet selv sto for
        // (se `Book::restore_drift`): forrige valg ER utgangspunktet, og
        // avlesningen før viser det kortet faktisk hadde.
        let drift_restore = self.before == initial && self.requested == initial;
        if !drift_restore && !matched(request_readback(&self.before, &self.before_observed)) {
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
    /// Etter en lagringsfeil kan en omdøping eller sletting være synlig.
    /// En adapter må beholde ansvaret hvis trygg rydding ikke kan bekreftes.
    fn journal_present(&self) -> Option<bool> {
        None
    }
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
    /// Ett statisk bilde under en allerede beholdt transaksjon. Den første
    /// setterens varige journal og eget skriveansvar må fortsatt være gyldige.
    /// Sammenlign siste egne avlesning med ferske felt på samme bundne enhet
    /// før første setter. Ingen ny journal eller fsync kreves per bilde;
    /// journalen bevarer den lagrede originalen for senere avklaring.
    fn animation_frame(
        &mut self,
        _binding: &Binding,
        _request: &Request,
        _expected_before: &Observed,
        _baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        Err(WriteError {
            message: "Denne RGB-adapteren støtter ikke programvareanimasjon.".into(),
            write_attempted: false,
        })
    }
    /// «Hold» i en ren økt: skriv det lagrede utgangspunktet på nytt, uten
    /// journal. Bare når `expected_before` er adapterens egen ferske avlesning
    /// og alle lesbare felt allerede er lik utgangspunktet; transporten
    /// sammenligner på nytt rett før første setter. Et avbrudd kan da bare
    /// etterlate utgangspunktets egne verdier (se `Book::refresh`).
    fn refresh_baseline(
        &mut self,
        _binding: &Binding,
        _target: &Request,
        _expected_before: &Observed,
        _baseline: &Baseline,
    ) -> Result<Observed, WriteError> {
        Err(WriteError {
            message: "Denne RGB-adapteren kan ikke holde utgangspunktet.".into(),
            write_attempted: false,
        })
    }
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
    /// Etter en programvareeffekt (eller «Hold») har denne hjelperen selv
    /// skrevet hvert bilde og så utgangspunktet med den samme faste sekvensen.
    /// Stemmer alle lesbare felt med utgangspunktet, kan journalen ryddes uten
    /// en ny visuell bekreftelse. Brukes bare ved frakobling/frigivelse fra en
    /// kjørende effekt, aldri etter en prøve som ikke ble beholdt.
    fn settle_after_animation(&mut self, _binding: &Binding) -> Result<(), String> {
        Err("Denne RGB-adapteren kan ikke avslutte en effekt uten bekreftelse.".into())
    }
    /// Gjør et beholdt, tilbakelest statisk valg til nytt varig utgangspunkt
    /// (det kortet går tilbake til når appen lukkes). Det opprinnelige
    /// fabrikklyset må bevares for en vei tilbake. Varig før `Ok`; en
    /// eventuell journal for det gamle utgangspunktet ryddes først etter at
    /// det nye er lagret og fabrikklyset er sikret.
    fn adopt_baseline(&mut self, _binding: &Binding, _next: &Baseline) -> Result<(), String> {
        Err("Denne RGB-adapteren kan ikke lagre et varig utgangspunkt.".into())
    }
    /// Lyset kortet hadde før appen første gang lagret et varig valg
    /// (fabrikklyset), når det er sikret. `None`: utgangspunktet er fortsatt det.
    fn factory_baseline(&self) -> Option<Baseline> {
        None
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
    pub recovery_attempt: Option<u64>,
    pub read_error: Option<String>,
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
    // En frame avvist før setter kan skyldes en ekstern endring. Tidligere
    // egne skrivinger gir fortsatt ansvar, men ingen automatisk overskriving
    // på disconnect/release. Bare eksplisitt reset får forsøke originalen.
    automatic_recovery_blocked: bool,
    recovery: Option<RecoveryReadback>,
    recovery_confirmation_source: Option<RecoveryConfirmationSource>,
    recovery_attempt: u64,
    current_recovery_attempt: Option<u64>,
    /// Denne økten har skrevet bilder av en programvareeffekt siden siste
    /// beholdte valg (se `Backend::settle_after_animation`).
    animated: bool,
    read_error: Option<String>,
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
            automatic_recovery_blocked: false,
            recovery: None,
            recovery_confirmation_source: None,
            recovery_attempt: 0,
            current_recovery_attempt: None,
            animated: false,
            read_error: None,
            error: None,
            next_txn: 1,
            last_ms: None,
        })
    }

    /// Utgangspunktet kortet går tilbake til (det lagrede varige valget).
    pub fn baseline_request(&self) -> &Request {
        &self.initial
    }

    /// Lagre det beholdte valget som nytt varig utgangspunkt. Bare uten
    /// ventende prøve, sperre eller annen eier, og bare når en fersk avlesning
    /// stemmer med valget brukeren beholdt. Etterpå er økten ren: lukkes appen,
    /// skrives ingenting og ingen økt står uavklart.
    pub fn persist<B: Backend>(&mut self, backend: &mut B, owner: u64, now_ms: u64) -> Result<Outcome, Error> {
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        self.tick(backend, now_ms)?;
        if self.owner.is_some_and(|held| held != owner) {
            return Err(Error::NotOwner);
        }
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self.blocked() || self.automatic_recovery_blocked {
            return Err(Error::RecoveryRequired);
        }
        self.observe(backend)?;
        let readback = request_readback(&self.confirmed, &self.observed);
        if !matched(readback) {
            return Err(Error::Readback(readback));
        }
        if self.confirmed == self.initial && !self.dirty {
            return Ok(Outcome { txn: None, status: Status::Unchanged, reason: Reason::Keep, readback });
        }
        let next = Baseline::user_confirmed_static(self.baseline.binding().clone(), self.observed.clone())
            .map_err(|message| Error::Backend { operation: "varig utgangspunkt", message })?;
        let book = Book::new(next.clone())?;
        if book.initial != self.confirmed {
            // Avlesningen må gi nøyaktig valget som lagres (svart med 100 % er «Av»).
            if !(self.confirmed == Request::Off {} && book.initial == (Request::Static { color: [0, 0, 0], brightness: Brightness::try_from(100).expect("100") })) {
                return Err(Error::UnsupportedBaseline);
            }
        }
        backend
            .adopt_baseline(self.baseline.binding(), &next)
            .map_err(|message| Error::Backend { operation: "varig utgangspunkt", message })?;
        self.baseline = next;
        self.initial = self.confirmed.clone();
        self.dirty = false;
        self.may_have_written = false;
        self.owner = None;
        self.recovery = None;
        self.recovery_confirmation_source = None;
        self.current_recovery_attempt = None;
        self.error = None;
        Ok(Outcome { txn: None, status: Status::Kept, reason: Reason::Keep, readback })
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
            recovery_attempt: self.current_recovery_attempt,
            read_error: self.read_error.clone(),
            error: self.error.clone(),
        }
    }

    fn blocked(&self) -> bool {
        self.error.is_some()
            || self.read_error.is_some()
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
                self.read_error = None;
                Ok(())
            }
            Err(message) => {
                self.observed = Observed::default();
                if !self.dirty {
                    self.read_error = Some(format!("Fersk RGB-lesing feilet: {message}"));
                } else if self.recovery_confirmation_source.take().is_some() {
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

    /// Eksplisitt ny feltlesing for en ren økt. Endrer verken lys eller lease.
    pub fn read_clean<B: Backend>(&mut self, backend: &mut B) -> Result<(), Error> {
        if self.dirty || self.pending.is_some() || self.owner.is_some() {
            return Err(Error::RecoveryRequired);
        }
        self.observe(backend)?;
        let readback = request_readback(&self.confirmed, &self.observed);
        if !matched(readback) {
            return Err(Error::Readback(readback));
        }
        Ok(())
    }

    /// Ren økt der en fersk avlesning viser noe annet enn det lagrede
    /// utgangspunktet, uten at denne hjelperen har skrevet (for eksempel kort
    /// som går tilbake til fabrikklyset etter kaldstart). Ikke en sperre:
    /// [`Book::restore_drift`] gjenoppretter med journal.
    pub fn drift(&self) -> bool {
        !self.dirty
            && self.pending.is_none()
            && self.owner.is_none()
            && request_readback(&self.confirmed, &self.observed) == RecoveryReadback::Mismatch
    }

    /// Hva «hold» skriver på nytt nå: ventende prøve, ellers det bekreftede.
    pub fn target(&self) -> &Request {
        self.pending.as_ref().map_or(&self.confirmed, |pending| &pending.requested)
    }

    /// Eksplisitt «Gjenopprett utgangspunktet» i en ren økt med avvik. Kortet
    /// har selv endret seg, så det er ingen egen skriving å rulle tilbake; men
    /// gjenopprettingen er en skriving og journalføres som vanlig før første
    /// setter. Etterpå venter den på brukerens visuelle bekreftelse (ett klikk,
    /// ikke passord), som enhver gjenoppretting med lesbare felt.
    pub fn restore_drift<B: Backend>(&mut self, backend: &mut B, owner: u64, now_ms: u64) -> Result<Outcome, Error> {
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        self.tick(backend, now_ms)?;
        if self.dirty || self.pending.is_some() || self.owner.is_some() {
            return Err(Error::RecoveryRequired);
        }
        if self.blocked() || self.automatic_recovery_blocked {
            return Err(Error::RecoveryRequired);
        }
        let next = self.next_txn.checked_add(1).ok_or(Error::Overflow)?;
        self.observe(backend)?;
        let readback = request_readback(&self.initial, &self.observed);
        if matched(readback) && self.confirmed == self.initial {
            return Ok(Outcome { txn: None, status: Status::Unchanged, reason: Reason::Reset, readback });
        }
        let journal = Journal {
            version: 1,
            baseline: self.baseline.clone(),
            before: self.initial.clone(),
            before_observed: self.observed.clone(),
            requested: self.initial.clone(),
            owner,
            txn: self.next_txn,
        };
        backend.mark_dirty(&journal).map_err(|message| Error::Backend { operation: "journalføring", message })?;
        self.next_txn = next;
        self.dirty = true;
        self.owner = Some(owner);
        Ok(self.recover(backend, self.initial.clone(), None, Reason::Reset))
    }

    /// Én «hold»-skriving av `target` (keepalive for kort som selv går tilbake
    /// fra «Av»). Tre tilfeller, alle uten ny prøve:
    /// - ventende prøve av `target`: samme journal som prøven dekker skrivingen;
    /// - beholdt `target` som ikke er utgangspunktet: som et effektbilde;
    /// - ren økt der `target` er det lagrede utgangspunktet: uten journal, bare
    ///   når en fersk avlesning allerede viser utgangspunktet i alle lesbare
    ///   felt. Da kan selv et avbrudd midt i skrivingen bare etterlate
    ///   utgangspunktets egne verdier; det er ingen endring å gjenopprette, og
    ///   et krasj gir ingen uavklart økt. Avvik stopper «hold» uten skriving.
    pub fn refresh<B: Backend>(&mut self, backend: &mut B, owner: u64, target: &Request) -> Result<(), Error> {
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        if !matches!(target, Request::Static { .. }) {
            return Err(Error::UnsupportedBaseline);
        }
        if self.blocked() || self.automatic_recovery_blocked || self.recovery.is_some() {
            return Err(Error::RecoveryRequired);
        }
        if let Some(pending) = self.pending.clone() {
            if pending.owner != owner || &pending.requested != target || !self.may_have_written {
                return Err(Error::Busy);
            }
            let result = backend.animation_frame(self.baseline.binding(), target, &self.observed, &self.baseline);
            return self.after_refresh(result, target);
        }
        if self.dirty {
            if self.owner != Some(owner) || &self.confirmed != target {
                return Err(Error::Busy);
            }
            return self.animation_frame(backend, owner, target.clone());
        }
        if self.owner.is_some() || &self.initial != target || &self.confirmed != target {
            return Err(Error::Busy);
        }
        self.observe(backend)?;
        let before = request_readback(target, &self.observed);
        if !matched(before) {
            return Err(Error::Readback(before));
        }
        match backend.refresh_baseline(self.baseline.binding(), target, &self.observed, &self.baseline) {
            Ok(observed) => {
                self.observed = observed;
                let readback = request_readback(target, &self.observed);
                if matched(readback) { Ok(()) } else { Err(Error::Readback(readback)) }
            }
            Err(failure) => {
                // Bare utgangspunktets egne verdier kan ha blitt skrevet; økten
                // forblir ren, men lyset er ikke lest etterpå.
                self.observed = Observed::default();
                Err(Error::Backend { operation: "hold", message: failure.message })
            }
        }
    }

    fn after_refresh(&mut self, result: Result<Observed, WriteError>, target: &Request) -> Result<(), Error> {
        let failure = match result {
            Ok(observed) => {
                self.observed = observed;
                let readback = request_readback(target, &self.observed);
                if matched(readback) {
                    return Ok(());
                }
                Error::Readback(readback)
            }
            Err(failure) => {
                self.automatic_recovery_blocked = !failure.write_attempted;
                self.observed = Observed::default();
                Error::Backend { operation: "hold", message: failure.message }
            }
        };
        self.recovery = Some(RecoveryReadback::Unknown);
        self.recovery_confirmation_source = None;
        self.error = Some(failure.to_string());
        Err(failure)
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
        self.apply_inner(backend, owner, request, now_ms, timeout_ms, false, None)
    }

    /// En effekt trenger en vanlig journalført prøve og Behold også når
    /// startfargen er lik forrige bekreftede valg. Dermed finnes skriveansvar
    /// og en varig original før senere animasjonsbilder avviker fra startfargen.
    pub fn apply_for_effect<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        request: Request,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Result<Outcome, Error> {
        if !matches!(request, Request::Static { .. }) {
            return Err(Error::Backend {
                operation: "animasjon",
                message: "En programvareanimasjon trenger en statisk startfarge.".into(),
            });
        }
        self.apply_inner(backend, owner, request, now_ms, timeout_ms, true, None)
    }

    /// Bytter trygt fra en pågående, beholdt programvareeffekt til et nytt valg
    /// (statisk farge, av eller ny effekt) for samme eier. Kontrollerer at kortet
    /// fortsatt viser siste kjente animasjonsramme før en ny journalført prøve
    /// startes; opprinnelig baseline beholdes alltid urørt.
    pub fn apply_after_animation<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        expected_frame: Request,
        request: Request,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Result<Outcome, Error> {
        if !matches!(expected_frame, Request::Static { .. }) {
            return Err(Error::Backend {
                operation: "animasjon",
                message: "En aktiv programvareanimasjon må ha en statisk siste ramme.".into(),
            });
        }
        self.apply_inner(
            backend,
            owner,
            request,
            now_ms,
            timeout_ms,
            true,
            Some(expected_frame),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_inner<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        request: Request,
        now_ms: u64,
        timeout_ms: u64,
        force_transaction: bool,
        animation_frame: Option<Request>,
    ) -> Result<Outcome, Error> {
        self.tick(backend, now_ms)?;
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        // En ny prøve er ikke lenger en kjørende effekt; bilder setter det igjen.
        self.animated = false;
        if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(Error::InvalidTimeout);
        }
        let deadline_ms = now_ms.checked_add(timeout_ms).ok_or(Error::Overflow)?;
        let next = self.next_txn.checked_add(1).ok_or(Error::Overflow)?;
        if self.blocked() || self.automatic_recovery_blocked {
            return Err(Error::RecoveryRequired);
        }
        if self.pending.is_some() || self.owner.is_some_and(|held| held != owner) {
            return Err(Error::Busy);
        }
        if animation_frame.is_some()
            && (self.owner != Some(owner) || !self.dirty || !self.may_have_written)
        {
            return Err(Error::RecoveryRequired);
        }
        self.observe(backend)?;
        let before_request = animation_frame.clone().unwrap_or_else(|| self.confirmed.clone());
        let before_match = request_readback(&before_request, &self.observed);
        if !matched(before_match) {
            if animation_frame.is_some() {
                self.automatic_recovery_blocked = true;
                self.recovery = Some(RecoveryReadback::Unknown);
                self.recovery_confirmation_source = None;
                self.error = Some("Lyset ble endret utenfor appen mens effekten kjørte.".into());
            }
            return Err(Error::Readback(before_match));
        }
        if request == before_request && !force_transaction {
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
            before: before_request,
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
        if let Err(message) = backend.mark_dirty(&journal) {
            if backend.journal_present() == Some(true) {
                self.dirty = true;
                self.automatic_recovery_blocked = true;
                self.current_recovery_attempt = None;
                self.recovery = Some(RecoveryReadback::Unknown);
                self.recovery_confirmation_source = None;
                self.error = Some(format!("RGB-journalen er uavklart: {message}"));
            }
            return Err(Error::Backend {
                operation: "journalføring",
                message,
            });
        }
        self.dirty = true;
        self.owner = Some(owner);
        self.next_txn = next;
        self.pending = Some(pending.clone());
        self.recovery = None;
        self.recovery_confirmation_source = None;
        self.current_recovery_attempt = None;
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
                    if animation_frame.is_some() {
                        self.automatic_recovery_blocked = true;
                    }
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

    /// Kalleren må først behandle monoton tid, lease og autorisasjon. Bare
    /// samme eier av en BEHOLDT statisk transaksjon kan sende bilder. Et bilde
    /// endrer avlesningen, aldri det bekreftede startvalget eller journalen.
    /// Ved feil stanser kalleren animasjonen og kan kalle release én gang:
    /// mulig delvis skriving får original-recovery, ren avvisning får ingen
    /// automatisk overskriving. Journal og ansvar beholdes for eksplisitt reset.
    pub fn animation_frame<B: Backend>(
        &mut self,
        backend: &mut B,
        owner: u64,
        request: Request,
    ) -> Result<(), Error> {
        if owner == 0 {
            return Err(Error::InvalidOwner);
        }
        if self.owner != Some(owner) {
            return Err(Error::NotOwner);
        }
        if !self.dirty
            || !self.may_have_written
            || self.pending.is_some()
            || self.recovery.is_some()
            || self.blocked()
            || self.automatic_recovery_blocked
        {
            return Err(Error::RecoveryRequired);
        }
        if !matches!(self.confirmed, Request::Static { .. })
            || !matches!(request, Request::Static { .. })
        {
            return Err(Error::Backend {
                operation: "animasjon",
                message: "Animasjonsbilder krever et beholdt statisk startvalg og statiske farger."
                    .into(),
            });
        }
        // Ikke erstatt expected_before med en ny selvstendig read: det ville
        // godtatt ekstern drift mellom bildene. Transporten sammenligner mot
        // siste egne bilde rett før første setter på samme åpne enhet.
        let failure = match backend.animation_frame(
            self.baseline.binding(),
            &request,
            &self.observed,
            &self.baseline,
        ) {
            Ok(observed) => {
                self.observed = observed;
                let readback = request_readback(&request, &self.observed);
                if matched(readback) {
                    self.animated = true;
                    return Ok(());
                }
                Error::Readback(readback)
            }
            Err(failure) => {
                self.animated = false;
                self.automatic_recovery_blocked = !failure.write_attempted;
                self.observed = Observed::default();
                Error::Backend {
                    operation: "animasjon",
                    message: failure.message,
                }
            }
        };
        self.recovery = Some(RecoveryReadback::Unknown);
        self.recovery_confirmation_source = None;
        self.error = Some(failure.to_string());
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
        if !self.may_have_written || self.automatic_recovery_blocked {
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
            if original && backend.journal_present() == Some(false) {
                self.dirty = false;
                self.may_have_written = false;
                self.owner = None;
            }
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
        self.current_recovery_attempt = self.recovery_attempt.checked_add(1);
        if let Some(attempt) = self.current_recovery_attempt {
            self.recovery_attempt = attempt;
        }
        self.automatic_recovery_blocked = false;
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
                let animation_ended = self.animated
                    && baseline_target
                    && readback == RecoveryReadback::ReadableFieldsMatch
                    && matches!(reason, Reason::Disconnect | Reason::Release)
                    && self.error.is_none();
                if animation_ended {
                    match backend.settle_after_animation(self.baseline.binding()) {
                        Ok(()) => {
                            self.dirty = false;
                            self.may_have_written = false;
                            self.owner = None;
                        }
                        Err(message) => {
                            self.error = Some(format!("Effekten er stoppet, men journalen kunne ikke ryddes: {message}"));
                        }
                    }
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
        self.animated = false;
        // Avsluttet effekt med samsvarende felt: ren økt, ingen ventende bekreftelse.
        let settled = !self.dirty && baseline_target && matched(readback) && self.error.is_none();
        self.recovery = if settled && readback != RecoveryReadback::Verified { None } else { Some(readback) };
        let status = match readback {
            _ if settled => Status::BaselineRestored,
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
        adopted: Vec<Baseline>,
        settled: usize,
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
        fn animation_frame(
            &mut self,
            expected: &Binding,
            request: &Request,
            before: &Observed,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.calls.push("frame");
            self.apply(expected, request, before, baseline)
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
        fn adopt_baseline(&mut self, _: &Binding, next: &Baseline) -> Result<(), String> {
            self.calls.push("adopt");
            self.adopted.push(next.clone());
            self.journal = None;
            Ok(())
        }
        fn settle_after_animation(&mut self, _: &Binding) -> Result<(), String> {
            self.calls.push("settle");
            self.settled += 1;
            self.journal = None;
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
    fn effekt_lik_originalen_krever_prove_og_behold_for_bilder() {
        let (mut book, mut hw) = setup(false);
        let initial = book.snapshot().confirmed;
        assert_eq!(
            book.apply(&mut hw, 7, initial.clone(), 0, 10_000)
                .unwrap()
                .status,
            Status::Unchanged
        );
        assert!(hw.journal.is_none() && hw.writes.is_empty());
        let pending = book
            .apply_for_effect(&mut hw, 7, initial.clone(), 1, 10_000)
            .unwrap();
        assert_eq!(pending.status, Status::Pending);
        assert!(book.animation_frame(&mut hw, 7, color([8, 9, 10])).is_err());
        assert_eq!(hw.writes.len(), 1);
        book.keep(&mut hw, 7, pending.txn.unwrap(), 2).unwrap();
        assert!(book.snapshot().dirty && book.snapshot().may_have_written);
        let journal = hw.journal.clone();
        let reads = hw.calls.iter().filter(|call| **call == "read").count();
        for frame in [color([8, 9, 10]), color([18, 19, 20])] {
            book.animation_frame(&mut hw, 7, frame.clone()).unwrap();
            assert_eq!(book.snapshot().observed, observation(&frame, false));
            assert_eq!(book.snapshot().confirmed, initial);
            assert!(book.snapshot().pending.is_none());
            assert_eq!(hw.journal, journal);
        }
        assert_eq!(hw.calls.iter().filter(|call| **call == "dirty").count(), 1);
        assert_eq!(
            hw.calls.iter().filter(|call| **call == "read").count(),
            reads
        );
        // Fra 1.7.0: effekten stoppes ved frigivelse, utgangspunktet skrives med
        // den faste sekvensen og leses likt, og økten avsluttes uten en ny
        // visuell bekreftelse (se `Backend::settle_after_animation`).
        let restored = book.release(&mut hw).unwrap();
        assert_eq!(restored.status, Status::BaselineRestored);
        assert_eq!(hw.writes.last(), Some(&initial));
        assert_eq!(hw.settled, 1);
        assert!(!book.snapshot().dirty && !book.snapshot().may_have_written);
        assert!(hw.journal.is_none());
        assert_eq!(book.snapshot().observed.mode, None);
    }

    #[test]
    fn animasjonsbilder_krever_egen_beholdt_statisk_transaksjon() {
        let (mut book, mut hw) = setup(false);
        assert_eq!(
            book.animation_frame(&mut hw, 0, color([8, 9, 10])),
            Err(Error::InvalidOwner)
        );
        assert_eq!(
            book.animation_frame(&mut hw, 7, color([8, 9, 10])),
            Err(Error::NotOwner)
        );
        assert!(
            book.apply_for_effect(&mut hw, 7, Request::Off {}, 0, 10_000)
                .is_err()
        );
        assert!(hw.calls.is_empty());
        let pending = book
            .apply_for_effect(&mut hw, 7, color([8, 9, 10]), 0, 10_000)
            .unwrap();
        assert_eq!(
            book.animation_frame(&mut hw, 8, color([18, 19, 20])),
            Err(Error::NotOwner)
        );
        assert_eq!(
            book.animation_frame(&mut hw, 7, color([18, 19, 20])),
            Err(Error::RecoveryRequired)
        );
        book.keep(&mut hw, 7, pending.txn.unwrap(), 1).unwrap();
        assert!(book.animation_frame(&mut hw, 7, Request::Off {}).is_err());
        assert!(!hw.calls.contains(&"frame"));
        book.restore_baseline(&mut hw).unwrap();
        assert_eq!(
            book.animation_frame(&mut hw, 7, color([18, 19, 20])),
            Err(Error::RecoveryRequired)
        );

        let (mut book, mut hw) = setup(false);
        let pending = book.apply(&mut hw, 7, Request::Off {}, 0, 10_000).unwrap();
        book.keep(&mut hw, 7, pending.txn.unwrap(), 1).unwrap();
        assert!(
            book.animation_frame(&mut hw, 7, color([18, 19, 20]))
                .is_err()
        );
        assert!(!hw.calls.contains(&"frame"));
    }

    #[test]
    fn ekstern_drift_mellom_bilder_stanser_ogsa_automatisk_recovery() {
        let (mut book, mut hw) = setup(false);
        let pending = book
            .apply_for_effect(&mut hw, 7, color([8, 9, 10]), 0, 10_000)
            .unwrap();
        book.keep(&mut hw, 7, pending.txn.unwrap(), 1).unwrap();
        let journal = hw.journal.clone();
        hw.observed.color = Some([40, 41, 42]);
        assert!(
            book.animation_frame(&mut hw, 7, color([18, 19, 20]))
                .is_err()
        );
        assert_eq!(hw.writes.len(), 1);
        assert!(book.snapshot().dirty && book.snapshot().may_have_written);
        assert!(book.snapshot().error.is_some());
        assert_eq!(hw.journal, journal);
        assert!(
            book.animation_frame(&mut hw, 7, color([28, 29, 30]))
                .is_err()
        );
        assert!(book.tick(&mut hw, 20_000).unwrap().is_none());
        assert!(book.release(&mut hw).is_none());
        assert!(book.disconnect(&mut hw, 7).is_none());
        assert!(book.release(&mut hw).is_none());
        assert_eq!(hw.writes.len(), 1);
        assert_eq!(hw.observed.color, Some([40, 41, 42]));
        assert!(book.snapshot().dirty && book.snapshot().may_have_written);
        assert_eq!(
            book.restore_baseline(&mut hw).unwrap().status,
            Status::RecoveryNeedsConfirmation
        );
        assert_eq!(hw.writes.len(), 2);
        book.ack_recovery(&mut hw, 8, 20_001).unwrap();
        assert!(!book.snapshot().dirty && hw.journal.is_none());
    }

    #[test]
    fn delvis_eller_feillest_bilde_gir_ett_originalforsok_ved_release() {
        for partial in [false, true] {
            let (mut book, mut hw) = setup(false);
            let pending = book
                .apply_for_effect(&mut hw, 7, color([8, 9, 10]), 0, 10_000)
                .unwrap();
            book.keep(&mut hw, 7, pending.txn.unwrap(), 1).unwrap();
            if partial {
                hw.fail_write_numbers = vec![2];
            } else {
                hw.mismatch_write_numbers = vec![2];
            }
            assert!(
                book.animation_frame(&mut hw, 7, color([18, 19, 20]))
                    .is_err()
            );
            assert_eq!(hw.writes.len(), 2, "frame gjør ingen egen recovery");
            assert_eq!(book.snapshot().confirmed, color([8, 9, 10]));
            assert!(
                book.animation_frame(&mut hw, 7, color([28, 29, 30]))
                    .is_err()
            );
            assert_eq!(
                book.release(&mut hw).unwrap().status,
                Status::RecoveryNeedsConfirmation
            );
            assert_eq!(hw.writes.last(), Some(&color([1, 2, 3])));
            assert_eq!(hw.writes.len(), 3);
            assert!(book.release(&mut hw).is_none());
            assert!(book.disconnect(&mut hw, 7).is_none());
            assert!(book.tick(&mut hw, 20_000).unwrap().is_none());
            assert_eq!(hw.writes.len(), 3);
            assert!(book.snapshot().dirty && book.snapshot().may_have_written);
            assert_eq!(book.snapshot().observed.mode, None);
        }
    }

    #[test]
    fn ubeholdt_effekt_lik_originalen_utloper_som_vanlig_prove() {
        let (mut book, mut hw) = setup(false);
        let initial = book.snapshot().confirmed;
        book.apply_for_effect(&mut hw, 7, initial.clone(), 0, 10_000)
            .unwrap();
        assert_eq!(
            book.tick(&mut hw, 10_000).unwrap().unwrap().status,
            Status::RecoveryNeedsConfirmation
        );
        assert_eq!(hw.writes, [initial.clone(), initial]);
        assert!(book.snapshot().pending.is_none());
        assert!(book.snapshot().dirty && book.snapshot().recovery_needs_confirmation);
        assert!(book.animation_frame(&mut hw, 7, color([8, 9, 10])).is_err());
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

    #[test]
    fn bytte_fra_aktiv_animasjon_sjekker_siste_ramme_og_beholder_original() {
        let (mut book, mut hw) = setup(false);
        let txn = book
            .apply_for_effect(&mut hw, 7, color([20, 40, 60]), 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        let frame = color([90, 10, 120]);
        book.animation_frame(&mut hw, 7, frame.clone()).unwrap();
        assert_eq!(book.snapshot().confirmed, color([20, 40, 60]));
        assert_eq!(book.snapshot().observed.color, Some([90, 10, 120]));

        // Nytt valg fra aktiv animasjon verifiserer siste animasjonsramme og
        // krever ny Behold; ved Rull tilbake ruller den til siste ramme.
        let next_txn = book
            .apply_after_animation(&mut hw, 7, frame.clone(), Request::Off {}, 2, 10_000)
            .unwrap()
            .txn
            .unwrap();
        assert_eq!(
            book.snapshot().pending.as_ref().unwrap().before,
            frame
        );
        let rev = book.revert(&mut hw, 7, next_txn, 3).unwrap();
        assert_eq!(rev.status, Status::RecoveryNeedsConfirmation);
        assert_eq!(book.snapshot().confirmed, frame);
        book.ack_recovery(&mut hw, 7, 4).unwrap();

        // Nytt bytte + Behold, deretter release gjenoppretter fortsatt originalen.
        let off_txn = book
            .apply(&mut hw, 7, Request::Off {}, 5, 10_000)
            .unwrap()
            .txn
            .unwrap();
        book.keep(&mut hw, 7, off_txn, 6).unwrap();
        let rel = book.release(&mut hw).unwrap();
        assert_eq!(rel.reason, Reason::Release);
        assert_eq!(hw.writes.last(), Some(&color([1, 2, 3])));
    }

    #[test]
    fn ekstern_drift_ved_bytte_fra_animasjon_skriver_ikke_og_sperrer_auto_recovery() {
        let (mut book, mut hw) = setup(false);
        let txn = book
            .apply_for_effect(&mut hw, 7, color([20, 40, 60]), 0, 10_000)
            .unwrap()
            .txn
            .unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        let frame = color([90, 10, 120]);
        book.animation_frame(&mut hw, 7, frame.clone()).unwrap();
        // Ekstern endring før brukeren trykker Bruk på en ny modus.
        hw.observed.color = Some([200, 100, 50]);
        let writes_before = hw.writes.len();
        assert_eq!(
            book.apply_after_animation(&mut hw, 7, frame, Request::Off {}, 2, 10_000),
            Err(Error::Readback(RecoveryReadback::Mismatch))
        );
        assert_eq!(hw.writes.len(), writes_before);
        assert!(book.snapshot().error.is_some());
        assert!(book.release(&mut hw).is_none());
        assert_eq!(hw.writes.len(), writes_before);
    }

    #[test]
    fn varig_utgangspunkt_gjor_okten_ren_og_lukking_skriver_ingenting() {
        let (mut book, mut hw) = setup(false);
        let blue = color([0, 0, 255]);
        let txn = book.apply(&mut hw, 7, blue.clone(), 0, 10_000).unwrap().txn.unwrap();
        // Ikke mens prøven venter.
        assert_eq!(book.persist(&mut hw, 7, 1), Err(Error::Busy));
        book.keep(&mut hw, 7, txn, 2).unwrap();
        assert!(book.snapshot().dirty);
        // En annen klient kan ikke lagre eierens valg.
        assert_eq!(book.persist(&mut hw, 8, 3), Err(Error::NotOwner));
        let outcome = book.persist(&mut hw, 7, 3).unwrap();
        assert_eq!(outcome.status, Status::Kept);
        assert_eq!(hw.adopted.len(), 1);
        assert_eq!(book.baseline_request(), &blue);
        let snapshot = book.snapshot();
        assert!(!snapshot.dirty && snapshot.owner.is_none() && hw.journal.is_none());
        // Appen lukkes: ingen skriving og ingen uavklart økt.
        let writes = hw.writes.len();
        assert_eq!(book.disconnect(&mut hw, 7), None);
        assert_eq!(hw.writes.len(), writes);
        // En senere prøve som ikke beholdes, går tilbake til det varige valget.
        let txn = book.apply(&mut hw, 7, color([255, 0, 0]), 4, 10_000).unwrap().txn.unwrap();
        book.revert(&mut hw, 7, txn, 5).unwrap();
        assert_eq!(hw.writes.last(), Some(&blue));
        assert_eq!(book.snapshot().confirmed, blue);
    }

    #[test]
    fn varig_utgangspunkt_krever_samsvarende_avlesning() {
        let (mut book, mut hw) = setup(false);
        let txn = book.apply(&mut hw, 7, color([0, 0, 255]), 0, 10_000).unwrap().txn.unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        hw.observed.color = Some([1, 1, 1]);
        assert!(matches!(book.persist(&mut hw, 7, 2), Err(Error::Readback(_))));
        assert!(hw.adopted.is_empty());
        assert!(book.snapshot().dirty, "journalen står til valget kan bekreftes");
    }

    #[test]
    fn av_som_svart_med_null_lysstyrke_kan_lagres_som_varig() {
        let (mut book, mut hw) = setup(false);
        let off = Request::Static { color: [0, 0, 0], brightness: Brightness::try_from(0).unwrap() };
        let txn = book.apply(&mut hw, 7, off.clone(), 0, 10_000).unwrap().txn.unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        book.persist(&mut hw, 7, 2).unwrap();
        assert_eq!(book.baseline_request(), &off);
        assert_eq!(hw.adopted[0].observed().brightness_raw, Some(0));
    }

    #[test]
    fn effekt_som_stoppes_ved_lukking_gir_ingen_uavklart_okt() {
        let (mut book, mut hw) = setup(false);
        let start = book.snapshot().confirmed;
        let txn = book.apply_for_effect(&mut hw, 7, start.clone(), 0, 10_000).unwrap().txn.unwrap();
        book.keep(&mut hw, 7, txn, 1).unwrap();
        book.animation_frame(&mut hw, 7, color([9, 9, 9])).unwrap();
        // Appen lukkes: utgangspunktet skrives og avleses likt, økten er avsluttet.
        let outcome = book.disconnect(&mut hw, 7).unwrap();
        assert_eq!(outcome.status, Status::BaselineRestored);
        assert_eq!(hw.writes.last(), Some(&start));
        assert_eq!(hw.settled, 1);
        let snapshot = book.snapshot();
        assert!(!snapshot.dirty && !snapshot.recovery_needs_confirmation && snapshot.recovery.is_none());
    }

    #[test]
    fn provd_men_ikke_beholdt_valg_krever_fortsatt_visuell_bekreftelse() {
        let (mut book, mut hw) = setup(false);
        let txn = book.apply(&mut hw, 7, color([0, 0, 255]), 0, 10_000).unwrap().txn.unwrap();
        // Frakobling under en prøve (ingen effekt): som før.
        let outcome = book.disconnect(&mut hw, 7).unwrap();
        assert_eq!(outcome.status, Status::RecoveryNeedsConfirmation);
        assert_eq!(hw.settled, 0);
        assert!(book.snapshot().dirty);
        let _ = txn;
    }
}
