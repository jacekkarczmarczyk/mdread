//! Edycja w zewnętrznym programie (mdread sam niczego nie edytuje): lista edytorów (Notatnik,
//! wykryte w systemie, dodane ręcznie), wybór i uruchamianie.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{HSTRING, PCWSTR};

use crate::i18n::{t, tf};
use crate::{exeinfo, settings};

/// Programy szukane w „App Paths” rejestru i w typowych miejscach instalacji.
const KNOWN_EDITORS: &[(&str, &[&str])] = &[
    ("notepad++.exe", &[r"%ProgramFiles%\Notepad++\notepad++.exe"]),
    (
        "Code.exe",
        &[r"%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe", r"%ProgramFiles%\Microsoft VS Code\Code.exe"],
    ),
    ("sublime_text.exe", &[r"%ProgramFiles%\Sublime Text\sublime_text.exe"]),
    ("Notepad3.exe", &[r"%ProgramFiles%\Notepad3\Notepad3.exe"]),
    ("typora.exe", &[r"%ProgramFiles%\Typora\Typora.exe"]),
];

thread_local! {
    /// Ikony i nazwy programów (odczyt z pliku exe jest wolniejszy niż budowa menu).
    static DETAILS: RefCell<HashMap<PathBuf, (String, Option<String>)>> = RefCell::default();
}

/// Notatnik pełną ścieżką z katalogu systemowego, żeby nie uruchomić przypadkiem innego `notepad.exe`.
fn notepad() -> PathBuf {
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(windows).join(r"System32\notepad.exe")
}

/// Program przycisku „Edytuj”: wybrany, bez wyboru Notatnik, a gdy Notatnik usunięto z listy, pierwszy
/// z listy. `None` przy pustej liście (strona wtedy otwiera wybór programu).
fn selected() -> Option<PathBuf> {
    settings::editor()
        .or_else(|| (!settings::notepad_hidden()).then(notepad))
        .or_else(|| editors().into_iter().find(|path| path.exists()))
}

pub fn open(path: &Path) {
    let Some(program) = selected() else { return };
    if let Err(error) = Command::new(&program).arg(path).spawn() {
        let message = tf("error.editor", &[("program", &program.display().to_string())]);
        crate::show_error(&format!("{message}\n\n{error}"));
    }
}

/// Wybiera edytor z listy (Notatnik zapisuje się jako brak wyboru).
pub fn select(program: &Path) {
    settings::set_editor((!same(program, &notepad())).then(|| program.to_owned()));
}

/// Edytory poza Notatnikiem. Przy pierwszym starcie lista powstaje z programów wykrytych w systemie
/// (i edytora wybranego w starszej wersji); potem jest już tylko to, co zapisane w ustawieniach.
fn editors() -> Vec<PathBuf> {
    if let Some(editors) = settings::editors() {
        return editors;
    }
    let mut editors = detected();
    if let Some(chosen) = settings::editor()
        && !editors.iter().any(|path| same(path, &chosen))
    {
        editors.push(chosen);
    }
    settings::set_editors(&editors);
    editors
}

/// Dopisuje program do listy i go wybiera (Notatnik wraca na listę, jeśli był usunięty).
pub fn add(program: &Path) {
    if same(program, &notepad()) {
        settings::set_notepad_hidden(false);
    } else {
        let mut editors = editors();
        if !editors.iter().any(|path| same(path, program)) {
            editors.push(program.to_owned());
            settings::set_editors(&editors);
        }
    }
    select(program);
}

/// Usuwa program z listy (także Notatnik); jeśli był wybrany, „Edytuj” bierze Notatnik albo pierwszy z listy.
pub fn remove(program: &Path) {
    if same(program, &notepad()) {
        settings::set_notepad_hidden(true);
        return;
    }
    let mut editors = editors();
    editors.retain(|path| !same(path, program));
    settings::set_editors(&editors);
    if settings::editor().is_some_and(|chosen| same(&chosen, program)) {
        settings::set_editor(None);
    }
}

/// Lista dla strony: Notatnik (jeśli nie usunięty) i zapisane edytory istniejące na dysku, z nazwą, ikoną
/// i zaznaczeniem.
pub fn info() -> Value {
    let selected = selected();
    let programs = (!settings::notepad_hidden())
        .then(notepad)
        .into_iter()
        .chain(editors().into_iter().filter(|path| path.exists()));
    let items: Vec<Value> = programs
        .map(|path| {
            let (name, icon) = details(&path);
            let is_selected = selected.as_deref().is_some_and(|chosen| same(&path, chosen));
            json!({ "path": path, "name": name, "icon": icon, "selected": is_selected })
        })
        .collect();
    let selected_name = selected.map(|path| details(&path).0);
    json!({ "type": "editors", "items": items, "selectedName": selected_name })
}

fn details(path: &Path) -> (String, Option<String>) {
    if let Some(details) = DETAILS.with_borrow(|cache| cache.get(path).cloned()) {
        return details;
    }
    let name = if same(path, &notepad()) {
        t("editor.notepad")
    } else {
        exeinfo::description(path)
            .unwrap_or_else(|| path.file_stem().unwrap_or(path.as_os_str()).to_string_lossy().into_owned())
    };
    let details = (name, exeinfo::icon_data_url(path, 32));
    DETAILS.with_borrow_mut(|cache| cache.insert(path.to_owned(), details.clone()));
    details
}

/// Edytory zainstalowane w systemie (bez Notatnika); sprawdzane tylko przy pierwszym starcie.
fn detected() -> Vec<PathBuf> {
    KNOWN_EDITORS
        .iter()
        .filter_map(|(exe, fallbacks)| {
            [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
                .into_iter()
                .filter_map(|root| app_path(root, exe))
                .chain(fallbacks.iter().map(|path| expand(path)))
                .find(|path| path.exists())
        })
        .collect()
}

/// Ścieżka programu z „App Paths” (wpis tworzony przez instalatory, np. Notepad++).
fn app_path(root: HKEY, exe: &str) -> Option<PathBuf> {
    registry_path(root, &format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}"), None)
}

/// Ścieżka zapisana w rejestrze (wartość `value`, domyślna gdy `None`), bez otaczających cudzysłowów.
pub fn registry_path(root: HKEY, key: &str, value: Option<&str>) -> Option<PathBuf> {
    let key = HSTRING::from(key);
    let value = value.map(HSTRING::from);
    let name = value.as_ref().map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(root, &key, name, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr() as _), Some(&mut size)).ok().ok()?;
    }
    let text = String::from_utf16_lossy(&buffer[..(size as usize / 2).saturating_sub(1)]);
    Some(PathBuf::from(text.trim_matches('"')))
}

/// Rozwija `%ZMIENNA%` w ścieżce.
fn expand(path: &str) -> PathBuf {
    let mut result = String::new();
    let mut parts = path.split('%');
    result.push_str(parts.next().unwrap_or_default());
    while let (Some(name), Some(rest)) = (parts.next(), parts.next()) {
        result.push_str(&std::env::var(name).unwrap_or_default());
        result.push_str(rest);
    }
    PathBuf::from(result)
}

fn same(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_environment_variables() {
        let windows = std::env::var("SystemRoot").unwrap();
        assert_eq!(expand(r"%SystemRoot%\System32\x.exe"), PathBuf::from(format!(r"{windows}\System32\x.exe")));
        assert_eq!(expand(r"C:\plain\path.exe"), PathBuf::from(r"C:\plain\path.exe"));
    }
}
