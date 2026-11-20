use gtk::prelude::*;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const MARK: &str = "/usr/local/libexec/agentic-rigg-system/revision";
const COMMAND: &str = include_str!("../../../data/hjelper/system-bootstrap.command");

pub fn trusted(path: &Path) -> bool {
    path.ancestors().enumerate().all(|(index, part)| {
        std::fs::symlink_metadata(part).is_ok_and(|info| {
            info.uid() == 0
                && info.mode() & 0o022 == 0
                && if index == 0 {
                    info.is_file()
                } else {
                    info.is_dir()
                }
        })
    })
}

pub fn ready(activate: bool) -> bool {
    [
        MARK,
        "/usr/local/libexec/agentic-rigg-system/system-install",
        "/usr/local/libexec/agentic-rigg-rgb-v2/rgb-admin.py",
    ]
    .iter()
    .all(|path| trusted(Path::new(path)))
        && std::fs::read_to_string(MARK).is_ok_and(|value| compatible_revision(&value, activate))
}

fn compatible_revision(value: &str, activate: bool) -> bool {
    if value == "1\n" {
        return !activate;
    }
    value.strip_prefix("2:").and_then(|v| v.trim_end().parse::<u32>().ok()).is_some_and(|build| {
        build > 0 && build <= rigg_core::version::build() && (!activate ||
            (rigg_core::hjelper::compatible(rigg_core::hjelper::PROTOCOL, build)
             && rigg_core::rgb_auth::compatible_build(build)))
    })
}

pub fn prompt() -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let text = gtk::Label::new(Some(
        "Systemkomponentene mangler eller må oppdateres. Kjør kommandoen i en administratorterminal. Oppgi arkivstien og SHA-256 fra utgivelsessiden.",
    ));
    text.set_wrap(true);
    text.set_xalign(0.0);
    let command = gtk::Entry::new();
    command.set_editable(false);
    command.set_text(COMMAND.trim());
    let copy = gtk::Button::with_label("Kopier installasjonskommando");
    copy.set_halign(gtk::Align::Start);
    copy.connect_clicked(|button| button.clipboard().set_text(COMMAND.trim()));
    box_.append(&text);
    box_.append(&command);
    box_.append(&copy);
    box_
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gammel_installasjon_tillater_bare_opprydding() {
        for old in ["1\n", "2:24\n"] {
            assert!(!compatible_revision(old, true));
            assert!(compatible_revision(old, false));
        }
        assert!(compatible_revision("2:25\n", true));
        for unknown in ["", "3:25\n", "2:0\n", "2:4294967295\n"] {
            assert!(!compatible_revision(unknown, false));
        }
    }
}
