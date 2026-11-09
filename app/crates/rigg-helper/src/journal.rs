//! Journalen. Under systemd går stderr til journald, og `<N>` foran linja
//! gir prioriteten (sd-daemon(3)). Utenfor systemd (testene) skrives linja
//! uten prefiks. Hver endring av kortet logges med hva, fra og til.

fn line(prio: u8, msg: &str) {
    if std::env::var_os("JOURNAL_STREAM").is_some() {
        eprintln!("<{prio}>{msg}");
    } else {
        eprintln!("agentic-rigg-helper: {msg}");
    }
}

pub fn err(msg: &str) {
    line(3, msg);
}
pub fn warn(msg: &str) {
    line(4, msg);
}
pub fn notice(msg: &str) {
    line(5, msg);
}
pub fn info(msg: &str) {
    line(6, msg);
}
