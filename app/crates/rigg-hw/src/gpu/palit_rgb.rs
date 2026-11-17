//! Eksplisitt, kortbundet Palit-identifikasjon og avgrenset statisk farge.
//!
//! Ingen kode her kjører ved modulimport eller vanlig GPU-innsamling.
//! `metadata_preview` leser bare sysfs/proc, og `identify` er fortsatt rent
//! lesende. Den separate `apply` krever en validert, kortbundet baseline;
//! kalleren må i tillegg ha autorisasjon og varig dirty-journal før kallet.
//! Ingen adresse-/registerskanning, automatisk rollback eller alternativ buss.
//!
//! Transport: Linux i2c-dev + NVIDIA 610.43.02 `nvidia/nv-i2c.c`.
//! Lesesett: ThunderMaster-opptak fra OpenRGB-sakene 1420 og 1938:
//! signatur F0–FF, deretter 6C–6F når signaturen er kjent. Ingen lesing av
//! ukjente modusregistre; nåværende modus og full reverserbarhet er ukjent.

use rigg_core::hjelper::{valid_pci, valid_uuid};
use rigg_core::rgb::{
    AdapterIdentity, Baseline, Binding, Discovery, Observed, PciIdentity, RecoveryReadback,
    Request, Signature,
};
use serde_json::{Value, json};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const ADDRESS: u8 = 0x49;
const I2C_MAJOR: u32 = 89;
const READ_BYTE_DATA: libc::c_ulong = 0x0008_0000;
const WRITE_BYTE_DATA: libc::c_ulong = 0x0010_0000;
const I2C_SLAVE: libc::c_ulong = 0x0703;
const I2C_FUNCS: libc::c_ulong = 0x0705;
const I2C_SMBUS: libc::c_ulong = 0x0720;

/// Feil bevarer errno uten å tolke NVIDIA EIO som en bestemt maskinvarefeil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalitRgbError {
    code: &'static str,
    message: String,
    errno: Option<i32>,
    raw_signature: Option<[u8; 16]>,
}

impl PalitRgbError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            errno: None,
            raw_signature: None,
        }
    }

    fn io(code: &'static str, operation: &str, error: io::Error) -> Self {
        Self {
            code,
            message: format!("{operation}: {error}"),
            errno: error.raw_os_error(),
            raw_signature: None,
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn errno(&self) -> Option<i32> {
        self.errno
    }

    pub fn to_json(&self) -> Value {
        json!({"code": self.code, "message": self.message, "errno": self.errno,
            "rawSignature": self.raw_signature})
    }
}

impl fmt::Display for PalitRgbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PalitRgbError {}

type Result<T> = std::result::Result<T, PalitRgbError>;

/// Sammenligning med forespørselen, aldri bevis på aktiv modus eller full
/// gjenoppretting. Alle fem skrivekall har returnert uten feil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyReadback {
    pub binding: Binding,
    pub before: Observed,
    pub observed: Observed,
    pub request_readback: RecoveryReadback,
}

impl ApplyReadback {
    pub fn to_json(&self) -> Value {
        json!({"binding": self.binding, "before": self.before,
            "observed": self.observed, "requestReadback": self.request_readback,
            "writesAttempted": 5, "writesCompleted": 5, "modeVerified": false})
    }
}

/// Et feilet ioctl kan allerede ha påvirket kontrolleren. `writes_completed`
/// teller bare kall uten feil, ikke fysisk bekreftede registerverdier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyFailure {
    pub cause: PalitRgbError,
    pub writes_attempted: u8,
    pub writes_completed: u8,
    pub observed_after: Option<Observed>,
}

impl ApplyFailure {
    fn before_write(cause: PalitRgbError) -> Self {
        Self {
            cause,
            writes_attempted: 0,
            writes_completed: 0,
            observed_after: None,
        }
    }

    pub fn hardware_state_unknown(&self) -> bool {
        self.writes_attempted != 0
    }

    pub fn to_json(&self) -> Value {
        json!({"cause": self.cause.to_json(), "writesAttempted": self.writes_attempted,
            "writesCompleted": self.writes_completed, "observedAfter": self.observed_after,
            "hardwareStateUnknown": self.hardware_state_unknown(), "modeVerified": false})
    }
}

impl fmt::Display for ApplyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)?;
        if self.hardware_state_unknown() {
            write!(
                f,
                "; fysisk lystilstand er ukjent og kan være delvis endret"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ApplyFailure {}

/// Et øyeblikksbilde av metadata, ikke et bevis for en RGB-kontroller.
/// Bussnummeret brukes bare til denne åpningen, aldri som varig binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalitRgbMetadata {
    root: PathBuf,
    pci: String,
    uuid: String,
    gpu_minor: u32,
    pci_path: PathBuf,
    adapter_path: PathBuf,
    adapter_name: String,
    bus_number: u32,
    device_path: PathBuf,
    device_major: u32,
    device_minor: u32,
}

impl PalitRgbMetadata {
    pub fn to_json(&self) -> Value {
        json!({
            "pci": self.pci,
            "uuid": self.uuid,
            "gpuMinor": self.gpu_minor,
            "pciIds": {"vendor": 0x10de, "device": 0x1e87,
                "subsystemVendor": 0x10de, "subsystemDevice": 0x1e87},
            "driver": "nvidia",
            "adapter": {"port": 1, "name": self.adapter_name,
                "sysfsPath": self.adapter_path, "parentPci": self.pci,
                "busNumber": self.bus_number},
            "devicePath": self.device_path,
            "deviceNumber": {"major": self.device_major, "minor": self.device_minor},
            "address": ADDRESS,
            "controllerIdentified": false,
            "registerReads": 0,
            "registerWrites": 0,
        })
    }

    fn binding(&self, signature: Signature) -> Binding {
        Binding {
            pci: self.pci.clone(),
            uuid: self.uuid.clone(),
            pci_ids: PciIdentity {
                vendor: 0x10de,
                device: 0x1e87,
                subsystem_vendor: 0x10de,
                subsystem_device: 0x1e87,
            },
            adapter: AdapterIdentity {
                port: 1,
                name: self.adapter_name.clone(),
                parent_pci: self.pci.clone(),
                driver: "nvidia".into(),
            },
            address: ADDRESS,
            signature,
        }
    }
}

fn bounded_path(root: &Path, path: &Path) -> Result<PathBuf> {
    let resolved = fs::canonicalize(path)
        .map_err(|e| PalitRgbError::io("metadata_unavailable", "Les metadata-sti", e))?;
    if !resolved.starts_with(root) {
        return Err(PalitRgbError::new(
            "invalid_root",
            "Metadata peker utenfor valgt rot",
        ));
    }
    Ok(resolved)
}

fn text(root: &Path, path: &Path) -> Result<String> {
    let path = bounded_path(root, path)?;
    if !fs::metadata(&path)
        .map_err(|e| PalitRgbError::io("metadata_unavailable", "Kontroller metadatafil", e))?
        .is_file()
    {
        return Err(PalitRgbError::new(
            "invalid_metadata",
            "Metadata er ikke en ordinær fil",
        ));
    }
    let file = File::open(path)
        .map_err(|e| PalitRgbError::io("metadata_unavailable", "Åpne metadata", e))?;
    let mut value = String::new();
    file.take(8193)
        .read_to_string(&mut value)
        .map_err(|e| PalitRgbError::io("metadata_unavailable", "Les metadata", e))?;
    if value.len() > 8192 {
        return Err(PalitRgbError::new(
            "invalid_metadata",
            "Metadata er for store",
        ));
    }
    Ok(value.trim().to_owned())
}

fn proc_field<'a>(information: &'a str, field: &str) -> Result<&'a str> {
    let mut values = information.lines().filter_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == field).then_some(value.trim())
    });
    let value = values.next().ok_or_else(|| {
        PalitRgbError::new(
            "identity_mismatch",
            format!("NVIDIA-metadata mangler {field}"),
        )
    })?;
    if values.next().is_some() {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "Tvetydig NVIDIA-identitet",
        ));
    }
    Ok(value)
}

fn absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(PalitRgbError::io(
            "metadata_unavailable",
            "Kontroller I2C-klient",
            e,
        )),
        Ok(_) => Err(PalitRgbError::new(
            "address_busy",
            "Adresse 0x49 har allerede en registrert kjerneklient",
        )),
    }
}

fn deny_protected_children(root: &Path, adapter: &Path, bus_number: u32) -> Result<()> {
    let prefix = format!("{bus_number}-");
    let entries = fs::read_dir(adapter)
        .map_err(|e| PalitRgbError::io("metadata_unavailable", "Les klientnavn", e))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| PalitRgbError::io("metadata_unavailable", "Les klientnavn", e))?;
        let entry_name = entry.file_name();
        let Some(address) = entry_name
            .to_str()
            .and_then(|s| s.strip_prefix(prefix.as_str()))
        else {
            continue;
        };
        if address.len() != 4 || !address.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let name = text(root, &entry.path().join("name"))?;
        if matches!(name.as_str(), "ccgx-ucsi" | "ee1004" | "spd5118" | "dummy") {
            return Err(PalitRgbError::new(
                "protected_adapter",
                "Adapteren har en beskyttet I2C-klient",
            ));
        }
    }
    Ok(())
}

/// Kun metadata. Ingen `/dev`-fil åpnes, heller ikke når roten er `/`.
/// Fixtures må ha alle metadata under egen rot; ingen fallback til verten.
pub fn metadata_preview(root: &Path, pci: &str, uuid: &str) -> Result<PalitRgbMetadata> {
    if !valid_pci(pci) || !pci.ends_with(".0") || !valid_uuid(uuid) {
        return Err(PalitRgbError::new(
            "identity_required",
            "Gyldig GPU-funksjon .0 og UUID kreves",
        ));
    }
    let pci = pci.to_ascii_lowercase();
    let root = fs::canonicalize(root)
        .map_err(|e| PalitRgbError::io("invalid_root", "Finn metadata-rot", e))?;
    let pci_path = bounded_path(&root, &root.join("sys/bus/pci/devices").join(&pci))?;
    for (field, expected) in [
        ("vendor", "0x10de"),
        ("device", "0x1e87"),
        ("subsystem_vendor", "0x10de"),
        ("subsystem_device", "0x1e87"),
    ] {
        if text(&root, &pci_path.join(field))?.to_ascii_lowercase() != expected {
            return Err(PalitRgbError::new(
                "identity_mismatch",
                "PCI-identiteten er ikke gjennomgått",
            ));
        }
    }
    let driver = bounded_path(&root, &pci_path.join("driver"))?;
    let expected_driver = bounded_path(&root, &root.join("sys/bus/pci/drivers/nvidia"))?;
    if driver != expected_driver {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "Kortet bruker ikke NVIDIA-driveren",
        ));
    }
    let information = text(
        &root,
        &root
            .join("proc/driver/nvidia/gpus")
            .join(&pci)
            .join("information"),
    )?;
    if proc_field(&information, "GPU UUID")? != uuid
        || proc_field(&information, "Bus Location")?.to_ascii_lowercase() != pci
    {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "GPU UUID eller PCI er endret",
        ));
    }
    // Dette nummeret kan flytte seg. Det tas med for ny identitetskontroll,
    // men brukes aldri til kortvalg eller som en reserve til indeks null.
    let gpu_minor = proc_field(&information, "Device Minor")?
        .parse::<u32>()
        .map_err(|_| PalitRgbError::new("identity_mismatch", "Ugyldig NVIDIA-enhetsnummer"))?;
    // NVIDIA-formatet bruker %x for bussen (uten innledende null), men
    // domenet bindes alltid separat gjennom den kanoniske PCI-stien.
    let bus = u8::from_str_radix(&pci[5..7], 16)
        .map_err(|_| PalitRgbError::new("identity_required", "Ugyldig PCI-buss"))?;
    let expected_name = format!("NVIDIA i2c adapter 1 at {bus:x}:{}", &pci[8..]);
    let adapters_dir = bounded_path(&root, &root.join("sys/bus/i2c/devices"))?;
    let entries = fs::read_dir(&adapters_dir)
        .map_err(|e| PalitRgbError::io("metadata_unavailable", "Finn I2C-adaptermetadata", e))?;
    let mut found = None;
    for entry in entries {
        let entry = entry
            .map_err(|e| PalitRgbError::io("metadata_unavailable", "Les I2C-adaptermetadata", e))?;
        let name = entry.file_name();
        let Some(number) = name.to_str().and_then(|s| s.strip_prefix("i2c-")) else {
            continue;
        };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let adapter_path = bounded_path(&root, &entry.path())?;
        // Avvis andre PCI-funksjoner, SMBus, USB-C/ccgx-ucsi og underbusser
        // før navn leses. Bare adapter- og klientnavn leses videre.
        if adapter_path.parent() != Some(pci_path.as_path()) {
            continue;
        }
        if text(&root, &adapter_path.join("name"))? != expected_name {
            continue;
        }
        let bus_number: u32 = number
            .parse()
            .map_err(|_| PalitRgbError::new("invalid_metadata", "Ugyldig Linux-bussnummer"))?;
        if found.is_some() {
            return Err(PalitRgbError::new(
                "ambiguous_adapter",
                "Flere adaptere har samme kortbinding",
            ));
        }
        found = Some((adapter_path, bus_number));
    }
    let (adapter_path, bus_number) = found.ok_or_else(|| {
        PalitRgbError::new(
            "adapter_unavailable",
            "Kortets NVIDIA I2C-adapter 1 ble ikke funnet",
        )
    })?;
    absent(&adapters_dir.join(format!("{bus_number}-0049")))?;
    absent(&adapter_path.join(format!("{bus_number}-0049")))?;
    deny_protected_children(&root, &adapter_path, bus_number)?;
    let adapter_id = format!("i2c-{bus_number}");
    let class_path = bounded_path(&root, &root.join("sys/class/i2c-dev").join(&adapter_id))?;
    if !class_path.starts_with(&adapter_path) {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "I2C-enhetsklassen peker på en annen adapter",
        ));
    }
    let device_number = text(&root, &class_path.join("dev"))?;
    let (major, minor) = device_number
        .split_once(':')
        .ok_or_else(|| PalitRgbError::new("invalid_metadata", "I2C-enhetsnummer mangler"))?;
    let device_major = major.parse::<u32>().ok();
    let device_minor = minor.parse::<u32>().ok();
    if device_major != Some(I2C_MAJOR) || device_minor != Some(bus_number) {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "I2C-enhetsnummer samsvarer ikke med adapteren",
        ));
    }
    let device_path = root.join("dev").join(adapter_id);
    Ok(PalitRgbMetadata {
        root,
        pci,
        uuid: uuid.into(),
        gpu_minor,
        pci_path,
        adapter_path,
        adapter_name: expected_name,
        bus_number,
        device_path,
        device_major: I2C_MAJOR,
        device_minor: bus_number,
    })
}

fn require_physical_context(root: &Path, euid: libc::uid_t) -> Result<()> {
    if root.as_os_str() != "/" {
        return Err(PalitRgbError::new(
            "invalid_root",
            "Fysisk RGB-tilgang krever rotstien /",
        ));
    }
    if euid != 0 {
        return Err(PalitRgbError::new(
            "permission_denied",
            "Fysisk RGB-tilgang krever root-hjelperen",
        ));
    }
    Ok(())
}

/// Eksplisitt fysisk identifikasjon. Ingen reserve til GPU 0, annen UUID,
/// adapter eller adresse. Feil stopper sekvensen uten gjentakelser.
pub fn identify(root: &Path, pci: &str, uuid: &str) -> Result<Discovery> {
    // SAFETY: geteuid har ingen parametre og endrer ingen rettigheter.
    require_physical_context(root, unsafe { libc::geteuid() })?;
    identify_with(root, pci, uuid, PhysicalTransport::open)
}

fn identify_with<T: ReadTransport>(
    root: &Path,
    pci: &str,
    uuid: &str,
    open: impl FnOnce(&PalitRgbMetadata) -> Result<T>,
) -> Result<Discovery> {
    let metadata = metadata_preview(root, pci, uuid)?;
    let mut transport = open(&metadata)?;
    probe(&metadata, &mut transport)
}

// Grensesnittet har bevisst ingen skriveoperasjon eller vilkårlig adresse.
trait ReadTransport {
    fn capabilities(&mut self) -> Result<libc::c_ulong>;
    fn select_controller(&mut self) -> Result<()>;
    fn read_byte(&mut self, register: u8) -> Result<u8>;
}

// Privat og uten buss-/adressevalg. Bare apply_with bruker skrivemetoden,
// etter baseline-, identitets- og signaturkontroll. identify får kun lesetrait.
trait WriteTransport: ReadTransport {
    fn revalidate(&mut self, metadata: &PalitRgbMetadata) -> Result<()>;
    fn write_byte(&mut self, register: u8, value: u8) -> Result<()>;
}

/// Setter bare statisk farge eller av via de fem kildebelagte registerbytene.
///
/// Sikkerhetskontrakt: Baseline er ingen autorisasjon. Kalleren må kontrollere
/// brukerens RGB-tillatelse og journalføre original baseline + dirty varig
/// FØR dette kallet, også før den første skrivetransaksjonen til 0x60.
/// Kalleren må eie transaksjonslåsen og håndtere eventuell rollback separat.
/// Ingen feil utløser automatisk nye skriveforsøk her. En feil på første
/// skrivekall regnes også som mulig fysisk endring.
/// `expected_before` er siste bekreftede RGB/lysstyrke; ferske avvik gir
/// ingen skriving. Likhet beviser ikke at ekstern effektmodus er uendret.
pub fn apply(
    root: &Path,
    expected: &Binding,
    request: &Request,
    baseline: &Baseline,
    expected_before: &Observed,
) -> std::result::Result<ApplyReadback, ApplyFailure> {
    // SAFETY: geteuid er rent lesende; fixture-røtter avvises før åpning.
    require_physical_context(root, unsafe { libc::geteuid() })
        .map_err(ApplyFailure::before_write)?;
    apply_with(
        root,
        expected,
        request,
        baseline,
        expected_before,
        PhysicalTransport::open,
    )
}

fn apply_with<T: WriteTransport>(
    root: &Path,
    expected: &Binding,
    request: &Request,
    baseline: &Baseline,
    expected_before: &Observed,
    open: impl FnOnce(&PalitRgbMetadata) -> Result<T>,
) -> std::result::Result<ApplyReadback, ApplyFailure> {
    if expected_before.color.is_none() || expected_before.brightness_percent().is_none() {
        return Err(ApplyFailure::before_write(PalitRgbError::new(
            "expected_state_required",
            "Sist bekreftet farge og støttet lysstyrke kreves før endring",
        )));
    }
    execute_with(
        root,
        expected,
        request,
        baseline,
        Purpose::Apply(expected_before),
        open,
    )
}

/// Separat gjenoppretting etter mulig delvis endring. Målet må komme fra
/// hjelperens validerte, root-eide journal: opprinnelig baseline eller sist
/// bekreftede valg. En vilkårlig RPC-forespørsel må aldri velge denne veien.
/// Kalleren må beholde dirty-journalen før og under forsøket.
///
/// Nåværende farge leses ikke før gjenoppretting: en tidligere transportfeil
/// kan gjøre den ukjent. Fersk kort-/fd-/signaturbinding kreves fortsatt.
/// Resultatet beviser fortsatt bare lesbare felt, ikke aktiv effektmodus.
pub fn recover(
    root: &Path,
    expected: &Binding,
    request: &Request,
    baseline: &Baseline,
) -> std::result::Result<ApplyReadback, ApplyFailure> {
    // SAFETY: samme rent lesende rettighetskontroll som apply.
    require_physical_context(root, unsafe { libc::geteuid() })
        .map_err(ApplyFailure::before_write)?;
    execute_with(
        root,
        expected,
        request,
        baseline,
        Purpose::Recover,
        PhysicalTransport::open,
    )
}

#[derive(Clone, Copy)]
enum Purpose<'a> {
    Apply(&'a Observed),
    Recover,
}

fn execute_with<T: WriteTransport>(
    root: &Path,
    expected: &Binding,
    request: &Request,
    baseline: &Baseline,
    purpose: Purpose<'_>,
    open: impl FnOnce(&PalitRgbMetadata) -> Result<T>,
) -> std::result::Result<ApplyReadback, ApplyFailure> {
    expected
        .validate()
        .map_err(|e| ApplyFailure::before_write(PalitRgbError::new("identity_mismatch", e)))?;
    if baseline.binding() != expected {
        return Err(ApplyFailure::before_write(PalitRgbError::new(
            "baseline_mismatch",
            "Grunnverdiene tilhører ikke den forventede lyskontrolleren",
        )));
    }
    let metadata = metadata_preview(root, &expected.pci, &expected.uuid)
        .map_err(ApplyFailure::before_write)?;
    // Ingen enhetsåpning før forventet binding og validert baseline stemmer.
    let mut transport = open(&metadata).map_err(ApplyFailure::before_write)?;
    let capabilities = transport
        .capabilities()
        .map_err(ApplyFailure::before_write)?;
    if capabilities & (READ_BYTE_DATA | WRITE_BYTE_DATA) != (READ_BYTE_DATA | WRITE_BYTE_DATA) {
        return Err(ApplyFailure::before_write(PalitRgbError::new(
            "transport_unsupported",
            "Adapteren mangler SMBus byte-lesing eller byte-skriving",
        )));
    }
    transport
        .select_controller()
        .map_err(ApplyFailure::before_write)?;
    verify_signature(&metadata, expected, &mut transport).map_err(ApplyFailure::before_write)?;
    let before = match purpose {
        Purpose::Apply(expected_before) => {
            let observed = read_observed(&mut transport).map_err(ApplyFailure::before_write)?;
            if observed.color != expected_before.color
                || observed.brightness_raw != expected_before.brightness_raw
            {
                return Err(ApplyFailure::before_write(PalitRgbError::new(
                    "state_changed",
                    "Lysverdiene er endret siden sist bekreftede avlesning",
                )));
            }
            observed
        }
        Purpose::Recover => Observed::default(),
    };
    // Samme RGB/lysstyrke avslører ikke ekstern endring av effektmodus.
    // Dette er en feltbasert driftkontroll, aldri full tilstandsverifikasjon.
    // Fersk sysfs/proc-binding, originalt fd, inode og enhetsnummer; samme
    // fd beholdes gjennom hele operasjonen. Deretter leses signaturen igjen
    // umiddelbart før skriving. Dette er kontroll, aldri feil-/adresseretry.
    transport
        .revalidate(&metadata)
        .map_err(ApplyFailure::before_write)?;
    verify_signature(&metadata, expected, &mut transport).map_err(ApplyFailure::before_write)?;

    let (color, brightness) = match request {
        Request::Static { color, brightness } => (*color, brightness.get()),
        Request::Off {} => ([0, 0, 0], 100),
    };
    let writes = [
        (0x60, 0x01),
        (0x6c, color[0]),
        (0x6d, color[1]),
        (0x6e, color[2]),
        (0x6f, brightness),
    ];
    let write_count = writes.len() as u8;
    for (index, (register, value)) in writes.into_iter().enumerate() {
        // Før kallet: selv en ioctl-feil kan komme etter at byten ble sendt.
        let completed = index as u8;
        let attempted = completed + 1;
        if let Err(cause) = transport.write_byte(register, value) {
            return Err(ApplyFailure {
                cause,
                writes_attempted: attempted,
                writes_completed: completed,
                observed_after: None,
            });
        }
    }
    let after_error = |cause| ApplyFailure {
        cause,
        writes_attempted: write_count,
        writes_completed: write_count,
        observed_after: None,
    };
    transport.revalidate(&metadata).map_err(after_error)?;
    let observed = read_observed(&mut transport).map_err(after_error)?;
    if observed.color != Some(color) || observed.brightness_raw != Some(brightness) {
        return Err(ApplyFailure {
            cause: PalitRgbError::new(
                "readback_mismatch",
                "Avlest farge eller lysstyrke samsvarer ikke med forespørselen",
            ),
            writes_attempted: write_count,
            writes_completed: write_count,
            observed_after: Some(observed),
        });
    }
    Ok(ApplyReadback {
        binding: expected.clone(),
        before,
        observed,
        request_readback: RecoveryReadback::ReadableFieldsMatch,
    })
}

fn verify_signature(
    metadata: &PalitRgbMetadata,
    expected: &Binding,
    transport: &mut impl ReadTransport,
) -> Result<()> {
    let signature = read_signature(transport)?;
    if metadata.binding(signature) != *expected {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "Den ferske kontrollerbindingen er endret",
        ));
    }
    Ok(())
}

fn probe(metadata: &PalitRgbMetadata, transport: &mut impl ReadTransport) -> Result<Discovery> {
    if transport.capabilities()? & READ_BYTE_DATA == 0 {
        return Err(PalitRgbError::new(
            "transport_unsupported",
            "Adapteren støtter ikke SMBus byte-lesing",
        ));
    }
    transport.select_controller()?;
    let signature = read_signature(transport)?;
    Discovery::identified(metadata.binding(signature), read_observed(transport)?)
        .map_err(|e| PalitRgbError::new("invalid_observation", e))
}

fn read_signature(transport: &mut impl ReadTransport) -> Result<Signature> {
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = transport.read_byte(0xf0 + index as u8)?;
    }
    Signature::from_bytes(bytes).map_err(|e| {
        let mut error = PalitRgbError::new("unknown_signature", e);
        error.raw_signature = Some(bytes);
        error
    })
}

fn read_observed(transport: &mut impl ReadTransport) -> Result<Observed> {
    let red = transport.read_byte(0x6c)?;
    let green = transport.read_byte(0x6d)?;
    let blue = transport.read_byte(0x6e)?;
    let brightness = transport.read_byte(0x6f)?;
    Ok(Observed {
        color: Some([red, green, blue]),
        brightness_raw: Some(brightness),
        mode: None,
    })
}

struct PhysicalTransport {
    file: File,
    opened_node: NodeIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NodeIdentity {
    character: bool,
    filesystem: u64,
    inode: u64,
    device: u64,
}

impl NodeIdentity {
    fn from_metadata(stat: &fs::Metadata) -> Self {
        Self {
            character: stat.file_type().is_char_device(),
            filesystem: stat.dev(),
            inode: stat.ino(),
            device: stat.rdev(),
        }
    }

    fn matches_adapter(self, metadata: &PalitRgbMetadata) -> bool {
        self.character
            && libc::major(self.device) == metadata.device_major
            && libc::minor(self.device) == metadata.device_minor
    }
}

impl PhysicalTransport {
    fn open(metadata: &PalitRgbMetadata) -> Result<Self> {
        // SAFETY: se samme rent lesende rettighetskontroll i identify.
        require_physical_context(&metadata.root, unsafe { libc::geteuid() })?;
        if metadata_preview(&metadata.root, &metadata.pci, &metadata.uuid)? != *metadata {
            return Err(PalitRgbError::new(
                "identity_mismatch",
                "Kortbindingen ble endret før åpning",
            ));
        }
        // Denne eneste enhetsåpningen skjer etter komplett kortbinding.
        // O_RDWR er i2c-dev-transportens tilgangsmåte, ikke registerskriving.
        // O_NOFOLLOW avviser en utbyttet node; fstat bekrefter enhetstypen.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&metadata.device_path)
            .map_err(|e| PalitRgbError::io("device_unavailable", "Åpne valgt I2C-adapter", e))?;
        let stat = file.metadata().map_err(|e| {
            PalitRgbError::io("device_unavailable", "Kontroller åpnet I2C-enhet", e)
        })?;
        let opened_node = NodeIdentity::from_metadata(&stat);
        if !opened_node.matches_adapter(metadata) {
            return Err(PalitRgbError::new(
                "identity_mismatch",
                "Åpnet fil er ikke den valgte I2C-enheten",
            ));
        }
        if metadata_preview(&metadata.root, &metadata.pci, &metadata.uuid)? != *metadata {
            return Err(PalitRgbError::new(
                "identity_mismatch",
                "Kortbindingen ble endret under åpning",
            ));
        }
        Ok(Self { file, opened_node })
    }
}

#[repr(C)]
union SmbusData {
    byte: u8,
    word: u16,
    block: [u8; 34],
}

#[repr(C)]
struct SmbusTransfer {
    read_write: u8,
    command: u8,
    size: u32,
    data: *mut SmbusData,
}

impl ReadTransport for PhysicalTransport {
    #[allow(clippy::disallowed_methods)] // Kun den lesende I2C_FUNCS-operasjonen nedenfor.
    fn capabilities(&mut self) -> Result<libc::c_ulong> {
        let mut functions: libc::c_ulong = 0;
        // SAFETY: fd er verifisert i2c-dev; I2C_FUNCS skriver kun den lokale
        // unsigned-long-masken. Ingen adresse-/registertransaksjon startes.
        let result = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_FUNCS, &mut functions) };
        if result < 0 {
            return Err(PalitRgbError::io(
                "transport_error",
                "Les I2C-funksjoner",
                io::Error::last_os_error(),
            ));
        }
        Ok(functions)
    }

    #[allow(clippy::disallowed_methods)] // Kun lokal I2C_SLAVE-adresse, aldri FORCE.
    fn select_controller(&mut self) -> Result<()> {
        // SAFETY: I2C_SLAVE setter bare adressen for dette fildeskriptor-
        // håndtaket. Ingen FORCE; en eksisterende kjerneklient gir EBUSY.
        // Ingen I2C_TIMEOUT/RETRIES: de ville endre hele adapterens oppsett.
        let result = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                I2C_SLAVE,
                libc::c_ulong::from(ADDRESS),
            )
        };
        if result < 0 {
            return Err(PalitRgbError::io(
                "transport_error",
                "Velg kontroller 0x49",
                io::Error::last_os_error(),
            ));
        }
        Ok(())
    }

    #[allow(clippy::disallowed_methods)] // Kun fast BYTE_DATA READ og gjennomgåtte registre.
    fn read_byte(&mut self, register: u8) -> Result<u8> {
        if !matches!(register, 0xf0..=0xff | 0x6c..=0x6f) {
            return Err(PalitRgbError::new(
                "register_forbidden",
                "Registeret er ikke i det gjennomgåtte lesesettet",
            ));
        }
        let mut data = SmbusData { block: [0; 34] };
        // I2C_SMBUS_READ=1, I2C_SMBUS_BYTE_DATA=2. Registeradressen inngår
        // i SMBus-leseprotokollen; ingen verdibyte skrives til et register.
        let mut request = SmbusTransfer {
            read_write: 1,
            command: register,
            size: 2,
            data: &mut data,
        };
        // SAFETY: repr(C)-struktur og union følger linux/i2c-dev.h/i2c.h.
        // Alle pekere er gyldige gjennom det synkrone kallet; fd/adresse
        // er tidligere validert. Resultatet brukes bare ved vellykket kall.
        let result = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS, &mut request) };
        if result < 0 {
            return Err(PalitRgbError::io(
                "transport_error",
                &format!("Les register 0x{register:02x}"),
                io::Error::last_os_error(),
            ));
        }
        // SAFETY: vellykket byte-data-lesing initialiserer data.byte.
        Ok(unsafe { data.byte })
    }
}

impl WriteTransport for PhysicalTransport {
    fn revalidate(&mut self, metadata: &PalitRgbMetadata) -> Result<()> {
        // SAFETY: ingen rettighetsendring eller maskinvaretransaksjon.
        require_physical_context(&metadata.root, unsafe { libc::geteuid() })?;
        if metadata_preview(&metadata.root, &metadata.pci, &metadata.uuid)? != *metadata {
            return Err(PalitRgbError::new(
                "identity_mismatch",
                "Kortmetadata er endret under transaksjonen",
            ));
        }
        let fd_stat = self.file.metadata().map_err(|e| {
            PalitRgbError::io("device_unavailable", "Kontroller samme åpne I2C-fd", e)
        })?;
        // lstat, ikke en ny åpning. En symlink eller erstattet node avvises.
        let path_stat = fs::symlink_metadata(&metadata.device_path).map_err(|e| {
            PalitRgbError::io("device_unavailable", "Kontroller I2C-enhetsnoden", e)
        })?;
        validate_open_node(
            metadata,
            self.opened_node,
            NodeIdentity::from_metadata(&fd_stat),
            NodeIdentity::from_metadata(&path_stat),
        )
    }

    #[allow(clippy::disallowed_methods)] // Kun fem gjennomgåtte BYTE_DATA-skrivinger til valgt adresse.
    fn write_byte(&mut self, register: u8, value: u8) -> Result<()> {
        if !matches!(
            (register, value),
            (0x60, 0x01) | (0x6c..=0x6e, _) | (0x6f, 0..=100)
        ) {
            return Err(PalitRgbError::new(
                "register_forbidden",
                "Register eller verdi er utenfor statisk lysstyring",
            ));
        }
        let mut data = SmbusData { block: [0; 34] };
        data.byte = value;
        let mut request = SmbusTransfer {
            read_write: 0,
            command: register,
            size: 2,
            data: &mut data,
        };
        // SAFETY: repr(C) følger Linux-ABI. Dette er den eneste register-
        // skrivende ioctl-stien, med fast BYTE_DATA WRITE og streng liste
        // over register/verdi. Bare apply_with når den etter fersk binding
        // og signatur på dette samme fd-et. Ingen fallback eller gjentakelse.
        let result = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS, &mut request) };
        if result < 0 {
            return Err(PalitRgbError::io(
                "transport_error",
                &format!("Skriv register 0x{register:02x}"),
                io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
}

fn validate_open_node(
    metadata: &PalitRgbMetadata,
    opened: NodeIdentity,
    fd_now: NodeIdentity,
    path_now: NodeIdentity,
) -> Result<()> {
    if !opened.matches_adapter(metadata) || fd_now != opened || path_now != opened {
        return Err(PalitRgbError::new(
            "identity_mismatch",
            "Åpent I2C-fd og enhetsnode samsvarer ikke lenger",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::symlink;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    const PCI: &str = "0000:1c:00.0";
    const UUID: &str = "GPU-11111111-2222-3333-4444-555555555555";
    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        gpu: PathBuf,
        adapter: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = loop {
                let path = std::env::temp_dir().join(format!(
                    "rigg-palit-rgb-{}-{}",
                    std::process::id(),
                    NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
                ));
                match fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("Kan ikke lage isolert fixture: {e}"),
                }
            };
            let gpu = root.join("sys/devices/pci0000:00").join(PCI);
            let adapter = gpu.join("i2c-37");
            for (field, value) in [
                ("vendor", "0x10de"),
                ("device", "0x1e87"),
                ("subsystem_vendor", "0x10de"),
                ("subsystem_device", "0x1e87"),
            ] {
                put(&gpu.join(field), value);
            }
            put(&adapter.join("name"), "NVIDIA i2c adapter 1 at 1c:00.0\n");
            put(&adapter.join("i2c-dev/i2c-37/dev"), "89:37\n");
            put(
                &root
                    .join("proc/driver/nvidia/gpus")
                    .join(PCI)
                    .join("information"),
                &format!("GPU UUID: {UUID}\nBus Location: {PCI}\nDevice Minor: 0\n"),
            );
            let driver = root.join("sys/bus/pci/drivers/nvidia");
            fs::create_dir_all(&driver).unwrap();
            link(&driver, &gpu.join("driver"));
            link(&gpu, &root.join("sys/bus/pci/devices").join(PCI));
            link(&adapter, &root.join("sys/bus/i2c/devices/i2c-37"));
            link(
                &adapter.join("i2c-dev/i2c-37"),
                &root.join("sys/class/i2c-dev/i2c-37"),
            );
            // En ordinær markørfil, aldri en enhetsnode. Metadata-preview
            // skal kunne brukes selv om /dev-filen ikke er lesbar/åpnbar.
            put(&root.join("dev/i2c-37"), "ikke en maskinvareenhet");
            Self { root, gpu, adapter }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    fn put(path: &Path, value: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    fn link(target: &Path, path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(target, path).unwrap();
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Capabilities,
        Select49,
        Read(u8),
        Revalidate,
        Write(u8, u8),
    }

    struct Fake {
        events: Rc<RefCell<Vec<Event>>>,
        signature: [u8; 16],
        color: [u8; 4],
        capabilities: libc::c_ulong,
        fail_at: Option<u8>,
        select_error: bool,
        writes: usize,
        fail_write_at: Option<usize>,
        fail_readback_at: Option<u8>,
        fail_before_at: Option<u8>,
        readback_override: Option<[u8; 4]>,
        revalidations: u8,
        fail_revalidate_at: Option<u8>,
        changed_signature: Option<[u8; 16]>,
    }

    impl Fake {
        fn new(signature: Signature) -> Self {
            Self {
                events: Rc::new(RefCell::new(Vec::new())),
                signature: signature.bytes(),
                color: [12, 34, 56, 100],
                capabilities: READ_BYTE_DATA,
                fail_at: None,
                select_error: false,
                writes: 0,
                fail_write_at: None,
                fail_readback_at: None,
                fail_before_at: None,
                readback_override: None,
                revalidations: 0,
                fail_revalidate_at: None,
                changed_signature: None,
            }
        }

        fn writer(signature: Signature) -> Self {
            let mut fake = Self::new(signature);
            fake.capabilities |= WRITE_BYTE_DATA;
            fake
        }
    }

    impl ReadTransport for Fake {
        fn capabilities(&mut self) -> Result<libc::c_ulong> {
            self.events.borrow_mut().push(Event::Capabilities);
            Ok(self.capabilities)
        }

        fn select_controller(&mut self) -> Result<()> {
            self.events.borrow_mut().push(Event::Select49);
            if self.select_error {
                return Err(PalitRgbError::io(
                    "transport_error",
                    "Opptatt adresse",
                    io::Error::from_raw_os_error(libc::EBUSY),
                ));
            }
            Ok(())
        }

        fn read_byte(&mut self, register: u8) -> Result<u8> {
            self.events.borrow_mut().push(Event::Read(register));
            if self.fail_at == Some(register)
                || (self.writes > 0 && self.fail_readback_at == Some(register))
                || (self.writes == 0 && self.fail_before_at == Some(register))
            {
                return Err(PalitRgbError::io(
                    "transport_error",
                    "Simulert lesefeil",
                    io::Error::from_raw_os_error(libc::EIO),
                ));
            }
            match register {
                0xf0..=0xff => Ok(self.signature[usize::from(register - 0xf0)]),
                0x6c..=0x6f => {
                    let colors = if self.writes > 0 {
                        self.readback_override.unwrap_or(self.color)
                    } else {
                        self.color
                    };
                    Ok(colors[usize::from(register - 0x6c)])
                }
                _ => panic!("Uventet registerlesing {register:02x}"),
            }
        }
    }

    impl WriteTransport for Fake {
        fn revalidate(&mut self, _: &PalitRgbMetadata) -> Result<()> {
            self.events.borrow_mut().push(Event::Revalidate);
            self.revalidations += 1;
            if self.fail_revalidate_at == Some(self.revalidations) {
                return Err(PalitRgbError::new(
                    "identity_mismatch",
                    "Simulert endring av åpent fd",
                ));
            }
            if let Some(signature) = self.changed_signature {
                self.signature = signature;
            }
            Ok(())
        }

        fn write_byte(&mut self, register: u8, value: u8) -> Result<()> {
            self.events.borrow_mut().push(Event::Write(register, value));
            let index = self.writes;
            self.writes += 1;
            // Simuler også at kontrolleren mottok byten før ioctl-feilen.
            if matches!(register, 0x6c..=0x6f) {
                self.color[usize::from(register - 0x6c)] = value;
            }
            if self.fail_write_at == Some(index) {
                return Err(PalitRgbError::io(
                    "transport_error",
                    "Simulert skrivefeil etter sending",
                    io::Error::from_raw_os_error(libc::EIO),
                ));
            }
            Ok(())
        }
    }

    fn expected_events(registers: impl IntoIterator<Item = u8>) -> Vec<Event> {
        [Event::Capabilities, Event::Select49]
            .into_iter()
            .chain(registers.into_iter().map(Event::Read))
            .collect()
    }

    #[test]
    fn metadata_binder_til_kortet_og_ikke_bussnummeret() {
        let fixture = Fixture::new();
        fs::remove_file(fixture.root.join("dev/i2c-37")).unwrap();
        let metadata = metadata_preview(&fixture.root, PCI, UUID).unwrap();
        assert_eq!(metadata.bus_number, 37);
        assert_eq!(metadata.device_major, 89);
        assert_eq!(metadata.device_minor, 37);
        assert_eq!(metadata.adapter_path, fixture.adapter);
        assert_eq!(metadata.to_json()["registerReads"], 0);
        assert_eq!(metadata.to_json()["registerWrites"], 0);
    }

    #[test]
    fn bare_kjente_signaturer_far_fargelesing_og_modus_forblir_ukjent() {
        let fixture = Fixture::new();
        for signature in [Signature::Palit100, Signature::Palit150] {
            let fake = Fake::new(signature);
            let events = fake.events.clone();
            let report = identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap();
            assert_eq!(report.binding.unwrap().signature, signature);
            assert_eq!(report.observed.color, Some([12, 34, 56]));
            assert_eq!(report.observed.brightness_raw, Some(100));
            assert_eq!(report.observed.mode, None);
            assert_eq!(
                *events.borrow(),
                expected_events((0xf0..=0xff).chain(0x6c..=0x6f))
            );
        }
    }

    #[test]
    fn ukjent_signatur_leser_aldri_farge_eller_prover_alternativ_protokoll() {
        let fixture = Fixture::new();
        let mut fake = Fake::new(Signature::Palit150);
        fake.signature[15] = 1;
        let raw_signature = fake.signature;
        let events = fake.events.clone();
        let error = identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap_err();
        assert_eq!(error.code(), "unknown_signature");
        assert_eq!(error.to_json()["rawSignature"], json!(raw_signature));
        assert_eq!(*events.borrow(), expected_events(0xf0..=0xff));
    }

    #[test]
    fn enhver_lesefeil_stopper_straks_uten_nye_forsok() {
        let fixture = Fixture::new();
        let all_registers: Vec<u8> = (0xf0..=0xff).chain(0x6c..=0x6f).collect();
        for (index, register) in all_registers.iter().enumerate() {
            let mut fake = Fake::new(Signature::Palit150);
            fake.fail_at = Some(*register);
            let events = fake.events.clone();
            let error = identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap_err();
            assert_eq!(error.code(), "transport_error");
            assert_eq!(error.errno(), Some(libc::EIO));
            assert_eq!(
                *events.borrow(),
                expected_events(all_registers[..=index].iter().copied())
            );
        }
    }

    #[test]
    fn kapabiliteter_og_opptatt_adresse_stopper_for_registertilgang() {
        let fixture = Fixture::new();
        let mut fake = Fake::new(Signature::Palit150);
        fake.capabilities = 0;
        let events = fake.events.clone();
        let error = identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap_err();
        assert_eq!(error.code(), "transport_unsupported");
        assert_eq!(*events.borrow(), vec![Event::Capabilities]);

        let mut fake = Fake::new(Signature::Palit150);
        fake.select_error = true;
        let events = fake.events.clone();
        let error = identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap_err();
        assert_eq!(error.errno(), Some(libc::EBUSY));
        assert_eq!(*events.borrow(), expected_events([]));
    }

    #[test]
    fn ra_lysstyrke_og_svart_farge_blir_ikke_antatt_modus() {
        let fixture = Fixture::new();
        let mut fake = Fake::new(Signature::Palit150);
        fake.color = [0, 0, 0, 255];
        let report = identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap();
        assert_eq!(report.observed.brightness_raw, Some(255));
        assert_eq!(report.observed.brightness_percent(), None);
        assert_eq!(report.observed.mode, None);
    }

    fn assert_no_transport(fixture: &Fixture, expected_code: &str) {
        let error = identify_with::<Fake>(&fixture.root, PCI, UUID, |_| {
            panic!("Ugyldig metadata må avvises før transporten åpnes")
        })
        .unwrap_err();
        assert_eq!(error.code(), expected_code);
    }

    #[test]
    fn feil_kortidentitet_og_kjerneklient_avvises_for_transport() {
        let fixture = Fixture::new();
        put(&fixture.gpu.join("subsystem_vendor"), "0x1043");
        assert_no_transport(&fixture, "identity_mismatch");
        put(&fixture.gpu.join("subsystem_vendor"), "0x10de");
        let info = fixture
            .root
            .join("proc/driver/nvidia/gpus")
            .join(PCI)
            .join("information");
        put(
            &info,
            &format!("GPU UUID: GPU-feil\nBus Location: {PCI}\nDevice Minor: 0\n"),
        );
        assert_no_transport(&fixture, "identity_mismatch");
        put(
            &info,
            &format!("GPU UUID: {UUID}\nBus Location: {PCI}\nDevice Minor: 1\n"),
        );
        assert_eq!(
            metadata_preview(&fixture.root, PCI, UUID)
                .unwrap()
                .gpu_minor,
            1
        );
        put(
            &info,
            &format!("GPU UUID: {UUID}\nBus Location: {PCI}\nDevice Minor: ugyldig\n"),
        );
        assert_no_transport(&fixture, "identity_mismatch");
        put(
            &info,
            &format!("GPU UUID: {UUID}\nBus Location: {PCI}\nDevice Minor: 0\n"),
        );
        fs::create_dir(fixture.adapter.join("37-0049")).unwrap();
        assert_no_transport(&fixture, "address_busy");
    }

    #[test]
    fn usb_c_smbus_feil_port_og_enhetsnummer_avvises_for_transport() {
        let fixture = Fixture::new();
        for name in [
            "ccgx-ucsi",
            "SMBus PIIX4 adapter port 0 at 0b00",
            "NVIDIA i2c adapter 2 at 1c:00.0",
        ] {
            put(&fixture.adapter.join("name"), name);
            assert_no_transport(&fixture, "adapter_unavailable");
        }
        put(
            &fixture.adapter.join("name"),
            "NVIDIA i2c adapter 1 at 1c:00.0",
        );
        put(&fixture.adapter.join("i2c-dev/i2c-37/dev"), "89:38");
        assert_no_transport(&fixture, "identity_mismatch");
        put(&fixture.adapter.join("i2c-dev/i2c-37/dev"), "89:37");
        let wrong_parent = fixture.gpu.with_file_name("0000:1c:00.3").join("i2c-37");
        fs::create_dir_all(wrong_parent.parent().unwrap()).unwrap();
        fs::rename(&fixture.adapter, &wrong_parent).unwrap();
        let adapter_link = fixture.root.join("sys/bus/i2c/devices/i2c-37");
        fs::remove_file(&adapter_link).unwrap();
        link(&wrong_parent, &adapter_link);
        assert_no_transport(&fixture, "adapter_unavailable");
    }

    #[test]
    fn beskyttede_klientnavn_avvises_selv_pa_riktig_adapter() {
        let fixture = Fixture::new();
        for name in ["ccgx-ucsi", "ee1004", "spd5118", "dummy"] {
            put(&fixture.adapter.join("37-0008/name"), name);
            assert_no_transport(&fixture, "protected_adapter");
        }
    }

    #[test]
    fn fixture_ma_aldri_falle_tilbake_til_vertens_metadata() {
        let fixture = Fixture::new();
        let other = Fixture::new();
        let adapter_link = fixture.root.join("sys/bus/i2c/devices/i2c-37");
        fs::remove_file(&adapter_link).unwrap();
        link(&other.adapter, &adapter_link);
        assert_no_transport(&fixture, "invalid_root");
    }

    #[test]
    fn fysisk_tilgang_krever_eksakt_rot_og_root_uavhengig_av_testbruker() {
        // Ingen test kaller identify("/"). Kontrollen testes med injisert
        // UID; CI som root kan derfor ikke nå vertens maskinvare.
        assert_eq!(
            require_physical_context(Path::new("/"), 1000)
                .unwrap_err()
                .code(),
            "permission_denied"
        );
        assert_eq!(
            require_physical_context(Path::new("/tmp"), 0)
                .unwrap_err()
                .code(),
            "invalid_root"
        );
        assert_eq!(
            require_physical_context(Path::new("/./"), 0)
                .unwrap_err()
                .code(),
            "invalid_root"
        );
        let fixture = Fixture::new();
        assert_eq!(
            identify(&fixture.root, PCI, UUID).unwrap_err().code(),
            "invalid_root"
        );
    }

    fn write_context(fixture: &Fixture, signature: Signature) -> (Binding, Baseline) {
        let binding = metadata_preview(&fixture.root, PCI, UUID)
            .unwrap()
            .binding(signature);
        let baseline = Baseline::user_confirmed_static(
            binding.clone(),
            Observed {
                color: Some([12, 34, 56]),
                brightness_raw: Some(100),
                mode: None,
            },
        )
        .unwrap();
        (binding, baseline)
    }

    fn static_request(brightness: u8) -> Request {
        Request::Static {
            color: [0, 127, 255],
            brightness: brightness.try_into().unwrap(),
        }
    }

    fn expected_before_writes() -> Vec<Event> {
        let mut events = expected_events((0xf0..=0xff).chain(0x6c..=0x6f));
        events.push(Event::Revalidate);
        events.extend((0xf0..=0xff).map(Event::Read));
        events
    }

    #[test]
    fn statisk_skriver_fem_byte_pa_samme_transport_og_bekrefter_bare_feltene() {
        let fixture = Fixture::new();
        for signature in [Signature::Palit100, Signature::Palit150] {
            let (binding, baseline) = write_context(&fixture, signature);
            for brightness in [0, 100] {
                let fake = Fake::writer(signature);
                let events = fake.events.clone();
                let opens = std::cell::Cell::new(0);
                let report = apply_with(
                    &fixture.root,
                    &binding,
                    &static_request(brightness),
                    &baseline,
                    baseline.observed(),
                    |_| {
                        opens.set(opens.get() + 1);
                        Ok(fake)
                    },
                )
                .unwrap();
                let mut expected = expected_before_writes();
                expected.extend(
                    [
                        (0x60, 1),
                        (0x6c, 0),
                        (0x6d, 127),
                        (0x6e, 255),
                        (0x6f, brightness),
                    ]
                    .into_iter()
                    .map(|(r, v)| Event::Write(r, v)),
                );
                expected.push(Event::Revalidate);
                expected.extend((0x6c..=0x6f).map(Event::Read));
                assert_eq!(*events.borrow(), expected);
                assert_eq!(opens.get(), 1);
                assert_eq!(report.before.color, Some([12, 34, 56]));
                assert_eq!(report.observed.color, Some([0, 127, 255]));
                assert_eq!(report.observed.brightness_raw, Some(brightness));
                assert_eq!(report.observed.mode, None);
                assert_eq!(
                    report.request_readback,
                    RecoveryReadback::ReadableFieldsMatch
                );
                assert_eq!(report.to_json()["modeVerified"], false);
            }
        }
    }

    #[test]
    fn av_bruker_kjent_statisk_svart_med_lysstyrke_hundre() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        let fake = Fake::writer(Signature::Palit150);
        let events = fake.events.clone();
        let report = apply_with(
            &fixture.root,
            &binding,
            &Request::Off {},
            &baseline,
            baseline.observed(),
            |_| Ok(fake),
        )
        .unwrap();
        let writes: Vec<_> = events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Write(r, v) => Some((*r, *v)),
                _ => None,
            })
            .collect();
        assert_eq!(
            writes,
            [(0x60, 1), (0x6c, 0), (0x6d, 0), (0x6e, 0), (0x6f, 100)]
        );
        assert_eq!(report.observed.color, Some([0, 0, 0]));
        assert_eq!(report.observed.mode, None);
    }

    #[test]
    fn hver_skrivefeil_stopper_og_forste_feil_er_ogsa_mulig_fysisk_endring() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        let writes = [(0x60, 1), (0x6c, 0), (0x6d, 127), (0x6e, 255), (0x6f, 100)];
        for fail_index in 0..5 {
            let mut fake = Fake::writer(Signature::Palit150);
            fake.fail_write_at = Some(fail_index);
            let events = fake.events.clone();
            let error = apply_with(
                &fixture.root,
                &binding,
                &static_request(100),
                &baseline,
                baseline.observed(),
                |_| Ok(fake),
            )
            .unwrap_err();
            let mut expected = expected_before_writes();
            expected.extend(
                writes[..=fail_index]
                    .iter()
                    .map(|(r, v)| Event::Write(*r, *v)),
            );
            assert_eq!(*events.borrow(), expected);
            assert_eq!(error.writes_attempted, (fail_index + 1) as u8);
            assert_eq!(error.writes_completed, fail_index as u8);
            assert!(error.hardware_state_unknown());
            assert!(error.to_string().contains("delvis endret"));
            assert_eq!(error.cause.errno(), Some(libc::EIO));
            assert_eq!(error.observed_after, None);
        }
    }

    #[test]
    fn baseline_og_rot_avvises_for_skrivetransport_kan_apnes() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        let mut other_binding = binding.clone();
        other_binding.uuid = "GPU-annet-kort".into();
        let error = apply_with::<Fake>(
            &fixture.root,
            &other_binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
            |_| panic!("Ulik baseline må avvises før åpning"),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "baseline_mismatch");
        assert!(!error.hardware_state_unknown());
        // Ingen test bruker apply med fysisk rot /. Også root-CI forblir isolert.
        let error = apply(
            &fixture.root,
            &binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "invalid_root");
        assert_eq!(error.writes_attempted, 0);
        put(&fixture.gpu.join("vendor"), "0x1002");
        let error = apply_with::<Fake>(
            &fixture.root,
            &binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
            |_| panic!("Endret PCI-identitet må avvises før åpning"),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "identity_mismatch");
    }

    #[test]
    fn manglende_skrivekapabilitet_gir_ingen_registertilgang() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        let fake = Fake::new(Signature::Palit150);
        let events = fake.events.clone();
        let error = apply_with(
            &fixture.root,
            &binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
            |_| Ok(fake),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "transport_unsupported");
        assert_eq!(*events.borrow(), [Event::Capabilities]);
        assert_eq!(error.writes_attempted, 0);
    }

    #[test]
    fn fersk_signatur_og_fd_kontrolleres_for_forste_skrivebyte() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        for initial_wrong in [true, false] {
            let mut fake = Fake::writer(Signature::Palit150);
            if initial_wrong {
                fake.signature = Signature::Palit100.bytes();
            } else {
                fake.changed_signature = Some(Signature::Palit100.bytes());
            }
            let events = fake.events.clone();
            let error = apply_with(
                &fixture.root,
                &binding,
                &static_request(100),
                &baseline,
                baseline.observed(),
                |_| Ok(fake),
            )
            .unwrap_err();
            assert_eq!(error.cause.code(), "identity_mismatch");
            assert_eq!(error.writes_attempted, 0);
            assert!(
                !events
                    .borrow()
                    .iter()
                    .any(|e| matches!(e, Event::Write(_, _)))
            );
        }
        let mut fake = Fake::writer(Signature::Palit150);
        fake.fail_revalidate_at = Some(1);
        let events = fake.events.clone();
        let error = apply_with(
            &fixture.root,
            &binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
            |_| Ok(fake),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "identity_mismatch");
        assert_eq!(events.borrow().last(), Some(&Event::Revalidate));
        assert_eq!(error.writes_attempted, 0);
    }

    #[test]
    fn feil_under_etterkontroll_beholder_mulig_endring_og_gjor_ingen_rollback() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        for fail_register in 0x6c..=0x6f {
            let mut fake = Fake::writer(Signature::Palit150);
            fake.fail_readback_at = Some(fail_register);
            let events = fake.events.clone();
            let error = apply_with(
                &fixture.root,
                &binding,
                &static_request(100),
                &baseline,
                baseline.observed(),
                |_| Ok(fake),
            )
            .unwrap_err();
            assert_eq!(error.writes_attempted, 5);
            assert_eq!(error.writes_completed, 5);
            assert!(error.hardware_state_unknown());
            assert_eq!(events.borrow().last(), Some(&Event::Read(fail_register)));
            assert_eq!(
                events
                    .borrow()
                    .iter()
                    .filter(|e| matches!(e, Event::Write(_, _)))
                    .count(),
                5
            );
        }
        let mut fake = Fake::writer(Signature::Palit150);
        fake.fail_revalidate_at = Some(2);
        let events = fake.events.clone();
        let error = apply_with(
            &fixture.root,
            &binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
            |_| Ok(fake),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "identity_mismatch");
        assert!(error.hardware_state_unknown());
        assert_eq!(events.borrow().last(), Some(&Event::Revalidate));
    }

    #[test]
    fn avvikende_readback_er_feil_og_ra_lysstyrke_blir_ikke_klippet() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        let mut fake = Fake::writer(Signature::Palit150);
        fake.readback_override = Some([0, 127, 255, 200]);
        let error = apply_with(
            &fixture.root,
            &binding,
            &static_request(100),
            &baseline,
            baseline.observed(),
            |_| Ok(fake),
        )
        .unwrap_err();
        assert_eq!(error.cause.code(), "readback_mismatch");
        let observed = error.observed_after.as_ref().unwrap();
        assert_eq!(observed.brightness_raw, Some(200));
        assert_eq!(observed.brightness_percent(), None);
        assert_eq!(observed.mode, None);
        assert!(error.hardware_state_unknown());
    }

    #[test]
    fn samme_enhetsnummer_er_ikke_nok_hvis_inode_eller_filtype_er_endret() {
        let fixture = Fixture::new();
        let metadata = metadata_preview(&fixture.root, PCI, UUID).unwrap();
        let original = NodeIdentity {
            character: true,
            filesystem: 7,
            inode: 123,
            device: libc::makedev(89, 37),
        };
        assert!(validate_open_node(&metadata, original, original, original).is_ok());
        for changed in [
            NodeIdentity {
                inode: 124,
                ..original
            },
            NodeIdentity {
                filesystem: 8,
                ..original
            },
            NodeIdentity {
                character: false,
                ..original
            },
            NodeIdentity {
                device: libc::makedev(89, 38),
                ..original
            },
        ] {
            assert!(validate_open_node(&metadata, original, original, changed).is_err());
            assert!(validate_open_node(&metadata, original, changed, original).is_err());
        }
    }

    #[test]
    fn identify_forblir_uten_skriving_selv_om_transporten_kan_skrive() {
        let fixture = Fixture::new();
        let fake = Fake::writer(Signature::Palit150);
        let events = fake.events.clone();
        identify_with(&fixture.root, PCI, UUID, |_| Ok(fake)).unwrap();
        assert_eq!(
            *events.borrow(),
            expected_events((0xf0..=0xff).chain(0x6c..=0x6f))
        );
    }

    #[test]
    fn fersk_farge_og_lysstyrke_ma_stemme_med_sist_bekreftede_verdier() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        for expected_before in [
            Observed {
                color: Some([99, 34, 56]),
                ..baseline.observed().clone()
            },
            Observed {
                brightness_raw: Some(25),
                ..baseline.observed().clone()
            },
        ] {
            let fake = Fake::writer(Signature::Palit150);
            let events = fake.events.clone();
            let error = apply_with(
                &fixture.root,
                &binding,
                &static_request(100),
                &baseline,
                &expected_before,
                |_| Ok(fake),
            )
            .unwrap_err();
            assert_eq!(error.cause.code(), "state_changed");
            assert_eq!(error.writes_attempted, 0);
            assert!(
                !events
                    .borrow()
                    .iter()
                    .any(|e| matches!(e, Event::Write(_, _)))
            );
        }
        for expected_before in [
            Observed::default(),
            Observed {
                brightness_raw: Some(200),
                ..baseline.observed().clone()
            },
        ] {
            let error = apply_with::<Fake>(
                &fixture.root,
                &binding,
                &static_request(100),
                &baseline,
                &expected_before,
                |_| panic!("Ugyldig sisteavlesning skal avvises før åpning"),
            )
            .unwrap_err();
            assert_eq!(error.cause.code(), "expected_state_required");
        }
    }

    #[test]
    fn gjenoppretting_kan_forsokes_selv_om_naavaerende_farge_er_ukjent() {
        let fixture = Fixture::new();
        let (binding, baseline) = write_context(&fixture, Signature::Palit150);
        let mut fake = Fake::writer(Signature::Palit150);
        fake.color = [200, 150, 25, 255];
        fake.fail_before_at = Some(0x6c);
        let events = fake.events.clone();
        // Her representerer typed request et tidligere bekreftet valg fra
        // journalen, ikke nødvendigvis opprinnelig baseline.
        let report = execute_with(
            &fixture.root,
            &binding,
            &static_request(25),
            &baseline,
            Purpose::Recover,
            |_| Ok(fake),
        )
        .unwrap();
        assert_eq!(report.before, Observed::default());
        assert_eq!(report.observed.color, Some([0, 127, 255]));
        assert_eq!(report.observed.brightness_raw, Some(25));
        assert_eq!(
            report.request_readback,
            RecoveryReadback::ReadableFieldsMatch
        );
        let events = events.borrow();
        let first_write = events
            .iter()
            .position(|e| matches!(e, Event::Write(_, _)))
            .unwrap();
        assert!(
            !events[..first_write]
                .iter()
                .any(|e| matches!(e, Event::Read(0x6c..=0x6f)))
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Write(_, _)))
                .count(),
            5
        );
        assert_eq!(
            recover(&fixture.root, &binding, &static_request(25), &baseline)
                .unwrap_err()
                .cause
                .code(),
            "invalid_root"
        );
    }
}
