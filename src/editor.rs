//! Edycja w zewnętrznym programie (mdread sam niczego nie edytuje).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::settings;

const NOTEPAD: &str = "notepad.exe";

pub fn open(path: &Path) {
    let program = settings::editor().unwrap_or_else(|| PathBuf::from(NOTEPAD));
    if let Err(error) = Command::new(&program).arg(path).spawn() {
        crate::show_error(&format!("Nie udało się uruchomić edytora {}.\n\n{error}", program.display()));
    }
}

/// Opis edytora dla strony: nazwa do podpowiedzi i czy wybrany przez użytkownika.
pub fn info() -> Value {
    match settings::editor() {
        Some(path) => {
            let name = path.file_stem().unwrap_or(path.as_os_str()).to_string_lossy().into_owned();
            json!({ "type": "editor", "name": name, "custom": true })
        }
        None => json!({ "type": "editor", "name": "Notatnik", "custom": false }),
    }
}
