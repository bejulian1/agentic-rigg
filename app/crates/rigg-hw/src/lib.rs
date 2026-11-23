//! Agentic HW Monitor · maskinvare. Bare lesing.
//!
//! - [`gpu`]: skjermkortet. `Nvml` (bare `nvmlDeviceGet*`/`nvmlSystemGet*`,
//!   lastet med `dlopen`) og `Sim` (scenarier for tester og skjermbilder).
//! - [`sys`]: prosessor, minne, nett og hwmon fra `/proc` og `/sys`, med åpne
//!   filer og `pread`, og en rot-sti som kan byttes (fixtures i testene).
//!
//! Ingenting her skriver til skjermkortet, åpner `/dev/i2c-*` eller starter
//! programmer (se `tests/bare_lesing.rs`).

pub mod gpu;
pub mod sys;

/// En fil som åpnes én gang og leses fra start med `pread` ved hver prøve
/// (maskinvare-revisjon kap. 12: 1,1–1,3 µs mot 9 µs for åpne+les).
pub struct PreadFile {
    file: std::fs::File,
    buf: Vec<u8>,
}

impl PreadFile {
    pub fn open(path: &std::path::Path) -> std::io::Result<PreadFile> {
        Ok(PreadFile { file: std::fs::File::open(path)?, buf: Vec::with_capacity(256) })
    }

    /// Hele fila (eller de første `max` bytene) som tekst.
    pub fn read(&mut self, max: usize) -> std::io::Result<&str> {
        use std::os::unix::fs::FileExt;
        self.buf.clear();
        let mut chunk = [0u8; 4096];
        loop {
            let n = self.file.read_at(&mut chunk, self.buf.len() as u64)?;
            if n == 0 {
                break;
            }
            self.buf.extend_from_slice(&chunk[..n]);
            if self.buf.len() >= max {
                break;
            }
        }
        Ok(std::str::from_utf8(&self.buf).unwrap_or(""))
    }

    pub fn read_f64(&mut self) -> Option<f64> {
        self.read(64).ok()?.trim().parse().ok()
    }
}

/// Leser en liten tekstfil én gang (faste data).
pub fn read_text(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// «root» når fila bare kan leses av root, ellers en kort feil.
pub fn read_err(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "root".into(),
        std::io::ErrorKind::NotFound => "mangler".into(),
        _ => "feil".into(),
    }
}
