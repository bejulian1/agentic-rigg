//! Livsløpet (arkitektur.md 3.6 og 4.7):
//! - `READY=1` til `$NOTIFY_SOCKET` når socketen er bundet og første prøve er
//!   tatt (`Type=notify`), uten egen crate.
//! - Stopp 60 s etter at siste klient koblet fra, når «Fortsett i
//!   bakgrunnen» er av og ingen logg går. En ny klient avbryter.
//! - SIGTERM, SIGINT og SIGHUP: lagre historikk og min/maks, lukk loggen,
//!   `nvmlShutdown`, slett socketen.
//! - Dvale (logind `PrepareForSleep`): stopp prøvene og lagre før, start igjen
//!   2 s etter oppvåkning.

use std::os::unix::net::UnixDatagram;

/// Sier fra til systemd. Gjør ingenting uten `NOTIFY_SOCKET`.
pub fn sd_notify(state: &str) {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let Ok(sock) = UnixDatagram::unbound() else { return };
    let p = path.to_string_lossy().into_owned();
    let res = if let Some(abs) = p.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        std::os::unix::net::SocketAddr::from_abstract_name(abs.as_bytes()).and_then(|a| sock.send_to_addr(state.as_bytes(), &a))
    } else {
        sock.send_to(state.as_bytes(), &p)
    };
    if let Err(e) = res {
        eprintln!("Kunne ikke si fra til systemd: {e}");
    }
}

/// Lytter på `PrepareForSleep` fra logind på systembussen. `f(true)` før
/// dvale, `f(false)` etter. Uten systembuss gjør den ingenting.
pub fn watch_sleep(f: impl Fn(bool) + 'static) -> Option<(gio::DBusConnection, gio::SignalSubscriptionId)> {
    let conn = gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>).ok()?;
    #[allow(deprecated)]
    let id = conn.signal_subscribe(
        Some("org.freedesktop.login1"),
        Some("org.freedesktop.login1.Manager"),
        Some("PrepareForSleep"),
        Some("/org/freedesktop/login1"),
        None,
        gio::DBusSignalFlags::NONE,
        move |_c, _sender, _path, _iface, _signal, params| {
            if let Some((going,)) = params.get::<(bool,)>() {
                f(going);
            }
        },
    );
    Some((conn, id))
}
