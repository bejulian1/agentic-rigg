//! Eksplisitt, kortbundet og bare lesende Palit-identifikasjon.
//!
//! Ingen kode her kjører ved modulimport eller vanlig GPU-innsamling.
//! `metadata_preview` leser bare sysfs/proc. Bare `identify`, kalt eksplisitt
//! av root-hjelperen, kan åpne den ene validerte I2C-adapteren. Det finnes
//! ingen registerskriving, adresse-/registerskanning eller alternativ buss.
//!
//! Transport: Linux i2c-dev + NVIDIA 610.43.02 `nvidia/nv-i2c.c`.
//! Lesesett: ThunderMaster-opptak fra OpenRGB-sakene 1420 og 1938:
//! signatur F0–FF, deretter 6C–6F når signaturen er kjent. Ingen lesing av
//! ukjente modusregistre; nåværende modus og full reverserbarhet er ukjent.

use rigg_core::hjelper::{valid_pci, valid_uuid};
use rigg_core::rgb::{AdapterIdentity, Binding, Discovery, Observed, PciIdentity, Signature};
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
        let Some(address) = entry_name.to_str().and_then(|s| s.strip_prefix(prefix.as_str())) else {
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
            "Fysisk identifikasjon krever rotstien /",
        ));
    }
    if euid != 0 {
        return Err(PalitRgbError::new(
            "permission_denied",
            "Fysisk identifikasjon krever root-hjelperen",
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

fn probe(metadata: &PalitRgbMetadata, transport: &mut impl ReadTransport) -> Result<Discovery> {
    if transport.capabilities()? & READ_BYTE_DATA == 0 {
        return Err(PalitRgbError::new(
            "transport_unsupported",
            "Adapteren støtter ikke SMBus byte-lesing",
        ));
    }
    transport.select_controller()?;
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = transport.read_byte(0xf0 + index as u8)?;
    }
    let signature = Signature::from_bytes(bytes).map_err(|e| {
        let mut error = PalitRgbError::new("unknown_signature", e);
        error.raw_signature = Some(bytes);
        error
    })?;
    let red = transport.read_byte(0x6c)?;
    let green = transport.read_byte(0x6d)?;
    let blue = transport.read_byte(0x6e)?;
    let brightness = transport.read_byte(0x6f)?;
    Discovery::identified(
        metadata.binding(signature),
        Observed {
            color: Some([red, green, blue]),
            brightness_raw: Some(brightness),
            mode: None,
        },
    )
    .map_err(|e| PalitRgbError::new("invalid_observation", e))
}

struct PhysicalTransport {
    file: File,
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
        if !stat.file_type().is_char_device()
            || libc::major(stat.rdev()) != metadata.device_major
            || libc::minor(stat.rdev()) != metadata.device_minor
        {
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
        Ok(Self { file })
    }
}

#[repr(C)]
union SmbusData {
    byte: u8,
    word: u16,
    block: [u8; 34],
}

#[repr(C)]
struct SmbusRead {
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
        let mut request = SmbusRead {
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
    }

    struct Fake {
        events: Rc<RefCell<Vec<Event>>>,
        signature: [u8; 16],
        color: [u8; 4],
        capabilities: libc::c_ulong,
        fail_at: Option<u8>,
        select_error: bool,
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
            }
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
            if self.fail_at == Some(register) {
                return Err(PalitRgbError::io(
                    "transport_error",
                    "Simulert lesefeil",
                    io::Error::from_raw_os_error(libc::EIO),
                ));
            }
            match register {
                0xf0..=0xff => Ok(self.signature[usize::from(register - 0xf0)]),
                0x6c..=0x6f => Ok(self.color[usize::from(register - 0x6c)]),
                _ => panic!("Uventet registerlesing {register:02x}"),
            }
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
}
