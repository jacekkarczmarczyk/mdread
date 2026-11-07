//! Dokument powitalny / pomoc (`ui/help/<język>.md`) wbudowany w exe. Otwierany jako zwykła karta
//! z kopii w `%APPDATA%\mdread\help`, więc działa jak każdy plik: sesja, pozycja czytania, edycja.

use std::path::PathBuf;

use crate::i18n;

fn source(language: &str) -> &'static str {
    match language {
        "pl" => include_str!("../ui/help/pl.md"),
        _ => include_str!("../ui/help/en.md"),
    }
}

/// Zapisuje kopię pomocy w bieżącym języku (nadpisuje, gdy nowsza wersja programu ją zmieniła).
fn path() -> PathBuf {
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
