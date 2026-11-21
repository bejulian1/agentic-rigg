//! `agentic-rigg`: vinduet til Agentic Rigg (GTK4, uten libadwaita).
//!
//! ```text
//! agentic-rigg [--side SIDE] [--innstillinger DEL] [--hint] [--version]
//! ```
//!
//! Vinduet laster aldri NVML. Tallene kommer fra tjenesten `agentic-riggd`,
//! som vinduet starter selv når den ikke kjører.


mod app;
mod client;
mod easter;
mod infobar;
mod logg_ui;
mod motion;
mod pages;
mod prefs;
mod rapport_ui;
mod settings;
mod system_install;
mod shortcuts;
mod skjermbilder;
mod sidebar;
mod state;
mod theme;
mod timing;
mod toast;
mod topbar;
mod widgets;
mod window;

use rigg_core::{i18n, paths::Profile, tr};

fn help() -> &'static str {
    tr!("Bruk: agentic-rigg [--side SIDE] [--innstillinger DEL] [--hint] [--version]

  --side SIDE           åpne en side: oversikt, sensorer, grafer, grafikkort,
                        oppgaver, prosessor, disker, system, kontroller (Klokker og
                        effekt), viftekurve (Vifter) eller lys (RGB-lys)
  --innstillinger DEL   åpne Innstillinger (generelt, utseende, format,
                        sidefelt, grafer, logging, data eller om)
  --hint                et hint om en liten overraskelse du ikke har funnet
  --version             skriv versjonen")
}

/// Valgene som gjelder vinduet (også fra en ny start mens vinduet er åpent).
pub fn parse_window_args(args: &[String]) -> app::Cli {
    let mut cli = app::Cli::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--side" => cli.page = it.next().filter(|p| pages::def(p).is_some()).cloned(),
            "--innstillinger" => cli.settings = Some(it.next().cloned().unwrap_or_else(|| "generelt".into())),
            _ => {}
        }
    }
    cli
}

fn main() -> gtk::glib::ExitCode {
    let t0 = std::time::Instant::now();
    timing::init(t0);
    timing::mark("main");
    let args: Vec<String> = std::env::args().skip(1).collect();
    for a in &args {
        match a.as_str() {
            "--version" | "-V" => {
                println!("{}", rigg_core::version::long());
                return gtk::glib::ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                i18n::init(&i18n::read_preference(Profile::current()));
                println!("{}", help());
                return gtk::glib::ExitCode::SUCCESS;
            }
            "--hint" => {
                let (s, _) = prefs::load(rigg_core::paths::Profile::current());
                rigg_core::i18n::init(&s.language);
                if !s.surprises {
                    println!("{}", crate::easter::disabled_hint());
                } else {
                    let pick = (rigg_core::now_ms() / 1000) as usize;
                    println!("{}", rigg_core::easter::hint(&s.eggs_found, pick));
                }
                return gtk::glib::ExitCode::SUCCESS;
            }
            _ => {}
        }
    }
    choose_renderer();
    gtk::gio::resources_register_include!("rigg.gresource").expect("innebygde ressurser");
    timing::mark("gresource");
    let rc = app::run(parse_window_args(&args));
    timing::mark("exit");
    rc
}

/// Standardtegneren (runde 3C, `docs/produksjon/maalinger.md`).
pub const DEFAULT_RENDERER: &str = "cairo";

/// Velger tegneren før GTK starter. `GSK_RENDERER` fra miljøet vinner
/// (`cairo`, `gl`, `vulkan`). Variabelen blir stående, så popovere og
/// verktøytips (egne flater med egen tegner) får samme tegner som vinduet.
fn choose_renderer() {
    if std::env::var_os("GSK_RENDERER").is_none_or(|v| v.is_empty()) {
        // SAFETY: ingen andre tråder finnes ennå, og ingen leser miljøet samtidig.
        unsafe { std::env::set_var("GSK_RENDERER", DEFAULT_RENDERER) };
    }
}
