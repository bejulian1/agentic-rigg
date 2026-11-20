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

pub fn ready() -> bool {
    [
        MARK,
        "/usr/local/libexec/agentic-rigg-system/system-install",
        "/usr/local/libexec/agentic-rigg-rgb-v2/rgb-admin.py",
    ]
    .iter()
    .all(|path| trusted(Path::new(path)))
        && std::fs::read_to_string(MARK).is_ok_and(|value| value == "1\n")
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
