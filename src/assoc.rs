//! Rejestracja dla plików Markdown w rejestrze bieżącego użytkownika (HKCU, bez uprawnień administratora).
//! Windows nie pozwala programowi ustawić się jako domyślny; mdread pojawia się w „Otwórz za pomocą”
//! i w Ustawieniach → Aplikacje domyślne, gdzie użytkownik wybiera go sam.

use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_SZ, RegCloseKey, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW,
    RegOpenKeyExW, RegQueryInfoKeyW, RegSetKeyValueW,
};
use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
use windows::core::{HSTRING, PCWSTR, Result};

const EXTENSIONS: &[&str] = &[".md", ".markdown", ".mdown", ".mkd"];
const PROG_ID: &str = "mdread.md";
const PROG_ID_KEY: &str = r"Software\Classes\mdread.md";
const APPLICATION_KEY: &str = r"Software\Classes\Applications\mdread.exe";
const APP_KEY: &str = r"Software\mdread";
const CAPABILITIES_KEY: &str = r"Software\mdread\Capabilities";
const REGISTERED_APPLICATIONS_KEY: &str = r"Software\RegisteredApplications";

pub fn register() -> Result<()> {
    let exe = std::env::current_exe()?;
    let command = format!("\"{}\" \"%1\"", exe.display());
    let icon = format!("\"{}\",0", exe.display());

    set(PROG_ID_KEY, None, "Dokument Markdown")?;
    set(&format!(r"{PROG_ID_KEY}\DefaultIcon"), None, &icon)?;
    set(&format!(r"{PROG_ID_KEY}\shell\open\command"), None, &command)?;

    set(APPLICATION_KEY, Some("FriendlyAppName"), "mdread")?;
    set(&format!(r"{APPLICATION_KEY}\shell\open\command"), None, &command)?;

    set(CAPABILITIES_KEY, Some("ApplicationName"), "mdread")?;
    set(CAPABILITIES_KEY, Some("ApplicationDescription"), "Przeglądarka plików Markdown")?;
    for extension in EXTENSIONS {
        set(&format!(r"Software\Classes\{extension}\OpenWithProgids"), Some(PROG_ID), "")?;
        set(&format!(r"{APPLICATION_KEY}\SupportedTypes"), Some(extension), "")?;
        set(&format!(r"{CAPABILITIES_KEY}\FileAssociations"), Some(extension), PROG_ID)?;
    }
    set(REGISTERED_APPLICATIONS_KEY, Some("mdread"), CAPABILITIES_KEY)?;

    notify_shell();
    Ok(())
}

/// Usuwa wszystko, co dodało [`register`]. Brak wpisów nie jest błędem.
pub fn unregister() {
    unsafe {
        for key in [PROG_ID_KEY, APPLICATION_KEY, APP_KEY] {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key));
        }
        for extension in EXTENSIONS {
            let key = format!(r"Software\Classes\{extension}\OpenWithProgids");
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(&key), &HSTRING::from(PROG_ID));
            // Klucze utworzone przy rejestracji znikają, o ile nie zostało w nich nic innego.
            delete_if_empty(&key);
            delete_if_empty(&format!(r"Software\Classes\{extension}"));
        }
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(REGISTERED_APPLICATIONS_KEY), &HSTRING::from("mdread"));
    }
    notify_shell();
}

fn set(key: &str, name: Option<&str>, value: &str) -> Result<()> {
    let data: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let name = name.map(HSTRING::from);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            name.as_ref().map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr())),
            REG_SZ.0,
            Some(data.as_ptr() as _),
            (data.len() * 2) as u32,
        )
        .ok()
    }
}

fn delete_if_empty(key: &str) {
    let key = HSTRING::from(key);
    unsafe {
        let mut handle = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, &key, None, KEY_READ, &mut handle).is_err() {
            return;
        }
        let (mut subkeys, mut values) = (0u32, 0u32);
        let info = RegQueryInfoKeyW(
            handle, None, None, None, Some(&mut subkeys), None, None, Some(&mut values), None, None, None, None,
        );
        let _ = RegCloseKey(handle);
        if info.is_ok() && subkeys == 0 && values == 0 {
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &key);
        }
    }
}

fn notify_shell() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}
