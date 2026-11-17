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
use rigg_core::rgb::{Baseline, RecoveryReadback, Request};
use rigg_core::rgb_auth::{self, Authorization};
use rigg_core::rgb_txn::{Backend, Book, Error, Outcome};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub const PROTOCOL: u32 = 1;
pub const LEASE_MS: u64 = 15_000;

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
}

enum Action {
    Status,
    Apply(ApplyArgs),
    Confirm(TransactionArgs),
    Revert(TransactionArgs),
    Reset(IdentityArgs),
    Release(IdentityArgs),
    Heartbeat(IdentityArgs),
    AckRecovery(RecoveryAckArgs),
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
                let args: ApplyArgs = parse(args)?;
                if !(10..=60).contains(&args.confirm_timeout_s) {
                    return Err(ProtoError::msg(
                        ErrorCode::InvalidArgument,
                        "RGB-fristen må være et heltall fra 10 til 60 sekunder.",
                    ));
                }
                Self::Apply(args)
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
            "rgb_release" => Self::Release(parse(args)?),
            "rgb_heartbeat" => Self::Heartbeat(parse(args)?),
            "rgb_ack_recovery" => {
                let args: RecoveryAckArgs = parse(args)?;
                if !args.confirmed_static {
                    return Err(ProtoError::msg(
                        ErrorCode::InvalidArgument,
                        "Gjenopprettet statisk lys må være uttrykkelig bekreftet av brukeren.",
                    ));
                }
                Self::AckRecovery(args)
            }
            _ => return Err(ProtoError::new(ErrorCode::UnknownOp)),
        })
    }

    fn identity(&self) -> Option<(&str, &str)> {
        match self {
            Self::Status => None,
            Self::Apply(args) => Some((&args.gpu, &args.uuid)),
            Self::AckRecovery(args) => Some((&args.gpu, &args.uuid)),
            Self::Confirm(args) | Self::Revert(args) => Some((&args.gpu, &args.uuid)),
            Self::Reset(args) | Self::Release(args) | Self::Heartbeat(args) => {
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
}

impl<B: Backend> Runtime<B> {
    /// Ingen maskinvarekall. `authorization` og `baseline` må komme fra
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
        let book = Book::new(baseline).map_err(|error| error.to_string())?;
        Ok(Self {
            authorization,
            book,
            backend,
            holding: None,
            last_ms: None,
            last_outcome: None,
            observation_source: ObservationSource::StoredBaseline,
            clock_error: false,
            revoked: false,
        })
    }

    pub fn authorized_uid(&self) -> u32 {
        self.authorization.uid()
    }

    /// Ren cache: ingen backend-kall, klokkeoppdatering eller leasefornyelse.
    pub fn status(&self) -> Value {
        json!({
            "helper": {
                "name": "agentic-rigg-rgb-helper",
                "domain": "palit_rgb",
                "version": rigg_core::version::VERSION,
                "build": rigg_core::version::build(),
                "protocol": PROTOCOL
            },
            "identity": self.authorization.binding(),
            "authorized": !self.revoked,
            "holding": self.holding,
            "state": self.book.snapshot(),
            "observationSource": self.observation_source,
            "lastOutcome": self.last_outcome,
            "runtimeError": self.clock_error.then_some(
                "Den monotone RGB-klokken gikk bakover. Nye endringer er sperret."
            )
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
        let ack_before_tick =
            matches!(action, Action::AckRecovery(_)).then(|| self.book.snapshot());
        self.tick(now_ms)?;
        if ack_before_tick.is_some_and(|before| before != self.book.snapshot()) {
            return Err(ProtoError::msg(ErrorCode::WriteLocked,
                "RGB-tilstanden endret seg ved fristutløp. Se det nye målet før du bekrefter lyset.")
                .details(json!({"status": self.status()})));
        }
        let before_observation = self.book.snapshot().observed;
        let (result, renew) = match action {
            Action::Status => unreachable!("status ble håndtert uten sideeffekter"),
            Action::Apply(args) => (
                self.book
                    .apply(
                        &mut self.backend,
                        conn,
                        args.request,
                        now_ms,
                        args.confirm_timeout_s * 1000,
                    )
                    .map(Some),
                true,
            ),
            Action::Confirm(args) => (
                self.book
                    .keep(&mut self.backend, conn, args.txn, now_ms)
                    .map(Some),
                true,
            ),
            Action::Revert(args) => (
                self.book
                    .revert(&mut self.backend, conn, args.txn, now_ms)
                    .map(Some),
                false,
            ),
            Action::Heartbeat(_) => {
                self.require_owner(conn, false)?;
                (Ok(None), true)
            }
            Action::Release(_) => {
                self.require_owner(conn, false)?;
                (Ok(self.release()), false)
            }
            Action::Reset(_) => {
                self.require_owner(conn, true)?;
                (Ok(self.book.restore_baseline(&mut self.backend)), false)
            }
            Action::AckRecovery(args) => {
                self.require_owner(conn, true)?;
                if args.expected != self.book.snapshot().confirmed {
                    return Err(ProtoError::msg(ErrorCode::WriteLocked,
                        "RGB-målet er endret siden dialogen ble åpnet. Se den nye tilstanden før du bekrefter.")
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
        self.sync_holding(lease_deadline, renew && result.is_ok());
        match result {
            Ok(outcome) => {
                if let Some(outcome) = &outcome {
                    self.record_outcome(outcome);
                }
                Ok(json!({"outcome": outcome, "status": self.status()}))
            }
            Err(error) => {
                if self.book.snapshot().observed != before_observation {
                    self.observation_source = ObservationSource::Unknown;
                }
                Err(self.transaction_error(error))
            }
        }
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

    fn require_owner(&self, conn: u64, allow_unowned: bool) -> Result<(), ProtoError> {
        match self.book.snapshot().owner {
            Some(owner) if owner == conn => Ok(()),
            None if allow_unowned => Ok(()),
            _ => Err(ProtoError::new(ErrorCode::NotOwner)),
        }
    }

    fn sync_holding(&mut self, deadline_ms: u64, renew: bool) {
        let Some(owner) = self.book.snapshot().owner else {
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
        match self.book.tick(&mut self.backend, now_ms) {
            Ok(outcome) => {
                if self.book.snapshot().owner.is_none() {
                    self.holding = None;
                }
                if let Some(outcome) = &outcome {
                    self.record_outcome(outcome);
                }
                Ok(outcome.into_iter().collect())
            }
            Err(error) => Err(self.transaction_error(error)),
        }
    }

    pub fn disconnect(&mut self, conn: u64) -> Option<Outcome> {
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
    }

    impl Backend for Fake {
        fn read(&mut self, binding: &Binding) -> Result<Observed, String> {
            assert_eq!(binding, &self::binding());
            self.reads += 1;
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
        fn clear_dirty(&mut self, _: &Binding) -> Result<(), String> {
            panic!("Palit-fixturen beviser ikke modus og kan aldri rydde dirty")
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

    fn ack_args() -> Value {
        ack_args_for(color([1, 2, 3]))
    }

    fn ack_args_for(expected: Request) -> Value {
        let mut args = identity();
        args["confirmedStatic"] = json!(true);
        args["expected"] = serde_json::to_value(expected).unwrap();
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
                let mut a = ack_args();
                a["confirmedStatic"] = json!(false);
                a
            },
            {
                let mut a = ack_args();
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
                .handle(1001, 7, "rgb_ack_recovery", &ack_args(), 30)
                .unwrap_err()
                .code,
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            runtime
                .handle(1000, 8, "rgb_ack_recovery", &ack_args(), 30)
                .unwrap_err()
                .code,
            ErrorCode::NotOwner
        );
        assert_eq!(runtime.backend.reads, reads);
        let result = runtime
            .handle(1000, 7, "rgb_ack_recovery", &ack_args(), 40)
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
            let result = runtime.handle(1000, 8, "rgb_ack_recovery", &ack_args(), 30);
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
                &ack_args_for(color([4, 5, 6])),
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
                &ack_args_for(color([4, 5, 6])),
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
                    &ack_args_for(color([4, 5, 6])),
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
}
