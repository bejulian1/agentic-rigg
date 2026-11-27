//! Separat RGB-runtime uten NVML, filstier, klokkeoppslag eller enhetsåpning.
//!
//! Serveren må laste root-eid RGB-tillatelse og grunnopptak gjennom den private
//! journaladapteren. Den må kontrollere at tillatelsen er uendret før hver RPC
//! og hver periodiske `tick`; bortfall/endring krever `revoke` og stopp. Peer-UID
//! og et unikt, positivt forbindelsesnummer kommer fra serveren, aldri klienten.
//! Tid er monoton CLOCK_BOOTTIME i millisekunder, inkludert hviletid.
//!
//! `rgb_status` bruker bare cache og fornyer ingenting. Serveren må derfor
//! kalle `tick` minst hvert sekund også når ingen forespørsler kommer. Backend
//! kontrollerer faktisk kortbinding på nytt før lesing/skriving. En separat
//! RGB-lease gjelder også etter Behold; eksisterende GPU-status er irrelevant.

use rigg_core::proto::{ErrorCode, OpResult, ProtoError};
use rigg_core::rgb::{Baseline, Brightness, Mode, RecoveryReadback, Request};
use rigg_core::rgb_auth::{self, Authorization};
use rigg_core::rgb_effect::Effect;
use rigg_core::rgb_txn::{Backend, Book, Error, Outcome, Status};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub const PROTOCOL: u32 = rgb_auth::PROTOCOL;
pub const LEASE_MS: u64 = 15_000;
const FRAME_INTERVAL_MS: u64 = 250;

struct Animation {
    effect: Effect,
    owner: u64,
    color: [u8; 3],
    brightness: Brightness,
    txn: u64,
    started_ms: Option<u64>,
    last_frame_ms: u64,
    last_request: Request,
}

/// «Hold»: samme faste valg skrevet på nytt hvert `HOLD_INTERVAL_MS` mens
/// forbindelsen som startet det, er åpen (for kort som selv går tilbake fra
/// «Av»). Ingen effekt og ingen ny prøve: målet er det kortet allerede skal
/// vise (ventende prøve, beholdt valg eller det lagrede utgangspunktet). Stopper
/// av seg selv når målet endres, ved feil, frakobling og utløpt lease.
struct Keepalive {
    owner: u64,
    target: Request,
    last_write_ms: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ObservationSource {
    StoredBaseline,
    OperationReadback,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Holding {
    owner: u64,
    deadline_ms: u64,
    lease_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityArgs {
    gpu: String,
    uuid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyArgs {
    gpu: String,
    uuid: String,
    request: Request,
    confirm_timeout_s: u64,
    effect: Option<Effect>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionArgs {
    gpu: String,
    uuid: String,
    txn: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryAckArgs {
    gpu: String,
    uuid: String,
    confirmed_static: bool,
    /// Echo fra dialogens hjelperstatus, bare en compare-and-swap-betingelse.
    /// Dette blir aldri et nytt skrive- eller gjenopprettingsmål.
    expected: Request,
    recovery_id: String,
}

enum Action {
    Status,
    Apply(ApplyArgs),
    Confirm(TransactionArgs),
    Revert(TransactionArgs),
    Reset(IdentityArgs),
    Read(IdentityArgs),
    Release(IdentityArgs),
    Heartbeat(IdentityArgs),
    AckRecovery(RecoveryAckArgs),
    KeepObserved(RecoveryAckArgs),
    /// Lagre det beholdte faste valget (eller «Av») som varig utgangspunkt.
    Persist(IdentityArgs),
    /// Start «hold» av det kortet skal vise nå (se [`Keepalive`]).
    Hold(IdentityArgs),
}

fn parse<T: DeserializeOwned>(args: &Value) -> Result<T, ProtoError> {
    serde_json::from_value(args.clone()).map_err(|error| {
        ProtoError::msg(
            ErrorCode::InvalidArgument,
            format!("Ugyldige RGB-argumenter: {error}"),
        )
    })
}

impl Action {
    fn parse(op: &str, args: &Value) -> Result<Self, ProtoError> {
        Ok(match op {
            "rgb_status" => {
                parse::<EmptyArgs>(args)?;
                Self::Status
            }
            "rgb_apply" => {
                let parsed: ApplyArgs = parse(args)?;
                if !(10..=60).contains(&parsed.confirm_timeout_s) {
                    return Err(ProtoError::msg(
                        ErrorCode::InvalidArgument,
                        "RGB-fristen må være et heltall fra 10 til 60 sekunder.",
                    ));
                }
                if args.get("effect").is_some()
                    && (parsed.effect.is_none()
                        || !matches!(parsed.request, Request::Static { .. }))
                {
                    return Err(ProtoError::msg(
                        ErrorCode::InvalidArgument,
                        "En programvareeffekt krever statisk startfarge og lysstyrke.",
                    ));
                }
                Self::Apply(parsed)
            }
            "rgb_confirm" | "rgb_revert" => {
                let args: TransactionArgs = parse(args)?;
                if args.txn == 0 {
                    return Err(ProtoError::msg(
                        ErrorCode::InvalidArgument,
                        "RGB-transaksjonen må være større enn null.",
                    ));
                }
                if op == "rgb_confirm" {
                    Self::Confirm(args)
                } else {
                    Self::Revert(args)
                }
            }
            "rgb_reset" => Self::Reset(parse(args)?),
            "rgb_read" => Self::Read(parse(args)?),
            "rgb_release" => Self::Release(parse(args)?),
            "rgb_heartbeat" => Self::Heartbeat(parse(args)?),
            "rgb_persist" => Self::Persist(parse(args)?),
            "rgb_hold" => Self::Hold(parse(args)?),
            "rgb_ack_recovery" | "rgb_keep_observed" => {
                let args: RecoveryAckArgs = parse(args)?;
                if !args.confirmed_static {
                    return Err(ProtoError::msg(
                        ErrorCode::InvalidArgument,
                        "Gjenopprettet statisk lys må være uttrykkelig bekreftet av brukeren.",
                    ));
                }
                if op == "rgb_keep_observed" { Self::KeepObserved(args) } else { Self::AckRecovery(args) }
            }
            _ => return Err(ProtoError::new(ErrorCode::UnknownOp)),
        })
    }

    fn identity(&self) -> Option<(&str, &str)> {
        match self {
            Self::Status => None,
            Self::Apply(args) => Some((&args.gpu, &args.uuid)),
            Self::AckRecovery(args) | Self::KeepObserved(args) => Some((&args.gpu, &args.uuid)),
            Self::Confirm(args) | Self::Revert(args) => Some((&args.gpu, &args.uuid)),
            Self::Reset(args)
            | Self::Read(args)
            | Self::Release(args)
            | Self::Heartbeat(args)
            | Self::Persist(args)
            | Self::Hold(args) => {
                Some((&args.gpu, &args.uuid))
            }
        }
    }
}

pub struct Runtime<B: Backend> {
    authorization: Authorization,
    book: Book,
    backend: B,
    holding: Option<Holding>,
    last_ms: Option<u64>,
    last_outcome: Option<Outcome>,
    observation_source: ObservationSource,
    clock_error: bool,
    revoked: bool,
    static_baseline: bool,
    animation: Option<Animation>,
    keepalive: Option<Keepalive>,
    effect_error: Option<String>,
    instance_id: String,
    review: Option<(&'static str, String)>,
}

impl<B: Backend> Runtime<B> {
    /// `authorization` og `baseline` må komme fra
    /// serverens betrodde diskadapter, ikke fra klientens JSON.
    pub fn new(
        authorization: Authorization,
        baseline: Baseline,
        backend: B,
    ) -> Result<Self, String> {
        if !rgb_auth::compatible_build(authorization.build()) {
            return Err("RGB-tillatelsens hjelperbygg er ikke kompatibelt.".into());
        }
        if !authorization.allows(authorization.uid(), baseline.binding()) {
            return Err("RGB-tillatelsen er av eller gjelder et annet grunnopptak.".into());
        }
        let static_baseline = baseline.expected_mode() == Mode::Static;
        let book = Book::new(baseline).map_err(|error| error.to_string())?;
        let instance_id = crate::rgb_store::new_recovery_id()?;
        let mut runtime = Self {
            authorization,
            book,
            backend,
            holding: None,
            last_ms: None,
            last_outcome: None,
            observation_source: ObservationSource::StoredBaseline,
            clock_error: false,
            revoked: false,
            static_baseline,
            animation: None,
            keepalive: None,
            effect_error: None,
            instance_id,
            review: None,
        };
        // The persistent backend always reports journal presence. A new or
        // restarted worker cannot accept writes before its initial read.
        if runtime.backend.journal_present().is_some() || runtime.book.snapshot().baseline_source == rigg_core::rgb::BaselineSource::AwaitingVisual {
            runtime.inspect_startup()?;
        }
        Ok(runtime)
    }

    fn inspect_startup(&mut self) -> Result<(), String> {
        let result = self.read_clean();
        let kind = if self.book.snapshot().baseline_source == rigg_core::rgb::BaselineSource::AwaitingVisual {
            "first"
        } else if self.backend.journal_present() == Some(true) {
            "journal"
        } else if result.is_err() {
            "drift"
        } else {
            self.review = None;
            return Ok(());
        };
        self.review = Some((kind, crate::rgb_store::new_recovery_id()?));
        Ok(())
    }

    fn observed_request(&self) -> Option<Request> {
        let observed = self.book.snapshot().observed;
        Some(Request::Static { color: observed.color?, brightness: observed.brightness_percent()? })
    }

    pub fn authorized_uid(&self) -> u32 {
        self.authorization.uid()
    }

    fn recovery_id(&self) -> Option<String> {
        let state = self.book.snapshot();
        let attempt = state
            .recovery_attempt
            .filter(|_| state.recovery_needs_confirmation && state.error.is_none())?;
        Some(format!("{}-{attempt}", self.instance_id))
    }

    /// Ren cache: ingen backend-kall, klokkeoppdatering eller leasefornyelse.
    pub fn status(&self) -> Value {
        json!({
            "helper": {
                "name": "agentic-rigg-rgb-helper",
                "domain": "palit_rgb",
                "version": rigg_core::version::VERSION,
                "build": rigg_core::version::build(),
                "protocol": PROTOCOL, "strictRequests": true
            },
            "identity": self.authorization.binding(),
            "authorized": !self.revoked,
            "instanceId": self.instance_id,
            "holding": self.holding,
            "state": self.book.snapshot(),
            "recoveryId": self.recovery_id(),
            "review": self.review.as_ref().map(|(kind,id)| json!({"kind":kind,"id":id,"expected":self.observed_request()})),
            "observationSource": self.observation_source,
            "lastOutcome": self.last_outcome,
            "softwareEffects": [Effect::Breathing, Effect::Rainbow, Effect::Cycle],
            // Fra 1.7.0: varig utgangspunkt (det kortet går tilbake til når
            // appen lukkes, og fabrikklyset fra før første varige valg),
            // «hold» av det kortet skal vise, og avvik i en ren økt.
            "persist": true,
            "hold": true,
            "keepalive": self.keepalive.is_some(),
            "drift": self.book.drift(),
            "baselineRequest": (self.book.snapshot().baseline_source != rigg_core::rgb::BaselineSource::AwaitingVisual).then(|| self.book.baseline_request()),
            "factory": self.backend.factory_baseline().and_then(|b| Book::new(b).ok()).map(|b| b.baseline_request().clone()),
            "effect": self.animation.as_ref().map(|animation| animation.effect),
            "runtimeError": if self.clock_error {
                Some("Den monotone RGB-klokken gikk bakover. Nye endringer er sperret.")
            } else {
                self.effect_error.as_deref()
            }
        })
    }

    /// UID/forbindelse kommer fra socketen. Autorisasjonsfilen må fortsatt
    /// være uendret ved innkalling. Bare `rgb_heartbeat`, vellykket apply og
    /// Behold fornyer eksisterende RGB-lease; ingen GPU-operasjoner aksepteres.
    pub fn handle(
        &mut self,
        peer_uid: u32,
        conn: u64,
        op: &str,
        args: &Value,
        now_ms: u64,
    ) -> OpResult {
        if peer_uid != self.authorized_uid() {
            return Err(ProtoError::new(ErrorCode::PermissionDenied));
        }
        if conn == 0 {
            return Err(ProtoError::msg(
                ErrorCode::InvalidArgument,
                "RGB-forbindelsen må ha en servervalgt identitet større enn null.",
            ));
        }
        let action = Action::parse(op, args)?;
        if matches!(action, Action::Status) {
            return Ok(self.status());
        }
        if self.revoked {
            return Err(ProtoError::msg(
                ErrorCode::PermissionDenied,
                "RGB-tillatelsen er trukket tilbake.",
            ));
        }
        if let Some((pci, uuid)) = action.identity() {
            let binding = self.authorization.binding();
            if pci != binding.pci || uuid != binding.uuid {
                return Err(ProtoError::msg(
                    ErrorCode::UnknownGpu,
                    "RGB-forespørselen gjelder ikke kortet i tillatelsen.",
                ));
            }
        }
        // Avvis tall som ikke kan representere en hel lease før nye kall.
        let lease_deadline = now_ms.checked_add(LEASE_MS).ok_or_else(|| {
            ProtoError::msg(
                ErrorCode::InvalidArgument,
                "RGB-klokken er utenfor gyldig område.",
            )
        })?;
        self.tick(now_ms)?;
        if let Some((kind, id)) = self.review.clone() {
            match action {
                Action::Read(_) => self.inspect_startup().map_err(|e| ProtoError::msg(ErrorCode::Unavailable, e))?,
                Action::KeepObserved(args) => {
                    if args.recovery_id != id || self.observed_request().as_ref() != Some(&args.expected) {
                        return Err(self.transaction_error(Error::RecoveryRequired));
                    }
                    let shown = self.book.snapshot().observed;
                    self.inspect_startup().map_err(|e| ProtoError::msg(ErrorCode::Unavailable, e))?;
                    let fresh = self.book.snapshot().observed;
                    if fresh != shown || self.observed_request().as_ref() != Some(&args.expected) {
                        return Err(self.transaction_error(Error::RecoveryRequired));
                    }
                    let saved = self.backend.accept_observation(&fresh)
                        .map_err(|e| ProtoError::msg(ErrorCode::Unavailable, e))?;
                    self.book = Book::new(saved).map_err(|e| self.transaction_error(e))?;
                    self.book.accept_observation(args.expected, fresh);
                    self.review = None;
                }
                Action::Reset(_) if kind != "first" => {
                    let outcome = if self.backend.journal_present() == Some(true) {
                        self.book.restore_journal(&mut self.backend)
                    } else {
                        self.book.restore_drift(&mut self.backend, conn, now_ms)
                            .map_err(|e| self.transaction_error(e))?
                    };
                    self.review = None;
                    self.record_outcome(&outcome);
                    self.sync_holding(lease_deadline, true);
                    return Ok(json!({"outcome":outcome,"status":self.status()}));
                }
                _ => return Err(self.transaction_error(Error::RecoveryRequired)),
            }
            return Ok(json!({"outcome":null,"status":self.status()}));
        }
        // En annen forbindelses «hold» eier lyset som en effekt gjør.
        if self.keepalive.as_ref().is_some_and(|hold| hold.owner != conn)
            && !matches!(action, Action::Read(_) | Action::Heartbeat(_))
        {
            return Err(self.transaction_error(Error::Busy));
        }
        let before = self.book.snapshot();
        let clean_read = !before.dirty && matches!(action, Action::Read(_) | Action::Reset(_));
        let (result, renew) = match action {
            Action::KeepObserved(_) => return Err(self.transaction_error(Error::RecoveryRequired)),
            Action::Status => unreachable!("status ble håndtert uten sideeffekter"),
            Action::Apply(args) => {
                if self
                    .animation
                    .as_ref()
                    .is_some_and(|animation| animation.started_ms.is_none() || animation.owner != conn)
                {
                    return Err(self.transaction_error(Error::Busy));
                }
                if args.effect.is_some() && !self.static_baseline {
                    return Err(self.transaction_error(Error::UnsupportedBaseline));
                }
                let (request, seed) = if let Some(effect) = args.effect {
                    let Request::Static { color, brightness } = args.request else {
                        unreachable!("effektargumentet krever statisk forespørsel")
                    };
                    (effect.frame(color, brightness, 0), Some((effect, color, brightness)))
                } else {
                    (args.request, None)
                };
                let result = if let Some(active) = self.animation.take() {
                    let result = self.book.apply_after_animation(
                        &mut self.backend,
                        conn,
                        active.last_request.clone(),
                        request.clone(),
                        now_ms,
                        args.confirm_timeout_s * 1000,
                    );
                    match &result {
                        // Avvist før kortet ble lest: effekten fortsetter uendret.
                        Err(Error::InvalidOwner
                        | Error::InvalidTimeout
                        | Error::Overflow
                        | Error::ClockReversed
                        | Error::Busy) => self.animation = Some(active),
                        // Kortet kunne ikke leses før byttet. Samme vei som en
                        // feilet effektramme: stopp effekten og gi kortet
                        // tilbake til det lagrede utgangspunktet.
                        Err(error @ Error::Backend { operation: "lesing", .. }) => {
                            self.effect_error = Some(format!("Lyseffekten er stoppet: {error}"));
                            self.observation_source = ObservationSource::Unknown;
                            self.release();
                        }
                        _ => {}
                    }
                    result
                } else if seed.is_some() {
                    self.book.apply_for_effect(
                        &mut self.backend,
                        conn,
                        request.clone(),
                        now_ms,
                        args.confirm_timeout_s * 1000,
                    )
                } else {
                    self.book.apply(
                        &mut self.backend,
                        conn,
                        request.clone(),
                        now_ms,
                        args.confirm_timeout_s * 1000,
                    )
                };
                if let Ok(outcome) = &result {
                    if let Some((effect, color, brightness)) = seed {
                        self.animation = outcome.txn.map(|txn| Animation {
                            effect,
                            owner: conn,
                            color,
                            brightness,
                            txn,
                            started_ms: None,
                            last_frame_ms: now_ms,
                            last_request: request,
                        });
                    }
                    self.effect_error = None;
                }
                (result.map(Some), true)
            }
            Action::Confirm(args) => {
                let result = self.book.keep(&mut self.backend, conn, args.txn, now_ms);
                if result
                    .as_ref()
                    .is_ok_and(|outcome| outcome.status == Status::Kept)
                    && let Some(animation) = &mut self.animation
                    && animation.owner == conn
                    && animation.txn == args.txn
                    && animation.started_ms.is_none()
                {
                    animation.started_ms = Some(now_ms);
                    animation.last_frame_ms = now_ms;
                }
                (result.map(Some), true)
            }
            Action::Revert(args) => {
                if self.animation_active() {
                    return Err(self.transaction_error(Error::Busy));
                }
                if self
                    .animation
                    .as_ref()
                    .is_some_and(|animation| animation.owner == conn && animation.txn == args.txn)
                {
                    self.animation = None;
                }
                (
                    self.book
                        .revert(&mut self.backend, conn, args.txn, now_ms)
                        .map(Some),
                    false,
                )
            }
            Action::Heartbeat(_) => {
                self.require_owner(conn, false)?;
                (Ok(None), true)
            }
            Action::Release(_) => {
                self.require_owner(conn, false)?;
                if self.book.snapshot().owner.is_none() {
                    // Bare «hold» av utgangspunktet: ingen egen endring å rulle tilbake.
                    self.keepalive = None;
                    (Ok(None), false)
                } else {
                    (Ok(self.release()), false)
                }
            }
            Action::Reset(_) => {
                self.require_owner(conn, true)?;
                self.animation = None;
                self.keepalive = None;
                if self.book.snapshot().dirty {
                    (Ok(self.book.restore_baseline(&mut self.backend)), false)
                } else {
                    // Ren økt: les. Viser kortet noe annet enn utgangspunktet
                    // (kortet har selv endret seg), gjenopprett med journal.
                    let differs = self.book.snapshot().confirmed != *self.book.baseline_request();
                    match self.read_clean() {
                        Ok(_) if differs => (self.book.restore_drift(&mut self.backend, conn, now_ms).map(Some), true),
                        Err(Error::Readback(RecoveryReadback::Mismatch)) => {
                            (self.book.restore_drift(&mut self.backend, conn, now_ms).map(Some), true)
                        }
                        other => (other, false),
                    }
                }
            }
            Action::Hold(_) => {
                if self.animation.is_some() {
                    return Err(self.transaction_error(Error::Busy));
                }
                let target = self.book.target().clone();
                match self.book.refresh(&mut self.backend, conn, &target) {
                    Ok(()) => {
                        self.keepalive = Some(Keepalive { owner: conn, target, last_write_ms: now_ms });
                        self.effect_error = None;
                        self.observation_source = ObservationSource::OperationReadback;
                        (Ok(None), true)
                    }
                    Err(error) => {
                        self.keepalive = None;
                        // En mulig delvis skriving under egen journal får én
                        // avgrenset gjenoppretting, som et feilet effektbilde.
                        if self.book.snapshot().error.is_some() {
                            self.release();
                        }
                        (Err(error), false)
                    }
                }
            }
            Action::Read(_) => (self.read_clean(), false),
            Action::Persist(_) => {
                // Effekter kjører bare mens appen eier lyset og kan ikke lagres.
                if self.animation.is_some() {
                    return Err(ProtoError::msg(
                        ErrorCode::NotSupported,
                        "En lyseffekt kan ikke lagres som varig utgangspunkt. Velg fast farge eller av.",
                    )
                    .details(json!({"status": self.status()})));
                }
                let result = self.book.persist(&mut self.backend, conn, now_ms);
                if result.is_ok() {
                    self.static_baseline = true;
                    self.observation_source = ObservationSource::OperationReadback;
                }
                (result.map(Some), false)
            }
            Action::AckRecovery(args) => {
                self.require_owner(conn, true)?;
                if self.animation.is_some() {
                    return Err(self.transaction_error(Error::Busy));
                }
                if self.recovery_id().as_deref() != Some(&args.recovery_id)
                    || args.expected != self.book.snapshot().confirmed
                {
                    return Err(ProtoError::msg(ErrorCode::WriteLocked,
                        "RGB-bekreftelsen gjelder et tidligere forsøk. Se på kortet og åpne en ny bekreftelse.")
                        .details(json!({"status": self.status()})));
                }
                (
                    self.book
                        .ack_recovery(&mut self.backend, conn, now_ms)
                        .map(Some),
                    true,
                )
            }
        };
        // En feil kan ha etablert eget recoveryansvar. Gi en ny eier en
        // begrenset lease, men forny aldri en eksisterende lease ved feil.
        self.reconcile_keepalive();
        self.sync_holding(lease_deadline, renew && result.is_ok());
        self.reconcile_animation();
        match result {
            Ok(outcome) => {
                if let Some(outcome) = &outcome {
                    self.record_outcome(outcome);
                }
                Ok(json!({"outcome": outcome, "status": self.status()}))
            }
            Err(error) => {
                if !clean_read && self.book.snapshot().observed != before.observed {
                    self.observation_source = ObservationSource::Unknown;
                }
                Err(self.transaction_error(error))
            }
        }
    }

    fn read_clean(&mut self) -> Result<Option<Outcome>, Error> {
        if self.book.snapshot().dirty {
            return Err(Error::RecoveryRequired);
        }
        let result = self.book.read_clean(&mut self.backend);
        let observed = self.book.snapshot().observed;
        self.observation_source = if observed.color.is_some() && observed.brightness_raw.is_some() {
            ObservationSource::OperationReadback
        } else {
            ObservationSource::Unknown
        };
        result.map(|()| None)
    }

    fn record_outcome(&mut self, outcome: &Outcome) {
        self.last_outcome = Some(outcome.clone());
        let observed = self.book.snapshot().observed;
        self.observation_source = if outcome.readback != RecoveryReadback::Unknown
            && observed.color.is_some()
            && observed.brightness_raw.is_some()
        {
            ObservationSource::OperationReadback
        } else {
            ObservationSource::Unknown
        };
    }

    /// Forbindelsen som eier lyset: en transaksjon, ellers «hold».
    fn holder(&self) -> Option<u64> {
        self.book.snapshot().owner.or(self.keepalive.as_ref().map(|hold| hold.owner))
    }

    fn require_owner(&self, conn: u64, allow_unowned: bool) -> Result<(), ProtoError> {
        match self.holder() {
            Some(owner) if owner == conn => Ok(()),
            None if allow_unowned => Ok(()),
            _ => Err(ProtoError::new(ErrorCode::NotOwner)),
        }
    }

    /// «Hold» gjelder bare målet det ble startet for, og bare uten feil eller
    /// en annen eier. Ny prøve, tilbakerulling og gjenoppretting stopper det.
    fn reconcile_keepalive(&mut self) {
        let state = self.book.snapshot();
        if self.keepalive.as_ref().is_some_and(|hold| {
            self.animation.is_some()
                || state.error.is_some()
                || (state.recovery.is_some() && (state.dirty || state.recovery_needs_confirmation))
                || state.owner.is_some_and(|owner| owner != hold.owner)
                || self.book.target() != &hold.target
        }) {
            self.keepalive = None;
        }
    }

    fn sync_holding(&mut self, deadline_ms: u64, renew: bool) {
        let Some(owner) = self.holder() else {
            self.holding = None;
            return;
        };
        if renew || self.holding.is_none_or(|held| held.owner != owner) {
            self.holding = Some(Holding {
                owner,
                deadline_ms,
                lease_ms: LEASE_MS,
            });
        }
    }

    fn reconcile_animation(&mut self) {
        let state = self.book.snapshot();
        if self.animation.as_ref().is_some_and(|animation| {
            state.owner != Some(animation.owner)
                || state.error.is_some()
                || state.recovery_needs_confirmation
                || (animation.started_ms.is_none()
                    && state
                        .pending
                        .as_ref()
                        .is_none_or(|pending| pending.txn != animation.txn))
        }) {
            self.animation = None;
        }
    }

    pub fn animation_active(&self) -> bool {
        self.animation
            .as_ref()
            .is_some_and(|animation| animation.started_ms.is_some())
    }

    /// Service kaller dette først etter auth, stopp, frister, innkomne
    /// forespørsler og lukkede klienter. Ingen innhenting av tapte rammer.
    /// Ved `true` må `frame_completed` få ny BOOTTIME etter at I/O er ferdig.
    pub fn animate(&mut self, now_ms: u64) -> Result<bool, ProtoError> {
        self.tick(now_ms)?;
        let Some(animation) = &mut self.animation else {
            return Ok(self.hold(now_ms));
        };
        let Some(started_ms) = animation.started_ms else {
            return Ok(false);
        };
        if now_ms.saturating_sub(animation.last_frame_ms) < FRAME_INTERVAL_MS {
            return Ok(false);
        }
        animation.last_frame_ms = now_ms;
        let request =
            animation
                .effect
                .frame(animation.color, animation.brightness, now_ms - started_ms);
        if request == animation.last_request {
            return Ok(false);
        }
        let owner = animation.owner;
        match self
            .book
            .animation_frame(&mut self.backend, owner, request.clone())
        {
            Ok(()) => {
                if let Some(animation) = &mut self.animation {
                    animation.last_request = request;
                }
                self.observation_source = ObservationSource::OperationReadback;
            }
            Err(error) => {
                // Book skiller avvisning før setter fra mulig delvis frame.
                // release får bare gjøre én avgrenset, autorisert recovery.
                self.animation = None;
                self.effect_error = Some(format!("Lyseffekten er stoppet: {error}"));
                self.observation_source = ObservationSource::Unknown;
                self.release();
            }
        }
        Ok(true)
    }

    /// Én «hold»-skriving når den er forfallen. `true` når kortet ble lest
    /// eller skrevet. Ved feil stopper «hold»; en mulig delvis skriving under
    /// egen journal får én avgrenset gjenoppretting.
    fn hold(&mut self, now_ms: u64) -> bool {
        self.reconcile_keepalive();
        let Some(hold) = &self.keepalive else { return false };
        if now_ms.saturating_sub(hold.last_write_ms) < rigg_core::rgb_effect::HOLD_INTERVAL_MS {
            return false;
        }
        let (owner, target) = (hold.owner, hold.target.clone());
        match self.book.refresh(&mut self.backend, owner, &target) {
            Ok(()) => {
                if let Some(hold) = &mut self.keepalive {
                    hold.last_write_ms = now_ms;
                }
                self.observation_source = ObservationSource::OperationReadback;
            }
            Err(error) => {
                self.keepalive = None;
                self.effect_error = Some(format!("«Hold» er stoppet: {error}"));
                self.observation_source = ObservationSource::Unknown;
                if self.book.snapshot().error.is_some() {
                    self.release();
                }
            }
        }
        true
    }

    /// Minst 250 ms hvile etter fysisk retur, også når én frame tok lengre
    /// tid enn normalperioden. Denne oppdateringen fornyer aldri eierens lease.
    pub fn frame_completed(&mut self, now_ms: u64) -> Result<(), ProtoError> {
        self.tick(now_ms)?;
        if let Some(animation) = &mut self.animation {
            animation.last_frame_ms = now_ms;
        }
        Ok(())
    }

    /// Kalles periodisk selv uten klienttrafikk. Utløpt lease går foran
    /// pending-fristen: ved samtidige frister gjenopprettes originalen én gang.
    /// Ukjent recovery forblir sperret; timeren skal ikke prøve igjen.
    pub fn tick(&mut self, now_ms: u64) -> Result<Vec<Outcome>, ProtoError> {
        if self.revoked {
            return Ok(Vec::new());
        }
        if self.clock_error {
            return Err(self.transaction_error(Error::ClockReversed));
        }
        if self.last_ms.is_some_and(|last| now_ms < last) {
            self.clock_error = true;
            self.release();
            return Err(self.transaction_error(Error::ClockReversed));
        }
        self.last_ms = Some(now_ms);
        if self.holding.is_some_and(|held| now_ms >= held.deadline_ms) {
            return Ok(self.release().into_iter().collect());
        }
        if self
            .book
            .snapshot()
            .pending
            .is_some_and(|pending| now_ms >= pending.deadline_ms)
        {
            self.animation = None;
        }
        match self.book.tick(&mut self.backend, now_ms) {
            Ok(outcome) => {
                self.reconcile_keepalive();
                if self.holder().is_none() {
                    self.holding = None;
                }
                if let Some(outcome) = &outcome {
                    self.record_outcome(outcome);
                }
                self.reconcile_animation();
                Ok(outcome.into_iter().collect())
            }
            Err(error) => Err(self.transaction_error(error)),
        }
    }

    pub fn disconnect(&mut self, conn: u64) -> Option<Outcome> {
        if self
            .animation
            .as_ref()
            .is_some_and(|animation| animation.owner == conn)
        {
            self.animation = None;
        }
        if self.keepalive.as_ref().is_some_and(|hold| hold.owner == conn) {
            self.keepalive = None;
        }
        let outcome = self.book.disconnect(&mut self.backend, conn);
        if self.holding.is_some_and(|held| held.owner == conn) {
            self.holding = None;
        }
        if let Some(outcome) = &outcome {
            self.record_outcome(outcome);
        }
        outcome
    }

    /// Lifecycle-frigivelse ved nedstenging/utløp. Dette er aldri den
    /// eksplisitte reset-stien og gjør ingen setter etter ren preflight-feil.
    pub fn release(&mut self) -> Option<Outcome> {
        self.animation = None;
        self.keepalive = None;
        self.holding = None;
        let outcome = self.book.release(&mut self.backend);
        if let Some(outcome) = &outcome {
            self.record_outcome(outcome);
        }
        outcome
    }

    /// Brukes én gang når root-tillatelsen forsvinner eller endres. En ny
    /// tillatelse krever ny runtime; den gamle kan ikke aktiveres igjen.
    pub fn revoke(&mut self) -> Option<Outcome> {
        self.revoked = true;
        self.release()
    }

    fn transaction_error(&self, error: Error) -> ProtoError {
        let code = match error {
            Error::InvalidOwner | Error::InvalidTimeout | Error::Overflow => {
                ErrorCode::InvalidArgument
            }
            Error::Busy => ErrorCode::Busy,
            Error::NotOwner => ErrorCode::NotOwner,
            Error::UnknownTransaction => ErrorCode::TxnMismatch,
            Error::RecoveryRequired | Error::ClockReversed => ErrorCode::WriteLocked,
            Error::UnsupportedBaseline => ErrorCode::NotSupported,
            Error::Readback(_) | Error::Backend { .. } => ErrorCode::Unavailable,
        };
        ProtoError::msg(code, error.to_string()).details(json!({"status": self.status()}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::rgb::{AdapterIdentity, Binding, Brightness, Observed, PciIdentity, Signature};
    use rigg_core::rgb_txn::{Journal, Reason, Status, WriteError};

    fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-rgb-runtime-fixture".into(),
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
            signature: Signature::Palit100,
        }
    }

    fn color(rgb: [u8; 3]) -> Request {
        Request::Static {
            color: rgb,
            brightness: Brightness::try_from(80).unwrap(),
        }
    }

    fn observed(request: &Request) -> Observed {
        let (color, brightness) = match request {
            Request::Static { color, brightness } => (*color, brightness.get()),
            Request::Off {} => ([0, 0, 0], 100),
        };
        Observed {
            color: Some(color),
            brightness_raw: Some(brightness),
            mode: None,
        }
    }

    #[derive(Default)]
    struct Fake {
        current: Observed,
        reads: usize,
        journal: Option<Journal>,
        writes: Vec<Request>,
        reject_before_write: bool,
        partial_apply: bool,
        fail_recovery: bool,
        acknowledgments: usize,
        fail_ack: bool,
        fail_read: bool,
        adopted: Vec<Baseline>,
    }

    impl Backend for Fake {
        fn read(&mut self, binding: &Binding) -> Result<Observed, String> {
            assert_eq!(binding, &self::binding());
            self.reads += 1;
            if self.fail_read {
                return Err("Midlertidig lesefeil".into());
            }
            Ok(self.current.clone())
        }
        fn mark_dirty(&mut self, journal: &Journal) -> Result<(), String> {
            journal.validate(&binding()).unwrap();
            self.journal = Some(journal.clone());
            Ok(())
        }
        fn apply(
            &mut self,
            binding: &Binding,
            request: &Request,
            before: &Observed,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            assert_eq!(binding, baseline.binding());
            assert_eq!(&self.current, before);
            assert!(self.journal.is_some());
            if self.reject_before_write {
                return Err(WriteError {
                    message: "avvist før setter".into(),
                    write_attempted: false,
                });
            }
            self.writes.push(request.clone());
            self.current = observed(request);
            if self.partial_apply {
                return Err(WriteError {
                    message: "mulig delvis skriving".into(),
                    write_attempted: true,
                });
            }
            Ok(self.current.clone())
        }
        fn recover(
            &mut self,
            binding: &Binding,
            request: &Request,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            assert_eq!(binding, baseline.binding());
            assert!(self.journal.is_some());
            self.writes.push(request.clone());
            self.current = observed(request);
            if self.fail_recovery {
                return Err(WriteError {
                    message: "recovery feilet".into(),
                    write_attempted: true,
                });
            }
            Ok(self.current.clone())
        }
        fn animation_frame(
            &mut self,
            binding: &Binding,
            request: &Request,
            before: &Observed,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            if before != &self.current {
                return Err(WriteError {
                    message: "ekstern drift".into(),
                    write_attempted: false,
                });
            }
            self.apply(binding, request, before, baseline)
        }
        fn refresh_baseline(
            &mut self,
            binding: &Binding,
            request: &Request,
            before: &Observed,
            baseline: &Baseline,
        ) -> Result<Observed, WriteError> {
            assert_eq!(binding, baseline.binding());
            assert!(self.journal.is_none(), "«hold» av utgangspunktet skrives uten journal");
            assert_eq!(&self.current, before);
            self.writes.push(request.clone());
            self.current = observed(request);
            Ok(self.current.clone())
        }
        fn clear_dirty(&mut self, _: &Binding) -> Result<(), String> {
            panic!("Palit-fixturen beviser ikke modus og kan aldri rydde dirty")
        }
        fn adopt_baseline(&mut self, _: &Binding, next: &Baseline) -> Result<(), String> {
            assert_eq!(next.observed(), &self.current, "bare nøyaktig det som er lest");
            self.adopted.push(next.clone());
            self.journal = None;
            Ok(())
        }
        fn settle_after_animation(&mut self, _: &Binding) -> Result<(), String> {
            self.journal = None;
            Ok(())
        }
        fn ack_recovery(
            &mut self,
            binding: &Binding,
            target: &Request,
            actual: &Observed,
            baseline: &Baseline,
        ) -> Result<(), String> {
            assert_eq!(binding, baseline.binding());
            if self.fail_ack || actual != &self.current {
                return Err("simulert feil i visuell kvittering".into());
            }
            self.acknowledgments += 1;
            if target == &Book::new(baseline.clone()).unwrap().snapshot().confirmed {
                self.journal = None;
            }
            Ok(())
        }
    }

    fn setup() -> Runtime<Fake> {
        let current = observed(&color([1, 2, 3]));
        let baseline = Baseline::user_confirmed_static(binding(), current.clone()).unwrap();
        let authorization =
            Authorization::new(1000, binding(), rigg_core::version::build()).unwrap();
        Runtime::new(
            authorization,
            baseline,
            Fake {
                current,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn identity() -> Value {
        json!({"gpu": binding().pci, "uuid": binding().uuid})
    }

    fn apply_args(request: Request, timeout: u64) -> Value {
        let mut args = identity();
        args["request"] = serde_json::to_value(request).unwrap();
        args["confirmTimeoutS"] = json!(timeout);
        args
    }

    fn txn_args(txn: u64) -> Value {
        let mut args = identity();
        args["txn"] = json!(txn);
        args
    }

    fn ack_args(runtime: &Runtime<Fake>) -> Value {
        ack_args_for(runtime, color([1, 2, 3]))
    }

    fn ack_args_for(runtime: &Runtime<Fake>, expected: Request) -> Value {
        let mut args = identity();
        args["confirmedStatic"] = json!(true);
        args["expected"] = serde_json::to_value(expected).unwrap();
        args["recoveryId"] = json!(
            runtime
                .recovery_id()
                .unwrap_or_else(|| "gammelt-forsøk".into())
        );
        args
    }

    fn apply(runtime: &mut Runtime<Fake>, now: u64, request: Request, timeout: u64) -> u64 {
        runtime
            .handle(1000, 7, "rgb_apply", &apply_args(request, timeout), now)
            .unwrap()["outcome"]["txn"]
            .as_u64()
            .unwrap()
    }

    #[test]
    fn ack_rpc_krever_aktiv_bekreftelse_egen_eier_og_fersk_lesing() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 10, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_revert", &txn_args(txn), 20)
            .unwrap();
        let reads = runtime.backend.reads;
        for args in [
            identity(),
            {
                let mut a = ack_args(&runtime);
                a["confirmedStatic"] = json!(false);
                a
            },
            {
                let mut a = ack_args(&runtime);
                a["target"] = json!([1, 2, 3]);
                a
            },
        ] {
            assert_eq!(
                runtime
                    .handle(1000, 7, "rgb_ack_recovery", &args, 30)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidArgument
            );
        }
        assert_eq!(
            runtime
                .handle(1001, 7, "rgb_ack_recovery", &ack_args(&runtime), 30)
                .unwrap_err()
                .code,
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            runtime
                .handle(1000, 8, "rgb_ack_recovery", &ack_args(&runtime), 30)
                .unwrap_err()
                .code,
            ErrorCode::NotOwner
        );
        assert_eq!(runtime.backend.reads, reads);
        let result = runtime
            .handle(1000, 7, "rgb_ack_recovery", &ack_args(&runtime), 40)
            .unwrap();
        assert_eq!(result["outcome"]["status"], "user_confirmed");
        assert_eq!(result["outcome"]["readback"], "readable_fields_match");
        assert_eq!(
            result["status"]["state"]["recoveryConfirmationSource"],
            "userVisual"
        );
        assert!(result["status"]["state"]["observed"]["mode"].is_null());
        assert!(result["status"]["holding"].is_null());
        assert_eq!(runtime.backend.reads, reads + 1);
        assert_eq!(runtime.backend.acknowledgments, 1);
        assert_eq!(runtime.backend.writes.len(), 2);
    }

    #[test]
    fn samme_lysmal_krever_ny_bekreftelse_etter_leaseutlop_eller_arbeiderbytte() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 10, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_revert", &txn_args(txn), 20)
            .unwrap();
        let old = ack_args(&runtime);

        for now in [10 + LEASE_MS, 11 + LEASE_MS] {
            assert_eq!(
                runtime
                    .handle(1000, 7, "rgb_ack_recovery", &old, now)
                    .unwrap_err()
                    .code,
                ErrorCode::WriteLocked
            );
        }
        assert_eq!(runtime.backend.writes.len(), 3);
        assert_eq!(runtime.backend.acknowledgments, 0);
        let new = ack_args(&runtime);
        assert_eq!(old["expected"], new["expected"]);
        assert_ne!(old["recoveryId"], new["recoveryId"]);
        runtime
            .handle(1000, 7, "rgb_ack_recovery", &new, 12 + LEASE_MS)
            .unwrap();

        let mut restarted = setup();
        let txn = apply(&mut restarted, 10, color([4, 5, 6]), 10);
        restarted
            .handle(1000, 7, "rgb_revert", &txn_args(txn), 20)
            .unwrap();
        assert_eq!(
            restarted
                .handle(1000, 7, "rgb_ack_recovery", &old, 30)
                .unwrap_err()
                .code,
            ErrorCode::WriteLocked
        );
        assert_eq!(restarted.backend.acknowledgments, 0);
    }

    #[test]
    fn ny_feltlesing_etter_midlertidig_lesefeil_i_ren_okt() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 10, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_revert", &txn_args(txn), 20)
            .unwrap();
        let ack = ack_args(&runtime);
        runtime
            .handle(1000, 7, "rgb_ack_recovery", &ack, 30)
            .unwrap();
        runtime.backend.fail_read = true;
        let error = runtime
            .handle(1000, 7, "rgb_apply", &apply_args(color([4, 5, 6]), 10), 40)
            .unwrap_err();
        let state = &error.details.unwrap()["status"]["state"];
        assert_eq!(state["dirty"], false);
        assert!(state["readError"].is_string());

        let reads = runtime.backend.reads;
        runtime
            .handle(1000, 7, "rgb_status", &json!({}), 50)
            .unwrap();
        assert_eq!(runtime.backend.reads, reads);
        runtime.backend.fail_read = false;
        let view = runtime
            .handle(1000, 7, "rgb_read", &identity(), 60)
            .unwrap();
        assert_eq!(runtime.backend.reads, reads + 1);
        assert_eq!(runtime.backend.writes.len(), 2);
        assert!(view["status"]["holding"].is_null());
        assert!(view["status"]["state"]["readError"].is_null());
        assert_eq!(
            view["status"]["state"]["recoveryConfirmationSource"],
            "userVisual"
        );
        assert_eq!(view["status"]["observationSource"], "operation_readback");
        apply(&mut runtime, 70, color([4, 5, 6]), 10);
    }

    #[test]
    fn samme_uid_kan_bekrefte_etter_klienttap_men_ikke_drift_eller_io() {
        for fault in 0..3 {
            let mut runtime = setup();
            apply(&mut runtime, 10, color([4, 5, 6]), 10);
            runtime.disconnect(7).unwrap();
            match fault {
                1 => runtime.backend.current.color = Some([9, 9, 9]),
                2 => runtime.backend.fail_ack = true,
                _ => {}
            }
            let result = runtime.handle(1000, 8, "rgb_ack_recovery", &ack_args(&runtime), 30);
            assert_eq!(result.is_ok(), fault == 0);
            assert_eq!(runtime.backend.acknowledgments, usize::from(fault == 0));
            assert_eq!(runtime.backend.writes.len(), 2);
            assert_eq!(runtime.book.snapshot().dirty, fault != 0);
        }
    }

    #[test]
    fn ikkeoriginal_ack_beholder_lease_og_fristendret_mal_krever_ny_bekreftelse() {
        let mut runtime = setup();
        let first = apply(&mut runtime, 0, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(first), 1)
            .unwrap();
        let second = apply(&mut runtime, 2, color([7, 8, 9]), 10);
        runtime
            .handle(1000, 7, "rgb_revert", &txn_args(second), 3)
            .unwrap();
        let result = runtime
            .handle(
                1000,
                7,
                "rgb_ack_recovery",
                &ack_args_for(&runtime, color([4, 5, 6])),
                4,
            )
            .unwrap();
        assert_eq!(result["status"]["state"]["dirty"], true);
        assert_eq!(result["status"]["state"]["mayHaveWritten"], true);
        assert_eq!(result["status"]["holding"]["owner"], 7);
        assert_eq!(result["status"]["holding"]["deadlineMs"], 4 + LEASE_MS);
        let third = apply(&mut runtime, 5, color([10, 11, 12]), 10);
        runtime
            .handle(1000, 7, "rgb_revert", &txn_args(third), 6)
            .unwrap();
        let acknowledgments = runtime.backend.acknowledgments;
        let error = runtime
            .handle(
                1000,
                7,
                "rgb_ack_recovery",
                &ack_args_for(&runtime, color([4, 5, 6])),
                5 + LEASE_MS,
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::WriteLocked);
        assert_eq!(runtime.backend.acknowledgments, acknowledgments);
        assert_eq!(runtime.book.snapshot().confirmed, color([1, 2, 3]));
        assert_eq!(runtime.book.snapshot().owner, None);
        let reads = runtime.backend.reads;
        let writes = runtime.backend.writes.len();
        // Gammel UI-bekreftelse avvises også når fristen ble behandlet
        // FØR denne RPC-en kom frem, uten en ny feltlesing eller setter.
        assert_eq!(
            runtime
                .handle(
                    1000,
                    8,
                    "rgb_ack_recovery",
                    &ack_args_for(&runtime, color([4, 5, 6])),
                    6 + LEASE_MS
                )
                .unwrap_err()
                .code,
            ErrorCode::WriteLocked
        );
        assert_eq!(runtime.backend.reads, reads);
        assert_eq!(runtime.backend.writes.len(), writes);
    }

    #[test]
    fn oppstart_krever_egen_aktiv_kortbundet_kompatibel_tillatelse() {
        let baseline =
            Baseline::user_confirmed_static(binding(), observed(&color([1, 2, 3]))).unwrap();
        let mut other = binding();
        other.uuid = "GPU-other-runtime-fixture".into();
        let auth = Authorization::new(1000, other, rigg_core::version::build()).unwrap();
        assert!(Runtime::new(auth, baseline.clone(), Fake::default()).is_err());
        let auth = Authorization::new(1000, binding(), rigg_core::version::build()).unwrap();
        let mut disabled = serde_json::to_value(auth).unwrap();
        disabled["enabled"] = json!(false);
        assert!(
            Runtime::new(
                serde_json::from_value(disabled).unwrap(),
                baseline.clone(),
                Fake::default()
            )
            .is_err()
        );
        for build in [
            rgb_auth::MIN_COMPATIBLE_BUILD - 1,
            rigg_core::version::build() + 1,
        ] {
            let auth = Authorization::new(1000, binding(), build).unwrap();
            assert!(Runtime::new(auth, baseline.clone(), Fake::default()).is_err());
        }
        let runtime = setup();
        assert_eq!(runtime.authorized_uid(), 1000);
        assert_eq!(runtime.status()["observationSource"], "stored_baseline");
        assert_eq!(runtime.backend.reads, 0);
        assert!(runtime.backend.writes.is_empty());
    }

    #[test]
    fn uid_kort_og_strenge_argumenter_sjekkes_foer_backend() {
        let mut runtime = setup();
        let args = apply_args(color([4, 5, 6]), 10);
        assert_eq!(
            runtime
                .handle(1001, 7, "rgb_apply", &args, 0)
                .unwrap_err()
                .code,
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            runtime
                .handle(1000, 0, "rgb_apply", &args, 0)
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
        for field in ["gpu", "uuid"] {
            let mut wrong = args.clone();
            wrong[field] = json!("annet-kort");
            assert_eq!(
                runtime
                    .handle(1000, 7, "rgb_apply", &wrong, 0)
                    .unwrap_err()
                    .code,
                ErrorCode::UnknownGpu
            );
        }
        for (field, value) in [
            ("owner", json!(7)),
            ("path", json!("/dev/i2c-8")),
            ("confirmTimeoutS", json!(9)),
            ("confirmTimeoutS", json!(61)),
            ("confirmTimeoutS", json!(10.5)),
            ("confirmTimeoutS", json!("10")),
        ] {
            let mut bad = args.clone();
            bad[field] = value;
            assert_eq!(
                runtime
                    .handle(1000, 7, "rgb_apply", &bad, 0)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidArgument
            );
        }
        let mut bad = args;
        bad["request"]["brightness"] = json!(101);
        assert!(runtime.handle(1000, 7, "rgb_apply", &bad, 0).is_err());
        assert!(
            runtime
                .handle(1000, 7, "rgb_confirm", &txn_args(0), 0)
                .is_err()
        );
        assert!(
            runtime
                .handle(1000, 7, "rgb_status", &identity(), 0)
                .is_err()
        );
        assert_eq!(runtime.backend.reads, 0);
        assert!(runtime.backend.writes.is_empty() && runtime.backend.journal.is_none());
    }

    #[test]
    fn status_er_cache_og_gpu_trafikk_fornyer_aldri_rgb_lease() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 1)
            .unwrap();
        let reads = runtime.backend.reads;
        let deadline = runtime.status()["holding"]["deadlineMs"].clone();
        for op in ["status", "apply", "heartbeat", "rgb_get"] {
            assert_eq!(
                runtime
                    .handle(1000, 7, op, &json!({}), 14_000)
                    .unwrap_err()
                    .code,
                ErrorCode::UnknownOp
            );
        }
        let status = runtime
            .handle(1000, 7, "rgb_status", &json!({}), 20_000)
            .unwrap();
        assert_eq!(status["holding"]["deadlineMs"], deadline);
        assert_eq!(status["helper"]["domain"], "palit_rgb");
        assert_eq!(status["helper"]["protocol"], PROTOCOL);
        assert_eq!(runtime.backend.reads, reads);
        assert_eq!(runtime.backend.writes.len(), 1);
        let outcomes = runtime.tick(20_000).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].status, Status::RecoveryNeedsConfirmation);
        assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
        assert!(runtime.status()["holding"].is_null());
        assert!(runtime.tick(30_000).unwrap().is_empty());
        assert_eq!(runtime.backend.writes.len(), 2);
    }

    #[test]
    fn heartbeat_krever_eier_og_kan_ikke_gjenopplive_utloept_lease() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, color([4, 5, 6]), 60);
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 1)
            .unwrap();
        assert_eq!(
            runtime
                .handle(1000, 8, "rgb_heartbeat", &identity(), 10)
                .unwrap_err()
                .code,
            ErrorCode::NotOwner
        );
        assert_eq!(runtime.status()["holding"]["deadlineMs"], 15_001);
        runtime
            .handle(1000, 7, "rgb_heartbeat", &identity(), 14_000)
            .unwrap();
        assert_eq!(runtime.status()["holding"]["deadlineMs"], 29_000);
        assert_eq!(
            runtime
                .handle(1000, 7, "rgb_heartbeat", &identity(), 29_000)
                .unwrap_err()
                .code,
            ErrorCode::NotOwner
        );
        assert_eq!(runtime.backend.writes.len(), 2);
        assert!(runtime.status()["holding"].is_null());
    }

    #[test]
    fn bare_eier_kan_beholde_reverte_resette_eller_frigjore() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, color([4, 5, 6]), 10);
        for (op, args) in [
            ("rgb_confirm", txn_args(txn)),
            ("rgb_revert", txn_args(txn)),
            ("rgb_reset", identity()),
            ("rgb_release", identity()),
        ] {
            assert_eq!(
                runtime.handle(1000, 8, op, &args, 1).unwrap_err().code,
                ErrorCode::NotOwner
            );
        }
        assert!(runtime.disconnect(8).is_none());
        let result = runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 2)
            .unwrap();
        assert_eq!(result["outcome"]["status"], "kept");
        assert_eq!(result["outcome"]["readback"], "readable_fields_match");
        assert_eq!(result["status"]["observationSource"], "operation_readback");
        assert!(result["status"]["state"]["observed"]["mode"].is_null());
        assert_eq!(runtime.backend.writes.len(), 1);
        let result = runtime
            .handle(1000, 7, "rgb_release", &identity(), 3)
            .unwrap();
        assert_eq!(result["outcome"]["status"], "recovery_needs_confirmation");
        assert_eq!(result["status"]["state"]["dirty"], true);
        assert!(runtime.release().is_none());
        assert_eq!(runtime.backend.writes.len(), 2);
    }

    #[test]
    fn heartbeat_utvider_ikke_pendingfrist_og_sen_confirm_avvises() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_heartbeat", &identity(), 9_000)
            .unwrap();
        assert_eq!(runtime.status()["state"]["pending"]["deadlineMs"], 10_000);
        assert_eq!(
            runtime
                .handle(1000, 7, "rgb_confirm", &txn_args(txn), 10_000)
                .unwrap_err()
                .code,
            ErrorCode::TxnMismatch
        );
        assert_eq!(runtime.backend.writes.len(), 2);
        assert_eq!(runtime.status()["lastOutcome"]["reason"], "timeout");
        assert!(runtime.tick(11_000).unwrap().is_empty());
        assert_eq!(runtime.backend.writes.len(), 2);
    }

    #[test]
    fn samtidige_frister_gjenoppretter_originalen_bare_en_gang() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 1)
            .unwrap();
        apply(&mut runtime, 2, Request::Off {}, 15);
        let outcomes = runtime.tick(15_002).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].reason, Reason::Release);
        assert_eq!(runtime.backend.writes.len(), 3);
        assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
        assert!(runtime.tick(16_000).unwrap().is_empty());
    }

    #[test]
    fn avvisning_foer_setter_blir_ikke_skriving_ved_lifecycle() {
        let mut runtime = setup();
        runtime.backend.reject_before_write = true;
        assert!(
            runtime
                .handle(1000, 7, "rgb_apply", &apply_args(color([4, 5, 6]), 10), 0)
                .is_err()
        );
        assert_eq!(runtime.status()["state"]["mayHaveWritten"], false);
        assert_eq!(runtime.status()["observationSource"], "unknown");
        assert_eq!(runtime.status()["holding"]["deadlineMs"], 15_000);
        assert!(runtime.tick(15_000).unwrap().is_empty());
        assert!(runtime.disconnect(7).is_none());
        assert!(runtime.release().is_none());
        assert!(runtime.revoke().is_none());
        assert!(runtime.backend.writes.is_empty());
        assert_eq!(runtime.status()["state"]["dirty"], true);
    }

    #[test]
    fn revoke_lukker_for_nye_kall_og_beholder_feilet_recovery() {
        let mut runtime = setup();
        runtime.backend.partial_apply = true;
        runtime.backend.fail_recovery = true;
        assert!(
            runtime
                .handle(1000, 7, "rgb_apply", &apply_args(color([4, 5, 6]), 10), 0)
                .is_err()
        );
        assert_eq!(runtime.backend.writes.len(), 2);
        assert_eq!(runtime.revoke().unwrap().status, Status::RecoveryFailed);
        assert_eq!(runtime.status()["observationSource"], "unknown");
        assert_eq!(runtime.backend.writes.len(), 3);
        assert!(runtime.revoke().is_none());
        assert!(runtime.tick(100_000).unwrap().is_empty());
        assert_eq!(
            runtime
                .handle(1000, 7, "rgb_reset", &identity(), 100_001)
                .unwrap_err()
                .code,
            ErrorCode::PermissionDenied
        );
        assert_eq!(runtime.backend.writes.len(), 3);
        assert_eq!(runtime.status()["authorized"], false);
        assert_eq!(runtime.status()["state"]["dirty"], true);
    }

    #[test]
    fn baklengs_klokke_frigjor_en_gang_og_laser_nye_endringer() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 100, color([4, 5, 6]), 10);
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 101)
            .unwrap();
        assert_eq!(runtime.tick(99).unwrap_err().code, ErrorCode::WriteLocked);
        assert_eq!(runtime.backend.writes.len(), 2);
        assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
        assert!(runtime.tick(102).is_err());
        assert!(
            runtime
                .handle(1000, 7, "rgb_apply", &apply_args(Request::Off {}, 10), 103)
                .is_err()
        );
        assert!(runtime.disconnect(7).is_none());
        assert_eq!(runtime.backend.writes.len(), 2);
    }

    #[test]
    fn eksplisitt_reset_er_adskilt_fra_automatisk_frigivelse() {
        let mut runtime = setup();
        runtime.backend.reject_before_write = true;
        assert!(
            runtime
                .handle(1000, 7, "rgb_apply", &apply_args(Request::Off {}, 10), 0)
                .is_err()
        );
        assert!(runtime.backend.writes.is_empty());
        let result = runtime
            .handle(1000, 7, "rgb_reset", &identity(), 1)
            .unwrap();
        assert_eq!(result["outcome"]["reason"], "reset");
        assert_eq!(result["outcome"]["status"], "recovery_needs_confirmation");
        assert_eq!(runtime.backend.writes, [color([1, 2, 3])]);
    }

    fn effect_args(effect: Effect, request: Request) -> Value {
        let mut args = apply_args(request, 10);
        args["effect"] = serde_json::to_value(effect).unwrap();
        args
    }

    fn start_effect(runtime: &mut Runtime<Fake>, effect: Effect, start: u64) {
        let result = runtime
            .handle(
                1000,
                7,
                "rgb_apply",
                &effect_args(effect, color([120, 60, 30])),
                start,
            )
            .unwrap();
        let txn = result["outcome"]["txn"].as_u64().unwrap();
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), start)
            .unwrap();
    }

    #[test]
    fn effekt_lik_baseline_ma_beholdes_og_fornyer_aldri_lease_per_frame() {
        let mut runtime = setup();
        let result = runtime
            .handle(
                1000,
                7,
                "rgb_apply",
                &effect_args(Effect::Breathing, color([1, 2, 3])),
                0,
            )
            .unwrap();
        assert_eq!(result["outcome"]["status"], "pending");
        assert_eq!(result["status"]["effect"], "breathing");
        assert_eq!(runtime.backend.writes.len(), 1);
        assert!(!runtime.animate(5000).unwrap());
        assert_eq!(runtime.backend.writes.len(), 1);
        let txn = result["outcome"]["txn"].as_u64().unwrap();
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 5000)
            .unwrap();
        let lease = runtime.status()["holding"]["deadlineMs"].clone();
        assert!(!runtime.animate(5249).unwrap());
        // Små kanalverdier kan runde til samme frame. Ved bunnen endres de.
        assert!(runtime.animate(7000).unwrap());
        runtime.frame_completed(7000).unwrap();
        assert_eq!(runtime.status()["holding"]["deadlineMs"], lease);
        assert_eq!(runtime.book.snapshot().confirmed, color([1, 2, 3]));
        assert_ne!(
            runtime.book.snapshot().observed,
            observed(&color([1, 2, 3]))
        );
        assert_eq!(runtime.status()["state"]["observed"]["mode"], Value::Null);
        assert!(!runtime.animate(7249).unwrap());
    }

    #[test]
    fn varig_valg_over_socket_og_aldri_under_effekt() {
        let mut runtime = setup();
        let blue = color([0, 0, 255]);
        let txn = apply(&mut runtime, 0, blue.clone(), 10);
        runtime.handle(1000, 7, "rgb_confirm", &txn_args(txn), 1).unwrap();
        // En annen tilkobling kan ikke lagre eierens valg.
        assert!(runtime.handle(1000, 8, "rgb_persist", &identity(), 2).is_err());
        let result = runtime.handle(1000, 7, "rgb_persist", &identity(), 2).unwrap();
        assert_eq!(result["status"]["persist"], true);
        assert_eq!(result["status"]["baselineRequest"], serde_json::to_value(&blue).unwrap());
        assert_eq!(result["status"]["state"]["dirty"], false);
        assert_eq!(runtime.backend.adopted.len(), 1);
        // Lukking: ingen skriving, ingen uavklart økt.
        let writes = runtime.backend.writes.len();
        runtime.disconnect(7);
        assert_eq!(runtime.backend.writes.len(), writes);
        // Under en effekt kan valget ikke lagres.
        start_effect(&mut runtime, Effect::Breathing, 10);
        let error = runtime.handle(1000, 7, "rgb_persist", &identity(), 11).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotSupported);
    }

    const HOLD: u64 = rigg_core::rgb_effect::HOLD_INTERVAL_MS;

    fn off() -> Request {
        Request::Static { color: [0, 0, 0], brightness: Brightness::try_from(0).unwrap() }
    }

    #[test]
    fn hold_gaar_fra_prove_til_behold_og_varig_uten_ny_prove() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, off(), 10);
        // Allerede under prøven (før «Behold»).
        let result = runtime.handle(1000, 7, "rgb_hold", &identity(), 1).unwrap();
        assert_eq!(result["status"]["keepalive"], true);
        assert_eq!(result["status"]["hold"], true);
        let writes = runtime.backend.writes.len();
        assert!(!runtime.animate(1000).unwrap(), "ingen skriving før intervallet");
        assert!(runtime.animate(HOLD + 10).unwrap());
        assert_eq!(runtime.backend.writes.len(), writes + 1);
        assert_eq!(runtime.backend.writes.last(), Some(&off()));
        // Behold og varig: «hold» fortsetter, uten ny prøve og uten journal.
        runtime.handle(1000, 7, "rgb_confirm", &txn_args(txn), HOLD + 20).unwrap();
        assert_eq!(runtime.status()["keepalive"], true);
        let result = runtime.handle(1000, 7, "rgb_persist", &identity(), HOLD + 30).unwrap();
        assert_eq!(result["status"]["state"]["dirty"], false);
        assert_eq!(result["status"]["keepalive"], true);
        assert_eq!(result["status"]["holding"]["owner"], 7, "riggd holder forbindelsen og leasen");
        assert!(runtime.backend.journal.is_none());
        for step in 1..=5 {
            runtime.handle(1000, 7, "rgb_heartbeat", &identity(), HOLD + 30 + step * HOLD).unwrap();
            assert!(runtime.animate(HOLD + 40 + step * HOLD).unwrap());
        }
        assert_eq!(runtime.backend.writes.len(), writes + 6);
        assert!(runtime.backend.journal.is_none(), "et krasj her etterlater ingen journal");
        assert!(runtime.status()["state"]["pending"].is_null());
        // En annen forbindelse kan ikke ta lyset mens «hold» går.
        assert_eq!(
            runtime.handle(1000, 8, "rgb_apply", &apply_args(color([9, 9, 9]), 10), 20_000).unwrap_err().code,
            ErrorCode::Busy
        );
        // Lukkes appen, stopper «hold» uten skriving og uten uavklart økt.
        let writes = runtime.backend.writes.len();
        runtime.disconnect(7);
        assert_eq!(runtime.backend.writes.len(), writes);
        let status = runtime.status();
        assert_eq!(status["keepalive"], false);
        assert!(status["holding"].is_null());
        assert_eq!(status["state"]["dirty"], false);
        // Neste oppstart av appen kan starte «hold» igjen.
        runtime.handle(1000, 9, "rgb_hold", &identity(), 30_000).unwrap();
        assert_eq!(runtime.status()["keepalive"], true);
        // «Slutt å holde av» stopper bare «hold».
        runtime.handle(1000, 9, "rgb_release", &identity(), 30_001).unwrap();
        assert_eq!(runtime.status()["keepalive"], false);
        assert_eq!(runtime.backend.writes.len(), writes + 1);
    }

    #[test]
    fn hold_stopper_ved_tilbakerulling_og_lease() {
        let mut runtime = setup();
        let txn = apply(&mut runtime, 0, off(), 10);
        runtime.handle(1000, 7, "rgb_hold", &identity(), 1).unwrap();
        runtime.handle(1000, 7, "rgb_revert", &txn_args(txn), 2).unwrap();
        assert_eq!(runtime.status()["keepalive"], false, "målet er endret");
        let writes = runtime.backend.writes.len();
        assert!(!runtime.animate(HOLD + 10).unwrap());
        assert_eq!(runtime.backend.writes.len(), writes);
        // Utløpt lease stopper også «hold».
        let mut runtime = setup();
        apply(&mut runtime, 0, off(), 60);
        runtime.handle(1000, 7, "rgb_hold", &identity(), 1).unwrap();
        runtime.tick(LEASE_MS + 5).unwrap();
        assert_eq!(runtime.status()["keepalive"], false);
    }

    #[test]
    fn avvik_i_ren_okt_stopper_hold_og_gjenopprettes_med_journal_uten_lås() {
        let mut runtime = setup();
        // Kortet har selv gått tilbake til noe annet (for eksempel etter kaldstart).
        runtime.backend.current = observed(&color([69, 200, 0]));
        let error = runtime.handle(1000, 7, "rgb_hold", &identity(), 1).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(runtime.backend.writes.is_empty(), "ingen skriving over et lys vi ikke har satt");
        let status = runtime.status();
        assert_eq!(status["drift"], true);
        assert_eq!(status["state"]["dirty"], false);
        assert!(status["state"]["error"].is_null(), "avvik er ingen sperre");
        // «Gjenopprett utgangspunktet»: journal før skriving, deretter én bekreftelse.
        let result = runtime.handle(1000, 7, "rgb_reset", &identity(), 2).unwrap();
        assert_eq!(result["outcome"]["status"], "recovery_needs_confirmation");
        assert!(runtime.backend.journal.is_some());
        assert_eq!(runtime.backend.writes, vec![color([1, 2, 3])]);
        let args = ack_args(&runtime);
        runtime.handle(1000, 7, "rgb_ack_recovery", &args, 3).unwrap();
        let status = runtime.status();
        assert_eq!(status["state"]["dirty"], false);
        assert_eq!(status["drift"], false);
        assert!(runtime.backend.journal.is_none());
        // Ny prøve virker igjen.
        apply(&mut runtime, 4, color([9, 9, 9]), 10);
    }

    #[test]
    fn effekt_dropper_like_rammer_og_har_hvile_etter_treg_io_uten_catchup() {
        let mut runtime = setup();
        start_effect(&mut runtime, Effect::Cycle, 0);
        assert!(!runtime.animate(1000).unwrap());
        assert_eq!(runtime.backend.writes.len(), 1);
        assert!(runtime.animate(2100).unwrap());
        runtime.frame_completed(4100).unwrap();
        assert!(!runtime.animate(4100).unwrap());
        assert!(!runtime.animate(4349).unwrap());
        assert!(runtime.animate(4350).unwrap());
        runtime.frame_completed(4350).unwrap();
        assert_eq!(runtime.backend.writes.len(), 3);
        // Ett sent tick gir bare ett bilde fra nåtiden, ingen tapte mellombilder.
        assert!(runtime.animate(10_000).unwrap());
        assert_eq!(runtime.backend.writes.len(), 4);
        assert!(!runtime.animate(10_001).unwrap());
        assert_eq!(runtime.status()["holding"]["deadlineMs"], 15_000);
    }

    #[test]
    fn lesefeil_under_effektbytte_stopper_effekten_og_gir_tilbake_utgangspunktet() {
        let mut runtime = setup();
        start_effect(&mut runtime, Effect::Cycle, 0);
        assert!(runtime.animate(2500).unwrap());
        runtime.frame_completed(2500).unwrap();
        let writes = runtime.backend.writes.len();
        runtime.backend.fail_read = true;
        assert!(
            runtime
                .handle(1000, 7, "rgb_apply", &apply_args(Request::Off {}, 10), 2600)
                .is_err()
        );
        runtime.backend.fail_read = false;
        assert!(!runtime.animation_active());
        // Ingen ny farge ble skrevet før lesingen; bare tilbakeføringen.
        assert_eq!(runtime.backend.writes.get(writes..).map(<[_]>::len), Some(1));
        assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
        // Som ved feilet effektramme: sluttilstanden bekreftes visuelt.
        let locked = runtime
            .handle(1000, 7, "rgb_read", &identity(), 2700)
            .unwrap_err();
        assert_eq!(locked.code, ErrorCode::WriteLocked);
        let status = &locked.details.as_ref().unwrap()["status"];
        assert!(status["effect"].is_null());
        assert!(
            status["runtimeError"]
                .as_str()
                .is_some_and(|e| e.starts_with("Lyseffekten er stoppet"))
        );
        runtime
            .handle(1000, 7, "rgb_ack_recovery", &ack_args(&runtime), 2750)
            .unwrap();
        // Ugyldig frist avvises før lesing, og effekten går videre.
        start_effect(&mut runtime, Effect::Rainbow, 2800);
        assert!(runtime.animation_active());
        let mut args = apply_args(Request::Off {}, 10);
        args["confirmTimeoutS"] = json!(1);
        assert!(runtime.handle(1000, 7, "rgb_apply", &args, 2900).is_err());
        assert!(runtime.animation_active());
    }

    #[test]
    fn effekt_krever_gyldig_statisk_valg_og_tillater_trygt_modusbytte() {
        let mut runtime = setup();
        for effect in [json!("ukjent"), Value::Null, json!(false)] {
            let mut args = apply_args(color([40, 50, 60]), 10);
            args["effect"] = effect;
            assert_eq!(
                runtime
                    .handle(1000, 7, "rgb_apply", &args, 0)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidArgument
            );
        }
        assert_eq!(
            runtime
                .handle(
                    1000,
                    7,
                    "rgb_apply",
                    &effect_args(Effect::Cycle, Request::Off {}),
                    0
                )
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
        assert_eq!(runtime.backend.reads, 0);
        assert!(runtime.backend.writes.is_empty());

        // 1) Fast farge (beholdt) -> start effekt direkte uten å måtte nullstille.
        let txn = apply(&mut runtime, 0, color([20, 30, 40]), 10);
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(txn), 1)
            .unwrap();
        let res = runtime
            .handle(
                1000,
                7,
                "rgb_apply",
                &effect_args(Effect::Cycle, color([40, 50, 60])),
                2,
            )
            .unwrap();
        assert_eq!(res["outcome"]["status"], "pending");
        assert_eq!(res["status"]["effect"], "cycle");
        // Mens effekten venter på Behold, avvises ny apply med Busy.
        assert_eq!(
            runtime
                .handle(1000, 7, "rgb_apply", &apply_args(Request::Off {}, 10), 3)
                .unwrap_err()
                .code,
            ErrorCode::Busy
        );
        let cycle_txn = res["outcome"]["txn"].as_u64().unwrap();
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(cycle_txn), 4)
            .unwrap();
        assert!(runtime.animation_active());
        // Kjør en animasjonsramme slik at observed != confirmed.
        assert!(runtime.animate(2500).unwrap());
        runtime.frame_completed(2500).unwrap();

        // Annen klient får fortsatt Busy mens eier 7 kjører effekt.
        assert_eq!(
            runtime
                .handle(1000, 8, "rgb_apply", &apply_args(Request::Off {}, 10), 2600)
                .unwrap_err()
                .code,
            ErrorCode::Busy
        );

        // 2) Aktiv effekt (Cycle) -> bytt direkte til annen effekt (Rainbow).
        let res = runtime
            .handle(
                1000,
                7,
                "rgb_apply",
                &effect_args(Effect::Rainbow, color([10, 200, 30])),
                2700,
            )
            .unwrap();
        assert_eq!(res["outcome"]["status"], "pending");
        assert_eq!(res["status"]["effect"], "rainbow");
        assert!(!runtime.animation_active());
        let rainbow_txn = res["outcome"]["txn"].as_u64().unwrap();
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(rainbow_txn), 2800)
            .unwrap();
        assert!(runtime.animation_active());
        assert!(runtime.animate(3800).unwrap());
        runtime.frame_completed(3800).unwrap();

        // 3) Aktiv effekt (Rainbow) -> bytt direkte til Av.
        let res = runtime
            .handle(1000, 7, "rgb_apply", &apply_args(Request::Off {}, 10), 3900)
            .unwrap();
        assert_eq!(res["outcome"]["status"], "pending");
        assert!(res["status"]["effect"].is_null());
        let off_txn = res["outcome"]["txn"].as_u64().unwrap();
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(off_txn), 4000)
            .unwrap();
        assert!(!runtime.animation_active());

        // 4) Fra Av (beholdt) -> start ny effekt (Breathing) direkte.
        let res = runtime
            .handle(
                1000,
                7,
                "rgb_apply",
                &effect_args(Effect::Breathing, color([100, 150, 200])),
                4100,
            )
            .unwrap();
        assert_eq!(res["outcome"]["status"], "pending");
        assert_eq!(res["status"]["effect"], "breathing");
        let breath_txn = res["outcome"]["txn"].as_u64().unwrap();
        runtime
            .handle(1000, 7, "rgb_confirm", &txn_args(breath_txn), 4200)
            .unwrap();
        assert!(runtime.animation_active());

        // 5) Uavklart gjenoppretting sperrer fortsatt effektstart.
        runtime
            .handle(1000, 7, "rgb_reset", &identity(), 4300)
            .unwrap();
        assert!(!runtime.animation_active());
        assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
        assert_eq!(
            runtime
                .handle(
                    1000,
                    7,
                    "rgb_apply",
                    &effect_args(Effect::Rainbow, color([40, 50, 60])),
                    4400
                )
                .unwrap_err()
                .code,
            ErrorCode::WriteLocked
        );
        runtime
            .handle(1000, 7, "rgb_ack_recovery", &ack_args(&runtime), 4500)
            .unwrap();
        start_effect(&mut runtime, Effect::Rainbow, 4600);
        assert!(runtime.animation_active());
    }

    #[test]
    fn effekt_med_av_baseline_er_ikke_stottet() {
        let current = Observed {
            color: Some([0; 3]),
            brightness_raw: Some(100),
            mode: Some(Mode::Off),
        };
        let baseline = Baseline::from_observed(binding(), current.clone()).unwrap();
        let auth = Authorization::new(1000, binding(), rigg_core::version::build()).unwrap();
        let mut runtime = Runtime::new(
            auth,
            baseline,
            Fake {
                current,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            runtime
                .handle(
                    1000,
                    7,
                    "rgb_apply",
                    &effect_args(Effect::Cycle, color([1, 2, 3])),
                    0
                )
                .unwrap_err()
                .code,
            ErrorCode::NotSupported
        );
        assert!(runtime.backend.writes.is_empty());
    }

    #[test]
    fn effekt_stopper_for_reset_release_klienttap_revoke_og_leaseutlop() {
        for stop in ["rgb_reset", "rgb_release", "disconnect", "revoke", "lease"] {
            let mut runtime = setup();
            start_effect(&mut runtime, Effect::Breathing, 0);
            assert!(runtime.disconnect(8).is_none());
            assert!(runtime.animation_active());
            match stop {
                "disconnect" => {
                    runtime.disconnect(7);
                }
                "revoke" => {
                    runtime.revoke();
                }
                "lease" => {
                    assert!(!runtime.animate(15_000).unwrap());
                }
                op => {
                    runtime.handle(1000, 7, op, &identity(), 1000).unwrap();
                }
            }
            assert!(runtime.status()["effect"].is_null(), "{stop}");
            assert_eq!(
                runtime.backend.writes,
                [color([120, 60, 30]), color([1, 2, 3])],
                "{stop}"
            );
            assert!(
                !runtime
                    .animate(if stop == "lease" { 15_001 } else { 1001 })
                    .unwrap()
            );
            assert_eq!(runtime.backend.writes.len(), 2, "{stop}");
        }
    }

    #[test]
    fn pending_effekt_utloper_uten_en_eneste_animasjonsramme() {
        let mut runtime = setup();
        runtime
            .handle(
                1000,
                7,
                "rgb_apply",
                &effect_args(Effect::Breathing, color([120, 60, 30])),
                0,
            )
            .unwrap();
        assert!(!runtime.animate(10_000).unwrap());
        assert!(runtime.status()["effect"].is_null());
        assert_eq!(
            runtime.backend.writes,
            [color([120, 60, 30]), color([1, 2, 3])]
        );
    }

    #[test]
    fn framefeil_stopper_og_recoverer_egne_delvise_endringer_bare_en_gang() {
        for fail_recovery in [false, true] {
            let mut runtime = setup();
            start_effect(&mut runtime, Effect::Breathing, 0);
            runtime.backend.partial_apply = true;
            runtime.backend.fail_recovery = fail_recovery;
            assert!(runtime.animate(1000).unwrap());
            assert!(runtime.status()["effect"].is_null());
            assert!(runtime.status()["runtimeError"].is_string());
            assert_eq!(runtime.backend.writes.len(), 3);
            assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
            assert!(runtime.book.snapshot().dirty);
            assert!(!runtime.animate(2000).unwrap());
            assert!(runtime.release().is_none());
            assert_eq!(runtime.backend.writes.len(), 3);
        }
    }

    #[test]
    fn ekstern_framedrift_stopper_uten_automatisk_overskriving() {
        let mut runtime = setup();
        start_effect(&mut runtime, Effect::Breathing, 0);
        runtime.backend.current = observed(&color([6, 7, 8]));
        assert!(runtime.animate(1000).unwrap());
        assert!(runtime.status()["effect"].is_null());
        assert_eq!(runtime.backend.writes.len(), 1);
        assert!(runtime.book.snapshot().dirty);
        assert!(runtime.release().is_none());
        assert!(!runtime.animate(20_000).unwrap());
        assert_eq!(runtime.backend.writes.len(), 1);
        runtime
            .handle(1000, 7, "rgb_reset", &identity(), 20_001)
            .unwrap();
        assert_eq!(runtime.backend.writes.last(), Some(&color([1, 2, 3])));
    }

    #[test]
    fn tilbakegaende_animasjonsklokke_stopper_og_frigir_en_gang() {
        let mut runtime = setup();
        start_effect(&mut runtime, Effect::Breathing, 1000);
        assert!(runtime.animate(2000).unwrap());
        runtime.frame_completed(2000).unwrap();
        assert_eq!(
            runtime.animate(1999).unwrap_err().code,
            ErrorCode::WriteLocked
        );
        assert!(runtime.status()["effect"].is_null());
        assert_eq!(runtime.backend.writes.len(), 3);
        assert!(runtime.animate(3000).is_err());
        assert_eq!(runtime.backend.writes.len(), 3);
    }
}
