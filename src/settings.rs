//! Ustawienia w %APPDATA%\mdread\settings.json.

use std::path::PathBuf;

use serde_json::{Map, Value};

fn file() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("mdread").join("settings.json")
}

fn load() -> Map<String, Value> {
    std::fs::read(file())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(settings: &Map<String, Value>) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_vec_pretty(settings).unwrap_or_default());
}

/// Zewnętrzny edytor wybrany przez użytkownika; `None` oznacza Notatnik.
pub fn editor() -> Option<PathBuf> {
    load().get("editor").and_then(Value::as_str).map(PathBuf::from)
}

pub fn set_editor(editor: Option<PathBuf>) {
    let mut settings = load();
    match editor {
        Some(path) => settings.insert("editor".into(), Value::String(path.to_string_lossy().into_owned())),
        None => settings.remove("editor"),
    };
    save(&settings);
}
