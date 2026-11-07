//! Ustawienia w %APPDATA%\mdread\settings.json.

use std::path::PathBuf;

use serde_json::{Map, Value};

fn file() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("mdread").join("settings.json")
}

fn load() -> Map<String, Value> {
    // BOM dodaje np. Notatnik albo PowerShell 5.1 przy ręcznej edycji pliku.
    std::fs::read(file())
        .ok()
        .and_then(|bytes| serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok())
        .unwrap_or_default()
}

fn save(settings: &Map<String, Value>) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_vec_pretty(settings).unwrap_or_default());
}

pub const THEMES: &[&str] = &["github", "jetbrains", "vscode", "fluent", "reading"];
pub const MODES: &[&str] = &["system", "light", "dark"];

/// Wygląd zapisany przez stronę: motyw, tryb i kolory potrzebne przed jej załadowaniem.
pub struct Appearance {
    pub theme: String,
    pub mode: String,
    pub caption: Option<u32>,
    pub text: Option<u32>,
    pub background: Option<u32>,
}

pub fn appearance() -> Appearance {
    let settings = load();
    let text = |key: &str| settings.get(key).and_then(Value::as_str).map(str::to_owned);
    let color = |key: &str| text(key).as_deref().and_then(parse_color);
    let pick = |key: &str, allowed: &[&str]| {
        text(key).filter(|value| allowed.contains(&value.as_str())).unwrap_or_else(|| allowed[0].to_owned())
    };
    Appearance {
        theme: pick("theme", THEMES),
        mode: pick("mode", MODES),
        caption: color("caption"),
        text: color("text"),
        background: color("background"),
    }
}

/// Zapisuje wygląd z wiadomości strony (`theme`, `mode`, `caption`, `text`, `background`).
pub fn set_appearance(message: &Value) {
    let mut settings = load();
    for key in ["theme", "mode", "caption", "text", "background"] {
        if let Some(value) = message[key].as_str() {
            settings.insert(key.into(), Value::String(value.to_owned()));
        }
    }
    save(&settings);
}

/// Otwarte karty z ostatniego uruchomienia: `{ "tabs": [{ "path", "position" }], "active": ścieżka }`.
pub fn session() -> Option<Value> {
    load().get("session").cloned()
}

pub fn set_session(session: Value) {
    let mut settings = load();
    settings.insert("session".into(), session);
    save(&settings);
}

/// `system`, `pl` albo `en`.
pub fn language() -> String {
    load().get("language").and_then(Value::as_str).unwrap_or("system").to_owned()
}

pub fn set_language(language: &str) {
    let mut settings = load();
    settings.insert("language".into(), Value::String(language.to_owned()));
    save(&settings);
}

pub fn zoom() -> Option<f64> {
    load().get("zoom").and_then(Value::as_f64)
}

pub fn set_zoom(zoom: f64) {
    let mut settings = load();
    settings.insert("zoom".into(), Value::from(zoom));
    save(&settings);
}

/// `#rrggbb` → COLORREF (0x00BBGGRR).
pub fn parse_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#').filter(|hex| hex.len() == 6)?;
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | (rgb >> 16))
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

/// Lista edytorów poza Notatnikiem; `None`, dopóki nie została utworzona (pierwszy start).
pub fn editors() -> Option<Vec<PathBuf>> {
    let list = load().get("editors")?.as_array()?.iter().filter_map(Value::as_str).map(PathBuf::from).collect();
    Some(list)
}

pub fn set_editors(editors: &[PathBuf]) {
    let mut settings = load();
    let list = editors.iter().map(|path| Value::String(path.to_string_lossy().into_owned())).collect();
    settings.insert("editors".into(), Value::Array(list));
    save(&settings);
}

#[cfg(test)]
mod tests {
    use super::parse_color;

    #[test]
    fn parses_hex_to_colorref() {
        assert_eq!(parse_color("#0d1117"), Some(0x0017110d));
        assert_eq!(parse_color("#fff"), None);
        assert_eq!(parse_color("red"), None);
    }
}
