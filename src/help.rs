//! Welcome / help document (`ui/help/<language>.md`) built into the exe. Opened as a regular tab
//! from a copy in `%APPDATA%\mdread\help`, so it works like any file: session, reading position, editing.

use std::path::PathBuf;

use crate::i18n;

fn source(language: &str) -> &'static str {
    match language {
        "pl" => include_str!("../ui/help/pl.md"),
        _ => include_str!("../ui/help/en.md"),
    }
}

/// Writes a copy of the help in the current language (overwrites it when a newer program version changed it).
/// Also called at startup, before tabs are restored, so a help tab from the session shows the current content.
pub fn path() -> PathBuf {
    let language = i18n::current();
    let name = if language == "pl" { "MDRead — pomoc.md" } else { "MDRead — help.md" };
    let dir = PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("mdread").join("help");
    let path = dir.join(name);
    let text = source(language);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, text);
    }
    path
}

pub fn open() {
    crate::open_path(&path(), None);
}
