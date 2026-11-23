//! Protokollen mellom vinduet og tjenesten: NDJSON over Unix-socket,
//! protokoll 1 (docs/daemon-api.md, utvidet i arkitektur.md kap. 5 og
//! docs/produksjon/protokoll.md). Typene står bare her og brukes av begge sider.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL: u32 = 1;
/// Største melding per linje (byte, uten linjeskiftet).
pub const MAX_LINE: usize = 65_536;
pub const MAX_CLIENTS: usize = 16;
/// Takt i `subscribe` (ms). Aldri raskere enn 2 Hz (maskinvare-revisjon kap. 1).
pub const MIN_INTERVAL_MS: u32 = 500;
pub const MAX_INTERVAL_MS: u32 = 2000;
/// Takt i bakgrunnen (ms): 1, 2 eller 5 s.
pub const BACKGROUND_INTERVALS: [u32; 3] = [1000, 2000, 5000];
/// Hvor lenge tjenesten venter etter siste klient før den stopper (uten bakgrunn).
pub const GRACE_MS: u32 = 60_000;

/// Leseoperasjonene. Alle står i `capabilities.ops` med begge backendene.
pub const READ_OPS: &[&str] = &[
    "hello",
    "ping",
    "list_gpus",
    "get_static",
    "get_system",
    "get_limits",
    "get_state",
    "get_telemetry",
    "subscribe",
    "unsubscribe",
    "set_focus",
    "get_history",
    "get_stats",
    "reset_stats",
    "get_processes",
    "get_tasks",
    "get_task_detail",
    "get_disks",
    "get_network",
    "log_start",
    "log_stop",
    "log_status",
    "get_settings",
    "set_settings",
    "clear_history",
    "shutdown_when_idle",
    "get_helper",
];

/// Lokal prosessadministrasjon. Uavhengig av GPU-skriving og root-hjelperen;
/// daemonen krever egen bekreftelse og kontrollerer prosessidentiteten.
pub const TASK_OPS: &[&str] = &["terminate_task"];

/// Skriveoperasjonene. De står bare i `ops` med `--backend sim`; med NVML
/// svarer tjenesten `write_locked` (arkitektur.md kap. 5).
pub const WRITE_OPS: &[&str] = &[
    "apply",
    "set_power_limit",
    "set_clock_offsets",
    "set_temp_target",
    "set_fan",
    "confirm",
    "revert",
    "reset",
    "rgb_get",
    "rgb_set",
];

/// `txn` kommer bare med `--backend sim` (Behold eller Rull tilbake).
pub const EVENTS: &[&str] = &[
    "telemetry",
    "device",
    "stats",
    "log",
    "alarm",
    "settings",
    "txn",
    "shutdown",
];

/// Emnene en klient kan abonnere på.
pub const TOPICS: &[&str] = &[
    "telemetry",
    "device",
    "stats",
    "log",
    "alarm",
    "settings",
    "txn",
];

/// Trinnene for skriving (produktspesifikasjonen 6.2), i rekkefølgen VEIKART har valgt.
pub const WRITE_STEPS: &[&str] = &[
    "power",
    "clocks",
    "fan_manual",
    "fan_curve",
    "temp_target",
    "rgb",
];

/// Feilkodene (docs/daemon-api.md kap. 7, pluss `write_locked`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    BadJson,
    MessageTooLarge,
    BadRequest,
    HelloRequired,
    UnsupportedProtocol,
    UnknownOp,
    InvalidArgument,
    UnknownGpu,
    NotSupported,
    PermissionDenied,
    Busy,
    NoPending,
    TxnMismatch,
    NotOwner,
    NotImplemented,
    TooManyClients,
    ShuttingDown,
    Internal,
    WriteLocked,
    Unavailable,
    /// Hjelperen er på, men trinnet er ikke godkjent (runde 7A).
    StepNotEnabled,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        use ErrorCode::*;
        match self {
            BadJson => "bad_json",
            MessageTooLarge => "message_too_large",
            BadRequest => "bad_request",
            HelloRequired => "hello_required",
            UnsupportedProtocol => "unsupported_protocol",
            UnknownOp => "unknown_op",
            InvalidArgument => "invalid_argument",
            UnknownGpu => "unknown_gpu",
            NotSupported => "not_supported",
            PermissionDenied => "permission_denied",
            Busy => "busy",
            NoPending => "no_pending",
            TxnMismatch => "txn_mismatch",
            NotOwner => "not_owner",
            NotImplemented => "not_implemented",
            TooManyClients => "too_many_clients",
            ShuttingDown => "shutting_down",
            Internal => "internal",
            WriteLocked => "write_locked",
            Unavailable => "unavailable",
            StepNotEnabled => "step_not_enabled",
        }
    }

    pub fn from_code(s: &str) -> Option<ErrorCode> {
        use ErrorCode::*;
        Some(match s {
            "bad_json" => BadJson,
            "message_too_large" => MessageTooLarge,
            "bad_request" => BadRequest,
            "hello_required" => HelloRequired,
            "unsupported_protocol" => UnsupportedProtocol,
            "unknown_op" => UnknownOp,
            "invalid_argument" => InvalidArgument,
            "unknown_gpu" => UnknownGpu,
            "not_supported" => NotSupported,
            "permission_denied" => PermissionDenied,
            "busy" => Busy,
            "no_pending" => NoPending,
            "txn_mismatch" => TxnMismatch,
            "not_owner" => NotOwner,
            "not_implemented" => NotImplemented,
            "too_many_clients" => TooManyClients,
            "shutting_down" => ShuttingDown,
            "internal" => Internal,
            "write_locked" => WriteLocked,
            "unavailable" => Unavailable,
            "step_not_enabled" => StepNotEnabled,
            _ => return None,
        })
    }

    /// Standardmeldingen på norsk.
    pub fn message(self) -> &'static str {
        use ErrorCode::*;
        match self {
            BadJson => "Meldingen er ikke gyldig JSON.",
            MessageTooLarge => "Meldingen er for stor (høyst 64 KiB per linje).",
            BadRequest => "Forespørselen mangler id eller op, eller args er ikke et objekt.",
            HelloRequired => "Første melding må være hello.",
            UnsupportedProtocol => "Ingen felles protokollversjon.",
            UnknownOp => "Ukjent operasjon.",
            InvalidArgument => "Ugyldig argument.",
            UnknownGpu => "Ukjent skjermkort.",
            NotSupported => "Kortet eller driveren støtter ikke dette.",
            PermissionDenied => "Ikke tillatt.",
            Busy => "En annen klient har en endring som venter.",
            NoPending => "Ingen endring venter.",
            TxnMismatch => "Endringen finnes ikke lenger.",
            NotOwner => "Bare klienten som gjorde endringen, kan bekrefte den.",
            NotImplemented => "Ikke laget ennå.",
            TooManyClients => "For mange klienter (høyst 16).",
            ShuttingDown => "Tjenesten stopper.",
            Internal => "Feil i tjenesten.",
            WriteLocked => "Endringer er låst. Skriving til skjermkortet er ikke slått på.",
            Unavailable => "Ikke tilgjengelig nå.",
            StepNotEnabled => "Trinnet er ikke godkjent.",
        }
    }
}

/// En feil i et svar.
#[derive(Clone, Debug, PartialEq)]
pub struct ProtoError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl ProtoError {
    pub fn new(code: ErrorCode) -> Self {
        ProtoError {
            code,
            message: code.message().to_string(),
            details: None,
        }
    }
    pub fn msg(code: ErrorCode, message: impl Into<String>) -> Self {
        ProtoError {
            code,
            message: message.into(),
            details: None,
        }
    }
    pub fn details(mut self, d: Value) -> Self {
        self.details = Some(d);
        self
    }
    pub fn to_json(&self) -> Value {
        let mut e = json!({"code": self.code.as_str(), "message": self.message});
        if let Some(d) = &self.details {
            e["details"] = d.clone();
        }
        e
    }
    pub fn from_json(v: &Value) -> ProtoError {
        let code = v
            .get("code")
            .and_then(Value::as_str)
            .and_then(ErrorCode::from_code)
            .unwrap_or(ErrorCode::Internal);
        let message = v
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| code.message().into());
        ProtoError {
            code,
            message,
            details: v.get("details").cloned(),
        }
    }
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code.as_str())
    }
}

pub type OpResult = Result<Value, ProtoError>;

/// En forespørsel som er tolket.
#[derive(Clone, Debug)]
pub struct Request {
    pub id: Value,
    pub op: String,
    pub args: Value,
}

struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;
            fn expecting(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                fmt.write_str("JSON uten dupliserte felter")
            }
            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_str<E>(self, value: &str) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(Value::Array(values))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Value, A::Error> {
                let mut fields = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if fields.contains_key(&key) {
                        return Err(serde::de::Error::custom("Duplisert felt"));
                    }
                    fields.insert(key, map.next_value::<UniqueValue>()?.0);
                }
                Ok(Value::Object(fields))
            }
        }
        deserializer.deserialize_any(Visitor).map(UniqueValue)
    }
}

/// Tolker én linje. Feil gir `(id, feil)`; id er `null` når den ikke kunne leses.
pub fn parse_request(line: &[u8]) -> Result<Request, (Value, ProtoError)> {
    let UniqueValue(v) = serde_json::from_slice(line)
        .map_err(|_| (Value::Null, ProtoError::new(ErrorCode::BadJson)))?;
    let Some(obj) = v.as_object() else {
        return Err((Value::Null, ProtoError::new(ErrorCode::BadRequest)));
    };
    let id = obj.get("id").cloned().unwrap_or(Value::Null);
    let id_ok = match &id {
        Value::Number(n) => n.is_i64() || n.is_u64(),
        Value::String(s) => !s.is_empty() && s.chars().count() <= 64,
        _ => false,
    };
    if !id_ok {
        return Err((Value::Null, ProtoError::new(ErrorCode::BadRequest)));
    }
    let Some(op) = obj
        .get("op")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 64)
    else {
        return Err((id, ProtoError::new(ErrorCode::BadRequest)));
    };
    let args = match obj.get("args") {
        None | Some(Value::Null) => Value::Object(Default::default()),
        Some(a @ Value::Object(_)) => a.clone(),
        Some(_) => return Err((id, ProtoError::new(ErrorCode::BadRequest))),
    };
    Ok(Request {
        id,
        op: op.to_string(),
        args,
    })
}

/// Svar som én linje med linjeskift.
pub fn response_line(id: &Value, r: &OpResult) -> Vec<u8> {
    let v = match r {
        Ok(result) => json!({"id": id, "ok": true, "result": result}),
        Err(e) => json!({"id": id, "ok": false, "error": e.to_json()}),
    };
    let mut out = serde_json::to_vec(&v).unwrap_or_default();
    out.push(b'\n');
    out
}

/// Hendelse som én linje med linjeskift.
pub fn event_line(event: &str, data: &Value) -> Vec<u8> {
    let mut out = serde_json::to_vec(&json!({"event": event, "data": data})).unwrap_or_default();
    out.push(b'\n');
    out
}

/// En melding fra tjenesten slik klienten ser den.
#[derive(Clone, Debug)]
pub enum Incoming {
    Response { id: Value, result: OpResult },
    Event { event: String, data: Value },
}

pub fn parse_incoming(line: &[u8]) -> Option<Incoming> {
    let v: Value = serde_json::from_slice(line).ok()?;
    if let Some(ev) = v.get("event").and_then(Value::as_str) {
        return Some(Incoming::Event {
            event: ev.to_string(),
            data: v.get("data").cloned().unwrap_or(Value::Null),
        });
    }
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let ok = v.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let result = if ok {
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(ProtoError::from_json(
            v.get("error").unwrap_or(&Value::Null),
        ))
    };
    Some(Incoming::Response { id, result })
}

/// Deler en bytestrøm i linjer, med grense per linje.
#[derive(Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
    overflow: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LineEvent {
    Line(Vec<u8>),
    /// En linje passerte grensen. Forbindelsen skal lukkes.
    TooLarge,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Legger til bytes og gir hele linjer (uten `\n`, og uten tomme linjer).
    pub fn push(&mut self, data: &[u8], limit: usize) -> Vec<LineEvent> {
        let mut out = Vec::new();
        if self.overflow {
            return out;
        }
        self.buf.extend_from_slice(data);
        let mut start = 0;
        while let Some(pos) = self.buf[start..].iter().position(|&b| b == b'\n') {
            let end = start + pos;
            let mut line = &self.buf[start..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            if line.len() > limit {
                self.overflow = true;
                out.push(LineEvent::TooLarge);
                self.buf.clear();
                return out;
            }
            if !line.iter().all(u8::is_ascii_whitespace) {
                out.push(LineEvent::Line(line.to_vec()));
            }
            start = end + 1;
        }
        self.buf.drain(..start);
        if self.buf.len() > limit {
            self.overflow = true;
            self.buf.clear();
            out.push(LineEvent::TooLarge);
        }
        out
    }
}

/// `capabilities.write`: hvert trinn og om det er godkjent. Ingen trinn er
/// godkjent i 1.0 (produktspesifikasjonen kap. 6).
pub fn write_capabilities(backend: &str) -> Value {
    let steps: Vec<Value> = WRITE_STEPS
        .iter()
        .map(|s| json!({"id": s, "status": "unavailable"}))
        .collect();
    let state = if backend == "sim" { "sim" } else { "locked" };
    let fields = if backend == "sim" {
        let cap = |r: crate::txn::Range| json!({"available": true, "min": r.min, "max": r.max, "readback": true});
        json!({"power": cap(crate::txn::HW_POWER), "core_clock": cap(crate::txn::HW_CORE),
            "mem_clock": cap(crate::txn::HW_MEM), "fan_manual": cap(crate::txn::HW_FAN),
            "fan_curve": cap(crate::txn::HW_FAN), "temp_target": cap(crate::txn::HW_TEMP),
            "rgb": {"available": false, "reason": "RGB-backenden er ikke laget", "readback": false}})
    } else {
        json!({})
    };
    json!({"state": state, "steps": steps, "fields": fields})
}

/// `ops` i `hello` for en backend: skriveoperasjonene bare med sim.
pub fn ops_for(backend: &str) -> Vec<&'static str> {
    let mut ops: Vec<&'static str> = READ_OPS.to_vec();
    ops.extend_from_slice(TASK_OPS);
    if backend == "sim" {
        ops.extend_from_slice(WRITE_OPS);
    }
    ops
}

/// Svaret på `hello` (felt tjenesten fyller inn).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DaemonInfo {
    pub name: String,
    pub version: String,
    pub build: u32,
    pub mock: bool,
    pub pid: u32,
    pub backend: String,
    pub started_at: i64,
}

/// Tolker takten i `subscribe`: 500–2000 ms, ellers klemmes den.
pub fn clamp_interval(ms: Option<u64>) -> u32 {
    let v = ms
        .unwrap_or(1000)
        .clamp(MIN_INTERVAL_MS as u64, MAX_INTERVAL_MS as u64);
    v as u32
}

#[cfg(test)]
mod tests {
    #[test]
    fn dupliserte_felt_avvises_ogsa_i_nestede_argumenter() {
        for line in [
            r#"{"id":1,"op":"ping","op":"ping"}"#,
            r#"{"id":1,"op":"status","args":{},"args":{}}"#,
            r#"{"id":1,"op":"apply","args":{"liste":[{"felt":0,"f\u0065lt":1}]}}"#,
        ] {
            assert!(super::parse_request(line.as_bytes()).is_err());
        }
        assert!(super::parse_request(br#"{"id":1,"op":"status","args":{}}"#).is_ok());
    }
    use super::*;

    #[test]
    fn linjer_deles_og_grensen_holder() {
        let mut lb = LineBuffer::new();
        assert!(lb.push(b"{\"a\":1}", 100).is_empty());
        assert_eq!(
            lb.push(b"\n\n{\"b\":2}\r\n", 100),
            vec![
                LineEvent::Line(b"{\"a\":1}".to_vec()),
                LineEvent::Line(b"{\"b\":2}".to_vec())
            ]
        );
        let big = vec![b'x'; 101];
        assert_eq!(lb.push(&big, 100), vec![LineEvent::TooLarge]);
        assert!(
            lb.push(b"\n", 100).is_empty(),
            "etter for lang linje leses ingenting mer"
        );
    }

    #[test]
    fn forespørsler_tolkes_strengt() {
        let r = parse_request(br#"{"id":1,"op":"hello","args":{"protocol":[1]}}"#).unwrap();
        assert_eq!(r.op, "hello");
        assert_eq!(
            parse_request(b"{ikke json").unwrap_err().1.code,
            ErrorCode::BadJson
        );
        assert_eq!(
            parse_request(b"[1]").unwrap_err().1.code,
            ErrorCode::BadRequest
        );
        assert_eq!(
            parse_request(br#"{"op":"ping"}"#).unwrap_err().1.code,
            ErrorCode::BadRequest
        );
        assert_eq!(
            parse_request(br#"{"id":1,"op":"ping","args":3}"#)
                .unwrap_err()
                .1
                .code,
            ErrorCode::BadRequest
        );
        assert_eq!(
            parse_request(br#"{"id":true,"op":"ping"}"#)
                .unwrap_err()
                .1
                .code,
            ErrorCode::BadRequest
        );
        let r = parse_request(br#"{"id":"a","op":"ping"}"#).unwrap();
        assert!(r.args.is_object());
    }

    #[test]
    fn skriveoperasjoner_bare_med_sim() {
        let nvml = ops_for("nvml");
        assert!(!nvml.iter().any(|o| WRITE_OPS.contains(o)));
        assert!(nvml.contains(&"get_tasks"));
        assert!(nvml.contains(&"terminate_task"));
        assert!(!WRITE_OPS.contains(&"terminate_task"));
        assert!(ops_for("sim").contains(&"apply"));
        assert_eq!(write_capabilities("nvml")["state"], "locked");
        assert_eq!(
            write_capabilities("nvml")["steps"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }

    #[test]
    fn svar_og_feil_rundtur() {
        let line = response_line(&json!(3), &Err(ProtoError::new(ErrorCode::WriteLocked)));
        match parse_incoming(&line).unwrap() {
            Incoming::Response { id, result } => {
                assert_eq!(id, json!(3));
                let e = result.unwrap_err();
                assert_eq!(e.code, ErrorCode::WriteLocked);
                assert!(e.message.contains("låst"));
            }
            _ => panic!(),
        }
        assert_eq!(clamp_interval(Some(100)), 500);
        assert_eq!(clamp_interval(Some(9000)), 2000);
        assert_eq!(clamp_interval(None), 1000);
    }
}
