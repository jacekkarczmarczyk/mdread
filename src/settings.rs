//! Settings in %APPDATA%\mdread\settings.json.

use std::path::PathBuf;

use serde_json::{Map, Value};

fn file() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join(crate::PROFILE).join("settings.json")
}

fn load() -> Map<String, Value> {
    // A BOM is added e.g. by Notepad or PowerShell 5.1 when the file is edited by hand.
    std::fs::read(file())
        .ok()
        .and_then(|bytes| serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok())
        .unwrap_or_default()
}

/// Writes and forces the data to disk (`sync_all` = FlushFileBuffers), so the session and settings are not lost
/// in the Windows cache after a system crash. Writes are rare (tab switch, end of scrolling).
fn save(settings: &Map<String, Value>) {
    use std::io::Write;

    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::File::create(path) {
        let _ = file.write_all(&serde_json::to_vec_pretty(settings).unwrap_or_default());
        let _ = file.sync_all();
    }
}

pub const THEMES: &[&str] = &["github", "jetbrains", "vscode", "fluent", "reading"];
pub const MODES: &[&str] = &["system", "light", "dark"];

/// Appearance saved by the page: theme, mode and the colors needed before it loads.
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

/// Saves the appearance from a page message (`theme`, `mode`, `caption`, `text`, `background`).
pub fn set_appearance(message: &Value) {
    let mut settings = load();
    for key in ["theme", "mode", "caption", "text", "background"] {
        if let Some(value) = message[key].as_str() {
            settings.insert(key.into(), Value::String(value.to_owned()));
        }
    }
    save(&settings);
}

/// Tabs open in the last run: `{ "tabs": [{ "path", "position" }], "active": path }`.
pub fn session() -> Option<Value> {
    load().get("session").cloned()
}

pub fn set_session(session: Value) {
    let mut settings = load();
    settings.insert("session".into(), session);
    save(&settings);
}

/// `system`, `pl` or `en`.
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

/// Notepad removed from the editor list (× button in the Edit menu).
pub fn notepad_hidden() -> bool {
    load().get("notepadHidden").and_then(Value::as_bool).unwrap_or(false)
}

pub fn set_notepad_hidden(hidden: bool) {
    let mut settings = load();
    settings.insert("notepadHidden".into(), Value::from(hidden));
    save(&settings);
}

/// Whether to check GitHub for a newer version (on by default, switch in the gear menu).
pub fn update_check() -> bool {
    load().get("updateCheck").and_then(Value::as_bool).unwrap_or(true)
}

pub fn set_update_check(enabled: bool) {
    let mut settings = load();
    settings.insert("updateCheck".into(), Value::from(enabled));
    save(&settings);
}

/// Result of the last update check (`version`, `page`, `asset`, `sha256`, `checkedAt` in Unix seconds).
pub fn update_latest() -> Option<Value> {
    load().get("updateLatest").cloned()
}

pub fn set_update_latest(latest: Value) {
    let mut settings = load();
    settings.insert("updateLatest".into(), latest);
    save(&settings);
}

/// Version whose update bar the user hid ("Hide" until a newer one comes out).
pub fn update_dismissed() -> Option<String> {
    load().get("updateDismissed").and_then(Value::as_str).map(str::to_owned)
}

pub fn set_update_dismissed(version: &str) {
    let mut settings = load();
    settings.insert("updateDismissed".into(), Value::from(version));
    save(&settings);
}

/// Version of the last run, so the first run of a newer one opens its release notes.
pub fn last_version() -> Option<String> {
    load().get("lastVersion").and_then(Value::as_str).map(str::to_owned)
}

pub fn set_last_version(version: &str) {
    let mut settings = load();
    settings.insert("lastVersion".into(), Value::from(version));
    save(&settings);
}

/// Main window at the last close: normal (restored) rectangle in workspace coordinates and whether it was maximized.
pub struct WindowPlacement {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub maximized: bool,
}

pub fn window_placement() -> Option<WindowPlacement> {
    let settings = load();
    let window = settings.get("window")?;
    let number = |key: &str| window.get(key)?.as_i64().and_then(|value| i32::try_from(value).ok());
    Some(WindowPlacement {
        left: number("left")?,
        top: number("top")?,
        right: number("right")?,
        bottom: number("bottom")?,
        maximized: window.get("maximized").and_then(Value::as_bool).unwrap_or(false),
    })
}

pub fn set_window_placement(placement: &WindowPlacement) {
    let mut settings = load();
    settings.insert(
        "window".into(),
        serde_json::json!({
            "left": placement.left,
            "top": placement.top,
            "right": placement.right,
            "bottom": placement.bottom,
            "maximized": placement.maximized,
        }),
    );
    save(&settings);
}

/// Width of the outline panel in CSS pixels (shared by all files).
pub fn outline_width() -> Option<u64> {
    load().get("outlineWidth").and_then(Value::as_u64)
}

pub fn set_outline_width(width: u64) {
    let mut settings = load();
    settings.insert("outlineWidth".into(), Value::from(width));
    save(&settings);
}

/// Whether newly opened files show the outline right away (yes by default).
pub fn outline_default() -> bool {
    load().get("outlineDefault").and_then(Value::as_bool).unwrap_or(true)
}

pub fn set_outline_default(open: bool) {
    let mut settings = load();
    settings.insert("outlineDefault".into(), Value::from(open));
    save(&settings);
}

/// `#rrggbb` → COLORREF (0x00BBGGRR).
pub fn parse_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#').filter(|hex| hex.len() == 6)?;
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | (rgb >> 16))
}

/// External editor chosen by the user; `None` means Notepad.
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

/// Editors other than Notepad; `None` until the list has been created (first start).
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
