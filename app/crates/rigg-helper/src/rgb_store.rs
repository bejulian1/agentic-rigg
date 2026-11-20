//! Privat RGB-grunnopptak og skrivejournal. Ingen maskinvaretilgang.
//! En journal ved oppstart krever særskilt avklaring, også i Prepared-fasen.

use rigg_core::rgb::{Baseline, BaselineSource, Binding, Observed, Request};
use rigg_core::rgb_auth::{self, Authorization};
use rigg_core::rgb_txn::{Book, Journal};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const STATE_DIR: &str = "/var/lib/agentic-rigg-rgb";
const BASELINE: &str = "baseline.json";
const JOURNAL: &str = "journal.json";
const VISUAL: &str = "visual-confirmation.json";
const LIMIT: u64 = 65_536;
static TEMP_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn new_recovery_id() -> Result<String, String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| format!("Kunne ikke opprette RGB-identitet: {error}"))?;
    Ok(format!("{:032x}", u128::from_ne_bytes(bytes)))
}

pub(crate) fn valid_recovery_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Prepared,
    Attempted,
    RejectedBeforeWrite,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Operation {
    Apply,
    Recover,
}

/// `prior_write_responsibility` gjelder tidligere egne setterforsøk. Prepared
/// sier ingenting om et eventuelt setterkall mellom siste fsync og et krasj.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredJournal {
    pub version: u32,
    pub journal: Journal,
    pub operation: Operation,
    pub target: Request,
    pub phase: Phase,
    pub prior_write_responsibility: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_id: Option<String>,
}

impl StoredJournal {
    pub fn validate(&self, baseline: &Baseline) -> Result<(), String> {
        if self.version != 1
            || &self.journal.baseline != baseline
            || self
                .recovery_id
                .as_deref()
                .is_some_and(|id| !valid_recovery_id(id))
        {
            return Err("RGB-journalens format eller opprinnelige grunnopptak er endret.".into());
        }
        self.journal.validate(baseline.binding())?;
        let initial = Book::new(baseline.clone())
            .map_err(|e| e.to_string())?
            .snapshot()
            .confirmed;
        let valid_target = match self.operation {
            Operation::Apply => self.target == self.journal.requested,
            Operation::Recover => self.target == self.journal.before || self.target == initial,
        };
        if !valid_target {
            return Err("RGB-journalens mål mangler tilhørende bekreftet tilstand.".into());
        }
        Ok(())
    }

    /// Bare registrert ansvar, ikke en konklusjon om hva som kan ha skjedd
    /// etter en Prepared-journal. `false` beviser aldri urørt maskinvare.
    pub fn has_recorded_write_responsibility(&self) -> bool {
        self.prior_write_responsibility || self.phase == Phase::Attempted
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum EvidenceSource {
    UserVisual,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EvidenceContext {
    LiveRecovery,
    RestartBaseline,
}

/// Én siste kvittering, ikke en voksende logg. Effektmodus holdes ukjent;
/// dette dokumenterer menneskets bekreftelse sammen med ferske lesbare felt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VisualEvidence {
    version: u32,
    source: EvidenceSource,
    context: EvidenceContext,
    binding: Binding,
    uid: u32,
    journal_owner: u64,
    journal_txn: u64,
    target: Request,
    observed: Observed,
    original_baseline: bool,
}

pub(crate) fn fields_match(target: &Request, observed: &Observed) -> bool {
    let (color, brightness) = match target {
        Request::Static { color, brightness } => (*color, brightness.get()),
        Request::Off {} => ([0, 0, 0], 100),
    };
    observed.color == Some(color)
        && observed.brightness_raw == Some(brightness)
        && observed.mode.is_none()
}

fn initial_request(baseline: &Baseline) -> Result<Request, String> {
    Ok(Book::new(baseline.clone())
        .map_err(|e| e.to_string())?
        .snapshot()
        .confirmed)
}

impl VisualEvidence {
    pub fn new(
        uid: u32,
        baseline: &Baseline,
        record: &StoredJournal,
        target: &Request,
        observed: &Observed,
        context: EvidenceContext,
    ) -> Result<Self, String> {
        let evidence = Self {
            version: 1,
            source: EvidenceSource::UserVisual,
            context,
            binding: baseline.binding().clone(),
            uid,
            journal_owner: record.journal.owner,
            journal_txn: record.journal.txn,
            target: target.clone(),
            observed: observed.clone(),
            original_baseline: target == &initial_request(baseline)?,
        };
        evidence.validate_record(baseline, record)?;
        Ok(evidence)
    }

    fn validate(&self, baseline: &Baseline) -> Result<(), String> {
        if self.version != 1
            || self.uid == 0
            || self.uid == u32::MAX
            || self.journal_owner == 0
            || self.journal_txn == 0
            || &self.binding != baseline.binding()
            || !fields_match(&self.target, &self.observed)
            || self.original_baseline != (self.target == initial_request(baseline)?)
            || (self.context == EvidenceContext::RestartBaseline && !self.original_baseline)
        {
            return Err(
                "Ugyldig visuell RGB-bekreftelse; avlesning, mål og binding må samsvare.".into(),
            );
        }
        Ok(())
    }

    fn validate_record(&self, baseline: &Baseline, record: &StoredJournal) -> Result<(), String> {
        self.validate(baseline)?;
        record.validate(baseline)?;
        if self.journal_owner != record.journal.owner
            || self.journal_txn != record.journal.txn
            || (self.context == EvidenceContext::LiveRecovery
                && (record.operation != Operation::Recover
                    || record.phase != Phase::Attempted
                    || self.target != record.target))
        {
            return Err(
                "Den visuelle RGB-bekreftelsen tilhører ikke denne gjenopprettingen.".into(),
            );
        }
        Ok(())
    }
}

/// Lastes også ved tilbakekallingskontroll. Ingen klientstyrt sti brukes av
/// produksjonsserveren: den sender alltid rgb_auth::CONFIG_PATH.
pub(crate) fn load_authorization(path: &Path) -> Result<Authorization, String> {
    load_authorization_owned(path, 0, true)
}

/// Bare den allerede kjente, strenge diskmodellen brukes til administrativ
/// gjenoppretting. Byggnummeret sperrer fortsatt enhver live-arbeider.
pub(crate) fn load_recovery_authorization() -> Result<Authorization, String> {
    read_authorization_owned(Path::new(rgb_auth::CONFIG_PATH), 0, true)
}

/// Bare eksplisitt administratoraktivering etter hjelperens egen ferske
/// identifikasjon og menneskelig bekreftelse. Ingen automatisk capture her.
/// Baseline skrives før tillatelsen. En tidligere ren økt arkiveres ved
/// endret utgangspunkt; en eksisterende tillatelse beholder samme baseline.
pub(crate) fn prepare_production(auth: &Authorization, baseline: &Baseline) -> Result<(), String> {
    require_root()?;
    ensure_production_directory(Path::new(STATE_DIR), true)?;
    let store = PrivateStore::open_production()?;
    let parent = Path::new(rgb_auth::CONFIG_PATH)
        .parent()
        .ok_or("RGB-tillatelsen mangler mappe.")?;
    ensure_production_directory(parent, false)?;
    let dir = open_directory(parent, 0, true, false)?;
    prepare_documents(auth, baseline, &store, parent, &dir)
}

fn prepare_documents(
    auth: &Authorization,
    baseline: &Baseline,
    store: &PrivateStore,
    auth_parent: &Path,
    auth_directory: &File,
) -> Result<(), String> {
    if baseline.source() != BaselineSource::UserConfirmedStatic
        || !rgb_auth::compatible_build(auth.build())
        || !auth.allows(auth.uid(), baseline.binding())
    {
        return Err(
            "RGB-forberedelse krever separat tillatelse og brukerbekreftet statisk original."
                .into(),
        );
    }
    // Kontroller eksisterende tillatelse FØR en ny original eventuelt lagres.
    // En annen brukers/korts oppsett skal ikke etterlate en feil baseline.
    let check_auth = || -> Result<Option<Authorization>, String> {
        let current_dir = open_directory(auth_parent, store.owner, store.trusted_ancestors, false)?;
        let old = auth_directory.metadata().map_err(|e| e.to_string())?;
        let current = current_dir.metadata().map_err(|e| e.to_string())?;
        if (old.dev(), old.ino()) != (current.dev(), current.ino()) {
            return Err("RGB-tillatelsens mappe er byttet.".into());
        }
        let current = read_private(auth_directory, "rgb.json", store.owner)?
            .map(|text| Authorization::parse(&text))
            .transpose()?;
        if let Some(old) = &current
            && (old.uid() != auth.uid() || old.binding() != auth.binding())
        {
            return Err(
                "Eksisterende RGB-tillatelse tilhører en annen bruker eller et annet kort.".into(),
            );
        }
        Ok(current)
    };
    let previous_auth = check_auth()?;
    if read_private(&store.directory, JOURNAL, store.owner)?.is_some() {
        return Err("RecoveryRequired: eksisterende RGB-journal må avklares særskilt.".into());
    }
    let previous_baseline = store.baseline_optional()?;
    let invalid_receipt = previous_auth.is_none()
        && previous_baseline
            .as_ref()
            .is_some_and(|previous| store.visual_evidence(previous).is_err());
    if previous_baseline.as_ref() != Some(baseline) || invalid_receipt {
        if let Some(existing) = &previous_baseline {
            if previous_auth.is_some() {
                return Err(
                    "Slå av den rene RGB-tilgangen før et nytt utgangspunkt kan lagres.".into(),
                );
            }
            store.archive_baseline(existing)?;
        }
        let bytes = serde_json::to_vec(baseline).map_err(|e| e.to_string())?;
        store.replace(BASELINE, &bytes, || {
            store.revalidate()?;
            if store.baseline_optional()?.is_some()
                || read_private(&store.directory, JOURNAL, store.owner)?.is_some()
                || check_auth()? != previous_auth
            {
                return Err("RGB-tilstanden endret seg under forberedelsen.".into());
            }
            Ok(())
        })?;
    }
    let check = || {
        store.validate_state(baseline, None)?;
        if check_auth()? != previous_auth {
            return Err("RGB-tillatelsen ble endret under forberedelsen.".into());
        }
        Ok(())
    };
    check()?;
    replace_private(
        auth_directory,
        store.owner,
        "rgb.json",
        &serde_json::to_vec(auth).map_err(|e| e.to_string())?,
        check,
        || Ok(()),
    )
}

fn require_root() -> Result<(), String> {
    // SAFETY: ren prosessidentitet, ingen endring av privilegier.
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err(
            "RGB-forberedelse og restartavklaring krever eksplisitt administratoraktivering."
                .into(),
        )
    }
}

fn ensure_production_directory(path: &Path, private: bool) -> Result<(), String> {
    let parent = path.parent().ok_or("RGB-mappen mangler forelder.")?;
    let parent_fd = open_directory(parent, 0, true, false)?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("Ugyldig RGB-mappenavn.")?;
    let name = filename(name).map_err(|e| e.to_string())?;
    // SAFETY: fast enkeltkomponent under verifisert root-eid mappe. EEXIST
    // følges alltid av ny no-follow kontroll, aldri chmod av eksisterende sti.
    if unsafe { libc::mkdirat(parent_fd.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(format!("Opprett privat RGB-mappe: {error}"));
        }
    } else {
        parent_fd.sync_all().map_err(|e| e.to_string())?;
    }
    open_directory(path, 0, true, private).map(|_| ())
}

fn load_authorization_owned(
    path: &Path,
    owner: u32,
    trusted_ancestors: bool,
) -> Result<Authorization, String> {
    let auth = read_authorization_owned(path, owner, trusted_ancestors)?;
    if !rgb_auth::compatible_build(auth.build()) {
        return Err("RGB-tillatelsen gjelder et inkompatibelt hjelperbygg.".into());
    }
    Ok(auth)
}

fn read_authorization_owned(
    path: &Path,
    owner: u32,
    trusted_ancestors: bool,
) -> Result<Authorization, String> {
    let parent = path.parent().ok_or("RGB-tillatelsen mangler mappe.")?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Ugyldig filnavn for RGB-tillatelsen.")?;
    let dir = open_directory(parent, owner, trusted_ancestors, false)?;
    let text = read_private(&dir, name, owner)?.ok_or("RGB-tillatelsen mangler.")?;
    Authorization::parse(&text)
}

/// Produksjon bruker en fast root-eid mappe. Tester må uttrykkelig velge en
/// separat privat mappe; det finnes ingen fallback til vertens tilstand.
pub(crate) struct PrivateStore {
    path: PathBuf,
    directory: File,
    lock: File,
    owner: u32,
    trusted_ancestors: bool,
    #[cfg(test)]
    pub fail_before_rename: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(test)]
    pub fail_directory_sync: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pub fail_sync_after_unlink: std::sync::atomic::AtomicBool,
}

impl PrivateStore {
    pub fn open_production() -> Result<Self, String> {
        // SAFETY: geteuid leser prosessens identitet uten sideeffekter.
        if unsafe { libc::geteuid() } != 0 {
            return Err("RGB-journalen krever en separat administratoraktivert hjelper.".into());
        }
        Self::open(Path::new(STATE_DIR), 0, true)
    }

    fn open(path: &Path, owner: u32, trusted_ancestors: bool) -> Result<Self, String> {
        let directory = open_directory(path, owner, trusted_ancestors, true)?;
        let lock = open_at(
            &directory,
            "worker.lock",
            libc::O_RDWR | libc::O_CREAT,
            0o600,
        )
        .map_err(|e| format!("Åpne RGB-lås: {e}"))?;
        validate_file(&lock, owner)?;
        // SAFETY: flock gjelder bare vår verifiserte private låsefil. LOCK_NB
        // avviser en annen arbeider straks; filen holdes åpen hele levetiden.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(format!(
                "RGB-arbeideren er allerede låst: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self {
            path: path.to_owned(),
            directory,
            lock,
            owner,
            trusted_ancestors,
            #[cfg(test)]
            fail_before_rename: Default::default(),
            #[cfg(test)]
            fail_directory_sync: Default::default(),
            #[cfg(test)]
            fail_sync_after_unlink: Default::default(),
        })
    }

    #[cfg(test)]
    pub fn open_fixture(path: &Path) -> Result<Self, String> {
        // SAFETY: testene bruker kun den uttrykkelig opprettede tempmappen.
        Self::open(path, unsafe { libc::geteuid() }, false)
    }

    fn revalidate(&self) -> Result<(), String> {
        let current = open_directory(&self.path, self.owner, self.trusted_ancestors, true)?;
        let original = self.directory.metadata().map_err(|e| e.to_string())?;
        let current = current.metadata().map_err(|e| e.to_string())?;
        if (original.dev(), original.ino()) != (current.dev(), current.ino()) {
            return Err("RGB-tilstandsmappen er byttet under økten.".into());
        }
        let lock = open_at(&self.directory, "worker.lock", libc::O_RDONLY, 0)
            .map_err(|e| e.to_string())?;
        validate_file(&lock, self.owner)?;
        let original = self.lock.metadata().map_err(|e| e.to_string())?;
        let current = lock.metadata().map_err(|e| e.to_string())?;
        if (original.dev(), original.ino()) != (current.dev(), current.ino()) {
            return Err("RGB-låsefilen er byttet under økten.".into());
        }
        Ok(())
    }

    pub fn baseline(&self) -> Result<Baseline, String> {
        self.baseline_optional()?
            .ok_or_else(|| "RGB mangler et særskilt lagret, brukerbekreftet grunnopptak.".into())
    }

    fn baseline_optional(&self) -> Result<Option<Baseline>, String> {
        self.revalidate()?;
        read_private(&self.directory, BASELINE, self.owner)?
            .map(|text| {
                serde_json::from_str(&text).map_err(|e| format!("Ugyldig RGB-grunnopptak: {e}"))
            })
            .transpose()
    }

    /// Under arbeiderlåsen, med ren journal og uten en aktiv tillatelse.
    /// Kvitteringen flyttes først. Hvis det avbrytes før baseline flyttes,
    /// vil samme ledige baseline-nummer brukes ved neste forsøk.
    fn archive_baseline(&self, baseline: &Baseline) -> Result<(), String> {
        self.validate_state(baseline, None)?;
        for number in 1..=u64::MAX {
            let archived = format!("baseline-{number}.json");
            if read_private(&self.directory, &archived, self.owner)?.is_some() {
                continue;
            }
            let archived_receipt = format!("visual-confirmation-{number}.json");
            if read_private(&self.directory, VISUAL, self.owner)?.is_some() {
                if read_private(&self.directory, &archived_receipt, self.owner)?.is_some() {
                    return Err("RGB-arkivet har allerede en kvittering på dette nummeret.".into());
                }
                rename_at(&self.directory, VISUAL, &archived_receipt)?;
            }
            // Gjør også en eventuell tidligere, avbrutt kvitteringsflytting
            // varig før det forrige grunnopptaket får arkivnavn.
            self.directory.sync_all().map_err(|e| e.to_string())?;
            rename_at(&self.directory, BASELINE, &archived)?;
            return self.directory.sync_all().map_err(|e| e.to_string());
        }
        Err("RGB-arkivet har ingen ledige numre.".into())
    }

    pub fn journal(&self, baseline: &Baseline) -> Result<Option<StoredJournal>, String> {
        self.revalidate()?;
        let Some(text) = read_private(&self.directory, JOURNAL, self.owner)? else {
            return Ok(None);
        };
        // Parse direkte til struct: et mellomliggende Value ville skjult
        // duplikate JSON-nøkler før serde kunne avvise dem.
        let record: StoredJournal =
            serde_json::from_str(&text).map_err(|e| format!("Ugyldig RGB-journal: {e}"))?;
        record.validate(baseline)?;
        Ok(Some(record))
    }

    pub fn validate_state(
        &self,
        baseline: &Baseline,
        expected: Option<&StoredJournal>,
    ) -> Result<(), String> {
        if &self.baseline()? != baseline {
            return Err("Det opprinnelige RGB-grunnopptaket er endret under økten.".into());
        }
        if self.journal(baseline)?.as_ref() != expected {
            return Err("RGB-journalen er endret utenfor denne arbeideren.".into());
        }
        Ok(())
    }

    pub fn write_journal(
        &self,
        baseline: &Baseline,
        expected: Option<&StoredJournal>,
        record: &StoredJournal,
    ) -> Result<(), String> {
        self.validate_state(baseline, expected)?;
        record.validate(baseline)?;
        let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > LIMIT {
            return Err("RGB-journalen er for stor.".into());
        }
        self.replace(JOURNAL, &bytes, || {
            self.validate_state(baseline, expected)?;
            Ok(())
        })
    }

    fn replace(
        &self,
        name: &str,
        bytes: &[u8],
        check: impl Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        replace_private(
            &self.directory,
            self.owner,
            name,
            bytes,
            || {
                check()?;
                #[cfg(test)]
                if self.fail_before_rename.load(Ordering::Relaxed) {
                    return Err("Simulert fsync/rename-feil.".into());
                }
                Ok(())
            },
            || {
                #[cfg(test)]
                if self.fail_directory_sync.load(Ordering::Relaxed) {
                    return Err("Simulert fsync-feil på journalmappen etter rename.".into());
                }
                Ok(())
            },
        )
    }

    pub(crate) fn visual_evidence(
        &self,
        baseline: &Baseline,
    ) -> Result<Option<VisualEvidence>, String> {
        self.revalidate()?;
        read_private(&self.directory, VISUAL, self.owner)?
            .map(|text| {
                let evidence: VisualEvidence = serde_json::from_str(&text)
                    .map_err(|e| format!("Ugyldig visuell RGB-kvittering: {e}"))?;
                evidence.validate(baseline)?;
                Ok(evidence)
            })
            .transpose()
    }

    pub fn acknowledge_visual(
        &self,
        baseline: &Baseline,
        record: &StoredJournal,
        evidence: &VisualEvidence,
    ) -> Result<(), String> {
        evidence.validate_record(baseline, record)?;
        self.validate_state(baseline, Some(record))?;
        // En gyldig, ny visuell avklaring kan overskrive en gammel korrupt
        // kvittering fra protokoll 1. Krav til eier, filtype og modus består.
        let previous = read_private(&self.directory, VISUAL, self.owner)?;
        self.replace(
            VISUAL,
            &serde_json::to_vec(evidence).map_err(|e| e.to_string())?,
            || {
                self.validate_state(baseline, Some(record))?;
                if read_private(&self.directory, VISUAL, self.owner)? != previous {
                    return Err("RGB-kvitteringen er endret under bekreftelsen.".into());
                }
                Ok(())
            },
        )?;
        // replace har fsync-et både evidensfilen og mappen før journalen
        // kan fjernes. Lesbare felt alene kommer aldri til denne ryddingen.
        if evidence.original_baseline {
            if self.visual_evidence(baseline)?.as_ref() != Some(evidence) {
                return Err("RGB-kvitteringen kunne ikke kontrolleres etter lagring.".into());
            }
            self.clear_journal(baseline.binding(), baseline, record)?;
        }
        Ok(())
    }

    /// Bare etter full tilbakelesing eller varig, eksplisitt userVisual-
    /// kvittering fra acknowledge_visual. Dette er to ulike beviskilder.
    pub fn clear_journal(
        &self,
        binding: &Binding,
        baseline: &Baseline,
        expected: &StoredJournal,
    ) -> Result<(), String> {
        if binding != baseline.binding() {
            return Err("RGB-journalen tilhører et annet kort.".into());
        }
        self.validate_state(baseline, Some(expected))?;
        unlink_at(&self.directory, JOURNAL)?;
        #[cfg(test)]
        if self.fail_sync_after_unlink.load(Ordering::Relaxed) {
            return Err("Simulert fsync-feil etter sletting av RGB-journal.".into());
        }
        self.directory.sync_all().map_err(|e| e.to_string())
    }
}

/// Åpner hvert stilledd relativt forrige fd. Ingen komponent får være symlink.
/// Produksjonsforeldre må være root-eid uten gruppe-/andre-skrivetilgang.
fn open_directory(path: &Path, owner: u32, trusted: bool, private: bool) -> Result<File, String> {
    if !path.is_absolute() {
        return Err("RGB-lageret krever en absolutt, eksplisitt sti.".into());
    }
    let mut dir = File::open("/").map_err(|e| e.to_string())?;
    for part in path.components() {
        match part {
            Component::RootDir => continue,
            Component::Normal(name) => {
                let name = CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
                // SAFETY: dir er en åpen mappe; NUL-avsluttet enkeltkomponent
                // og O_NOFOLLOW hindrer symlink-følging i hvert stilledd.
                let fd = unsafe {
                    libc::openat(
                        dir.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    )
                };
                if fd < 0 {
                    return Err(format!(
                        "Åpne trygg RGB-mappe: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                // SAFETY: positiv fd er nettopp opprettet og eies heretter av File.
                dir = unsafe { File::from_raw_fd(fd) };
                let m = dir.metadata().map_err(|e| e.to_string())?;
                if trusted && (m.uid() != 0 || m.mode() & 0o022 != 0) {
                    return Err(
                        "RGB-stiens foreldre må være root-eid og uten andres skrivetilgang.".into(),
                    );
                }
            }
            _ => return Err("RGB-stien kan ikke inneholde . eller ...".into()),
        }
    }
    let m = dir.metadata().map_err(|e| e.to_string())?;
    if private && (m.uid() != owner || m.mode() & 0o7777 != 0o700) {
        return Err("RGB-tilstandsmappen må ha riktig eier og modus 0700.".into());
    }
    Ok(dir)
}

fn filename(name: &str) -> Result<CString, std::io::Error> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    CString::new(name).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
}

fn open_at(dir: &File, name: &str, flags: i32, mode: libc::mode_t) -> std::io::Result<File> {
    let name = filename(name)?;
    // Avvis eksisterende enhetsnoder/FIFO/symlinker før selve åpningen.
    // SAFETY: stat er en ren utskriftsbuffer og navnet/dirfd er gyldige.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    let status = unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if status == 0 && stat.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    if status != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::NotFound || flags & libc::O_CREAT == 0 {
            return Err(error);
        }
    }
    // SAFETY: validert enkeltfilnavn, gyldig dirfd. NONBLOCK unngår å henge
    // ved en FIFO; fstat avviser alle andre filtyper enn regulære filer.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            mode,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: eierskapet til denne nye fd-en overføres nøyaktig én gang.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn validate_file(file: &File, owner: u32) -> Result<(), String> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != owner || m.mode() & 0o7777 != 0o600 || m.nlink() != 1 {
        return Err(
            "RGB-filen må være en privat, regulær fil med riktig eier og modus 0600.".into(),
        );
    }
    Ok(())
}

fn read_private(dir: &File, name: &str, owner: u32) -> Result<Option<String>, String> {
    let file = match open_at(dir, name, libc::O_RDONLY, 0) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Åpne privat RGB-fil: {e}")),
    };
    validate_file(&file, owner)?;
    let mut text = String::new();
    file.take(LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > LIMIT {
        return Err("RGB-filen er for stor.".into());
    }
    Ok(Some(text))
}

/// Atomisk privat filbytte. Kalleren validerer det forventede dokumentet
/// før innkalling og igjen gjennom check etter fsync, rett før rename.
fn replace_private(
    dir: &File,
    owner: u32,
    name: &str,
    bytes: &[u8],
    check: impl Fn() -> Result<(), String>,
    after_rename: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    if bytes.len() as u64 > LIMIT {
        return Err("RGB-dokumentet er for stort.".into());
    }
    // Eksisterende mål må fortsatt være en trygg regulær fil, også når
    // operasjonen oppdaterer en tillatelse eller visuell kvittering.
    read_private(dir, name, owner)?;
    let temp = format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = open_at(
        dir,
        &temp,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )
    .map_err(|e| format!("Opprett privat RGB-dokument: {e}"))?;
    let result = (|| {
        validate_file(&file, owner)?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        check()?;
        rename_at(dir, &temp, name)?;
        after_rename()?;
        dir.sync_all().map_err(|e| e.to_string())
    })();
    // Kun vår egen nye tempfil. Journal og tidligere dokumenter slettes
    // aldri som feilhåndtering, heller ikke ved feil etter rename.
    let _ = unlink_at(dir, &temp);
    result
}

fn rename_at(dir: &File, from: &str, to: &str) -> Result<(), String> {
    let from = filename(from).map_err(|e| e.to_string())?;
    let to = filename(to).map_err(|e| e.to_string())?;
    // SAFETY: begge validerte navn ligger i samme åpne, private mappe.
    if unsafe { libc::renameat(dir.as_raw_fd(), from.as_ptr(), dir.as_raw_fd(), to.as_ptr()) } == 0
    {
        Ok(())
    } else {
        Err(format!(
            "Bytt RGB-journal atomisk: {}",
            std::io::Error::last_os_error()
        ))
    }
}

fn unlink_at(dir: &File, name: &str) -> Result<(), String> {
    let name = filename(name).map_err(|e| e.to_string())?;
    // SAFETY: enkeltfilnavnet er validert; ingen rekursjon eller symlink-følging.
    if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(format!(
            "Rydd privat RGB-fil: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use rigg_core::rgb::{AdapterIdentity, Observed, PciIdentity, Signature};
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

    pub struct Fixture {
        pub path: PathBuf,
    }

    impl Fixture {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rigg-rgb-private-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            DirBuilder::new().mode(0o700).create(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self { path }
        }

        pub fn write(&self, name: &str, bytes: &[u8]) {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(self.path.join(name))
                .unwrap();
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .unwrap();
            file.write_all(bytes).unwrap();
        }

        pub fn store(&self, baseline: &Baseline) -> PrivateStore {
            self.write(BASELINE, &serde_json::to_vec(baseline).unwrap());
            PrivateStore::open_fixture(&self.path).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }

    pub fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-private-rgb-fixture".into(),
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

    pub fn observed() -> Observed {
        Observed {
            color: Some([20, 40, 60]),
            brightness_raw: Some(80),
            mode: None,
        }
    }

    pub fn baseline() -> Baseline {
        Baseline::user_confirmed_static(binding(), observed()).unwrap()
    }

    pub fn authorization() -> Authorization {
        Authorization::new(1000, binding(), rgb_auth::MIN_COMPATIBLE_BUILD).unwrap()
    }

    pub fn journal(baseline: &Baseline) -> Journal {
        Journal {
            version: 1,
            baseline: baseline.clone(),
            before: Book::new(baseline.clone()).unwrap().snapshot().confirmed,
            before_observed: baseline.observed().clone(),
            requested: Request::Off {},
            owner: 1,
            txn: 1,
        }
    }

    pub fn record(baseline: &Baseline) -> StoredJournal {
        let journal = journal(baseline);
        StoredJournal {
            version: 1,
            target: journal.requested.clone(),
            journal,
            operation: Operation::Apply,
            phase: Phase::Prepared,
            prior_write_responsibility: false,
            recovery_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn eksplisitt_forberedelse_lagrer_originalen_uforandret_for_tillatelsen() {
        let f = Fixture::new();
        let store = PrivateStore::open_fixture(&f.path).unwrap();
        let baseline = baseline();
        prepare_documents(
            &authorization(),
            &baseline,
            &store,
            &f.path,
            &store.directory,
        )
        .unwrap();
        assert_eq!(store.baseline().unwrap(), baseline);
        let auth = read_private(&store.directory, "rgb.json", store.owner)
            .unwrap()
            .unwrap();
        assert_eq!(Authorization::parse(&auth).unwrap(), authorization());
        prepare_documents(
            &authorization(),
            &baseline,
            &store,
            &f.path,
            &store.directory,
        )
        .unwrap();
        let changed = Baseline::user_confirmed_static(
            binding(),
            Observed {
                color: Some([9, 9, 9]),
                ..observed()
            },
        )
        .unwrap();
        assert!(
            prepare_documents(
                &authorization(),
                &changed,
                &store,
                &f.path,
                &store.directory
            )
            .is_err()
        );
        assert_eq!(store.baseline().unwrap(), baseline);
        let intent = record(&baseline);
        store.write_journal(&baseline, None, &intent).unwrap();
        assert!(
            prepare_documents(
                &authorization(),
                &baseline,
                &store,
                &f.path,
                &store.directory
            )
            .is_err()
        );
        assert_eq!(store.journal(&baseline).unwrap(), Some(intent));
    }

    #[test]
    fn ny_ren_okt_arkiverer_tidligere_baseline_og_taaler_avbrutt_lagring() {
        let f = Fixture::new();
        let original = baseline();
        let store = f.store(&original);
        f.write(VISUAL, b"{}");
        let next = Baseline::user_confirmed_static(
            binding(),
            Observed {
                color: Some([11, 22, 33]),
                ..observed()
            },
        )
        .unwrap();

        store.fail_before_rename.store(true, Ordering::Relaxed);
        assert!(
            prepare_documents(&authorization(), &next, &store, &f.path, &store.directory).is_err()
        );
        assert!(store.baseline_optional().unwrap().is_none());
        assert_eq!(
            fs::read(f.path.join("visual-confirmation-1.json")).unwrap(),
            b"{}"
        );
        assert_eq!(
            serde_json::from_slice::<Baseline>(&fs::read(f.path.join("baseline-1.json")).unwrap())
                .unwrap(),
            original
        );
        assert!(!f.path.join("rgb.json").exists());

        store.fail_before_rename.store(false, Ordering::Relaxed);
        for _ in 0..2 {
            prepare_documents(&authorization(), &next, &store, &f.path, &store.directory).unwrap();
        }
        assert_eq!(store.baseline().unwrap(), next);
        assert!(store.visual_evidence(&next).unwrap().is_none());
        assert!(!f.path.join("baseline-2.json").exists());
    }

    #[test]
    fn ren_ny_aktivering_kan_reparere_kvittering_med_samme_utgangspunkt() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        f.write(VISUAL, b"{}");
        prepare_documents(
            &authorization(),
            &baseline,
            &store,
            &f.path,
            &store.directory,
        )
        .unwrap();
        assert_eq!(store.baseline().unwrap(), baseline);
        assert!(store.visual_evidence(&baseline).unwrap().is_none());
        assert_eq!(
            fs::read(f.path.join("visual-confirmation-1.json")).unwrap(),
            b"{}"
        );
    }

    #[test]
    fn annen_autorisasjon_kan_ikke_opprette_eller_erstatte_originalen() {
        let f = Fixture::new();
        let store = PrivateStore::open_fixture(&f.path).unwrap();
        let other = Authorization::new(1001, binding(), rgb_auth::MIN_COMPATIBLE_BUILD).unwrap();
        f.write("rgb.json", &serde_json::to_vec(&other).unwrap());
        assert!(
            prepare_documents(
                &authorization(),
                &baseline(),
                &store,
                &f.path,
                &store.directory
            )
            .is_err()
        );
        assert!(!f.path.join(BASELINE).exists());
        assert_eq!(
            Authorization::parse(&fs::read_to_string(f.path.join("rgb.json")).unwrap()).unwrap(),
            other
        );
    }

    #[test]
    fn visuell_kvittering_er_varig_for_originaljournalen_kan_ryddes() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        let record = record(&baseline);
        store.write_journal(&baseline, None, &record).unwrap();
        let target = initial_request(&baseline).unwrap();
        assert!(
            VisualEvidence::new(
                1000,
                &baseline,
                &record,
                &target,
                &observed(),
                EvidenceContext::LiveRecovery
            )
            .is_err()
        );
        let evidence = VisualEvidence::new(
            1000,
            &baseline,
            &record,
            &target,
            &observed(),
            EvidenceContext::RestartBaseline,
        )
        .unwrap();
        store.fail_before_rename.store(true, Ordering::Relaxed);
        assert!(
            store
                .acknowledge_visual(&baseline, &record, &evidence)
                .is_err()
        );
        assert!(store.journal(&baseline).unwrap().is_some());
        assert!(store.visual_evidence(&baseline).unwrap().is_none());
        store.fail_before_rename.store(false, Ordering::Relaxed);
        store.fail_directory_sync.store(true, Ordering::Relaxed);
        assert!(
            store
                .acknowledge_visual(&baseline, &record, &evidence)
                .is_err()
        );
        assert!(store.journal(&baseline).unwrap().is_some());
        store.fail_directory_sync.store(false, Ordering::Relaxed);
        store
            .acknowledge_visual(&baseline, &record, &evidence)
            .unwrap();
        assert!(store.journal(&baseline).unwrap().is_none());
        assert_eq!(
            store.visual_evidence(&baseline).unwrap(),
            Some(evidence.clone())
        );
        assert_eq!(
            fs::metadata(f.path.join(VISUAL)).unwrap().mode() & 0o7777,
            0o600
        );
        let json = serde_json::to_value(evidence).unwrap();
        assert_eq!(json["source"], "userVisual");
        assert!(json["observed"]["mode"].is_null());
        assert_eq!(store.baseline().unwrap(), baseline);
    }

    #[test]
    fn visuell_forrigekvittering_beholder_journal_og_strengt_schema() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        let previous = Request::Static {
            color: [7, 8, 9],
            brightness: 70u8.try_into().unwrap(),
        };
        let actual = Observed {
            color: Some([7, 8, 9]),
            brightness_raw: Some(70),
            mode: None,
        };
        let mut record = record(&baseline);
        record.journal.before = previous.clone();
        record.journal.before_observed = actual.clone();
        record.target = previous.clone();
        record.operation = Operation::Recover;
        record.phase = Phase::Attempted;
        store.write_journal(&baseline, None, &record).unwrap();
        let evidence = VisualEvidence::new(
            1000,
            &baseline,
            &record,
            &previous,
            &actual,
            EvidenceContext::LiveRecovery,
        )
        .unwrap();
        store
            .acknowledge_visual(&baseline, &record, &evidence)
            .unwrap();
        assert_eq!(store.journal(&baseline).unwrap(), Some(record));
        let good = fs::read_to_string(f.path.join(VISUAL)).unwrap();
        for bad in [
            good.replacen("\"version\":1", "\"version\":1,\"version\":1", 1),
            good.replacen("\"userVisual\"", "\"hardwareVerified\"", 1),
            good.replacen("{", "{\"unknown\":1,", 1),
        ] {
            fs::write(f.path.join(VISUAL), bad).unwrap();
            assert!(store.visual_evidence(&baseline).is_err());
        }
    }

    #[test]
    fn private_mapper_filer_og_las_har_ingen_vert_fallback() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        assert_eq!(store.baseline().unwrap(), baseline);
        assert!(store.journal(&baseline).unwrap().is_none());
        assert!(PrivateStore::open_fixture(&f.path).is_err());
        drop(store);
        assert!(PrivateStore::open_fixture(&f.path).is_ok());
        assert!(PrivateStore::open_fixture(&f.path.join("finnes-ikke")).is_err());
    }

    #[test]
    fn symlinker_i_mapper_filer_og_las_avvises() {
        let f = Fixture::new();
        let other = Fixture::new();
        symlink(&other.path, f.path.join("lenke")).unwrap();
        assert!(PrivateStore::open_fixture(&f.path.join("lenke")).is_err());
        symlink(other.path.join("ingenting"), f.path.join("worker.lock")).unwrap();
        assert!(PrivateStore::open_fixture(&f.path).is_err());
        fs::remove_file(f.path.join("worker.lock")).unwrap();
        let store = PrivateStore::open_fixture(&f.path).unwrap();
        symlink(other.path.join(BASELINE), f.path.join(BASELINE)).unwrap();
        assert!(store.baseline().is_err());
        fs::remove_file(f.path.join(BASELINE)).unwrap();
        f.write(BASELINE, &serde_json::to_vec(&baseline()).unwrap());
        symlink(other.path.join(JOURNAL), f.path.join(JOURNAL)).unwrap();
        assert!(store.journal(&baseline()).is_err());
    }

    #[test]
    fn feil_eier_modus_hardlenke_og_filtype_avvises() {
        let f = Fixture::new();
        let store = f.store(&baseline());
        fs::set_permissions(f.path.join(BASELINE), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.baseline().is_err());
        fs::set_permissions(f.path.join(BASELINE), fs::Permissions::from_mode(0o600)).unwrap();
        let file = File::open(f.path.join(BASELINE)).unwrap();
        let wrong_owner = file.metadata().unwrap().uid().wrapping_add(1);
        assert!(validate_file(&file, wrong_owner).is_err());
        fs::hard_link(f.path.join(BASELINE), f.path.join("annen.json")).unwrap();
        assert!(store.baseline().is_err());
        fs::remove_file(f.path.join("annen.json")).unwrap();
        fs::set_permissions(&f.path, fs::Permissions::from_mode(0o750)).unwrap();
        assert!(store.baseline().is_err());
        fs::set_permissions(&f.path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(f.path.join(JOURNAL)).unwrap();
        assert!(store.journal(&baseline()).is_err());
    }

    #[test]
    fn json_schema_duplikater_storrelse_og_originalbaseline_valideres() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        let good = serde_json::to_string(&record(&baseline)).unwrap();
        for text in [
            good.replacen("\"version\":1", "\"version\":1,\"version\":1", 1),
            good.replacen("\"version\":1", "\"version\":9", 1),
            good.replacen("{", "{\"ukjent\":1,", 1),
            " ".repeat(LIMIT as usize + 1),
        ] {
            f.write(JOURNAL, text.as_bytes());
            assert!(store.journal(&baseline).is_err());
            fs::remove_file(f.path.join(JOURNAL)).unwrap();
        }
        let mut wrong = record(&baseline);
        wrong.journal.before_observed.color = Some([1, 2, 3]);
        assert!(store.write_journal(&baseline, None, &wrong).is_err());
        assert!(!f.path.join(JOURNAL).exists());
        let changed = Baseline::user_confirmed_static(
            binding(),
            rigg_core::rgb::Observed {
                color: Some([1, 2, 3]),
                ..observed()
            },
        )
        .unwrap();
        assert!(store.validate_state(&changed, None).is_err());
    }

    #[test]
    fn atomisk_journal_bevarer_forrige_intent_ved_feil() {
        let f = Fixture::new();
        let baseline = baseline();
        let store = f.store(&baseline);
        let before = record(&baseline);
        store.write_journal(&baseline, None, &before).unwrap();
        assert_eq!(
            fs::metadata(f.path.join(JOURNAL)).unwrap().mode() & 0o7777,
            0o600
        );
        let bytes = fs::read(f.path.join(JOURNAL)).unwrap();
        let mut next = before.clone();
        next.phase = Phase::Attempted;
        store.fail_before_rename.store(true, Ordering::Relaxed);
        assert!(
            store
                .write_journal(&baseline, Some(&before), &next)
                .is_err()
        );
        assert_eq!(fs::read(f.path.join(JOURNAL)).unwrap(), bytes);
        assert_eq!(store.journal(&baseline).unwrap(), Some(before.clone()));
        assert!(
            fs::read_dir(&f.path).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
        store.fail_before_rename.store(false, Ordering::Relaxed);
        store
            .write_journal(&baseline, Some(&before), &next)
            .unwrap();
        assert_eq!(store.journal(&baseline).unwrap(), Some(next));
    }

    #[test]
    fn byttet_mappe_las_eller_journal_avvises() {
        let f = Fixture::new();
        let store = f.store(&baseline());
        fs::rename(f.path.join("worker.lock"), f.path.join("old.lock")).unwrap();
        f.write("worker.lock", b"");
        assert!(store.baseline().is_err());
        fs::remove_file(f.path.join("worker.lock")).unwrap();
        fs::rename(f.path.join("old.lock"), f.path.join("worker.lock")).unwrap();
        let record = record(&baseline());
        store.write_journal(&baseline(), None, &record).unwrap();
        assert!(store.validate_state(&baseline(), None).is_err());
        let old_path = f.path.with_extension("old");
        fs::rename(&f.path, &old_path).unwrap();
        fs::create_dir(&f.path).unwrap();
        fs::set_permissions(&f.path, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(store.baseline().is_err());
        fs::remove_dir(&f.path).unwrap();
        fs::rename(old_path, &f.path).unwrap();
    }

    #[test]
    fn autorisasjon_er_streng_privatisert_og_byggbundet() {
        let f = Fixture::new();
        // SAFETY: testens eget fil-eierskap; ingen produksjonssti.
        let uid = unsafe { libc::geteuid() };
        let path = f.path.join("rgb.json");
        let good = serde_json::to_string(&authorization()).unwrap();
        f.write("rgb.json", good.as_bytes());
        assert_eq!(
            load_authorization_owned(&path, uid, false).unwrap(),
            authorization()
        );
        fs::remove_file(&path).unwrap();
        for bad in [
            good.replacen("\"version\":1", "\"version\":1,\"version\":1", 1),
            good.replacen("{", "{\"ukjent\":1,", 1),
            good.replace(
                &format!("\"build\":{}", rgb_auth::MIN_COMPATIBLE_BUILD),
                "\"build\":23",
            ),
            good.replace(
                &format!("\"build\":{}", rgb_auth::MIN_COMPATIBLE_BUILD),
                "\"build\":4294967295",
            ),
        ] {
            f.write("rgb.json", bad.as_bytes());
            assert!(load_authorization_owned(&path, uid, false).is_err());
            fs::remove_file(&path).unwrap();
        }
    }
}
