//! Interface texts in the languages from `ui/i18n/*.json`; the page gets the same dictionaries (`strings.js`).

use std::sync::{OnceLock, RwLock};

use serde_json::{Map, Value, json};
use windows::Win32::Globalization::GetUserDefaultUILanguage;

use crate::settings;

/// Interface languages (also the choices in the menu); the first is the fallback when a text or the system
/// language is missing.
pub const LANGUAGES: &[&str] = &["en", "pl"];

const LANG_POLISH: u16 = 0x15;

static CURRENT: RwLock<&'static str> = RwLock::new("en");

fn dictionary(language: &str) -> &'static Map<String, Value> {
    static EN: OnceLock<Map<String, Value>> = OnceLock::new();
    static PL: OnceLock<Map<String, Value>> = OnceLock::new();
    let (cell, source) = match language {
        "pl" => (&PL, include_str!("../ui/i18n/pl.json")),
        _ => (&EN, include_str!("../ui/i18n/en.json")),
    };
    cell.get_or_init(|| serde_json::from_str(source).expect("i18n dictionary"))
}

/// Sets the language from the settings. On the first run (no language saved yet, or `system` from older versions)
/// it is the Windows display language, saved right away; from then on only the menu changes it.
pub fn init() {
    let saved = settings::language();
    let language = LANGUAGES.iter().copied().find(|&language| language == saved).unwrap_or_else(|| {
        let language = system_language();
        settings::set_language(language);
        language
    });
    *CURRENT.write().unwrap() = language;
}

fn system_language() -> &'static str {
    // The primary language id is the low 10 bits of LANGID.
    if unsafe { GetUserDefaultUILanguage() } & 0x3FF == LANG_POLISH { "pl" } else { "en" }
}

pub fn current() -> &'static str {
    *CURRENT.read().unwrap()
}

/// Language code for WebView2 (e.g. the built-in context menu items).
pub fn locale() -> &'static str {
    match current() {
        "pl" => "pl-PL",
        _ => "en-US",
    }
}

pub fn t(key: &str) -> String {
    [current(), LANGUAGES[0]]
        .iter()
        .find_map(|language| dictionary(language).get(key).and_then(Value::as_str))
        .unwrap_or(key)
        .to_owned()
}

/// Text with `{name}` substitutions.
pub fn tf(key: &str, params: &[(&str, &str)]) -> String {
    params.iter().fold(t(key), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

/// `strings.js` loaded by the page before `app.js`: the current language's dictionary (filled in
/// from the fallback), the language itself and the setting. Changing the language restarts the app (`crate::restart`).
pub fn strings_script() -> String {
    let mut strings = dictionary(LANGUAGES[0]).clone();
    strings.extend(dictionary(current()).clone());
    let data = json!({ "strings": strings, "language": current(), "setting": settings::language() });
    format!("window.I18N = {data};")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionaries_have_the_same_keys() {
        let en: Vec<_> = dictionary("en").keys().collect();
        let pl: Vec<_> = dictionary("pl").keys().collect();
        assert_eq!(en, pl);
    }

    #[test]
    fn substitutes_parameters() {
        *CURRENT.write().unwrap() = "en";
        assert_eq!(tf("edit.title", &[("name", "Notepad")]), "Edit in: Notepad (Ctrl+E)");
    }
}
