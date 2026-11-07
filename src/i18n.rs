//! Teksty interfejsu w językach z `ui/i18n/*.json`; te same słowniki dostaje strona (`strings.js`).

use std::sync::{OnceLock, RwLock};

use serde_json::{Map, Value, json};

/// Ustawienia języka do wyboru w menu (`system` oznacza język interfejsu Windows).
pub const SETTINGS: &[&str] = &["system", "en", "pl"];
use windows::Win32::Globalization::GetUserDefaultUILanguage;

use crate::settings;

/// Języki interfejsu; pierwszy jest zapasowy, gdy brakuje tekstu albo języka systemu.
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
    cell.get_or_init(|| serde_json::from_str(source).expect("słownik i18n"))
}

/// Ustala język z ustawień (`system` → język interfejsu Windows).
pub fn init() {
    let language = match settings::language().as_str() {
        "system" => system_language(),
        chosen => LANGUAGES.iter().copied().find(|&language| language == chosen).unwrap_or(LANGUAGES[0]),
    };
    *CURRENT.write().unwrap() = language;
}

fn system_language() -> &'static str {
    // Główny identyfikator języka to dolne 10 bitów LANGID.
    if unsafe { GetUserDefaultUILanguage() } & 0x3FF == LANG_POLISH { "pl" } else { "en" }
}

pub fn current() -> &'static str {
    *CURRENT.read().unwrap()
}

/// Kod języka dla WebView2 (np. wbudowane pozycje menu kontekstowego).
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

/// Tekst z podstawieniami `{nazwa}`.
pub fn tf(key: &str, params: &[(&str, &str)]) -> String {
    params.iter().fold(t(key), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

/// `strings.js` ładowany przez stronę przed `app.js`: słownik bieżącego języka (z uzupełnieniem
/// z zapasowego), sam język i ustawienie. Zmiana języka uruchamia aplikację od nowa (`crate::restart`).
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
