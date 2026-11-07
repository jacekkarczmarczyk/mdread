//! Registration for Markdown files in the current user's registry (HKCU, no administrator rights).
//! Windows doesn't let a program set itself as the default; mdread shows up in "Open with"
//! and in Settings → Default apps, where the user picks it themselves.

use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_SZ, RegCloseKey, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW,
    RegOpenKeyExW, RegQueryInfoKeyW, RegSetKeyValueW,
};
use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
use windows::core::{HSTRING, PCWSTR, Result};

use std::path::Path;

use crate::i18n::t;

const EXTENSIONS: &[&str] = &[".md", ".markdown", ".mdown", ".mkd"];
const PROG_ID: &str = "mdread.md";
const PROG_ID_KEY: &str = r"Software\Classes\mdread.md";
const APPLICATION_KEY: &str = r"Software\Classes\Applications\mdread.exe";
const APP_KEY: &str = r"Software\mdread";
const CAPABILITIES_KEY: &str = r"Software\mdread\Capabilities";
const REGISTERED_APPLICATIONS_KEY: &str = r"Software\RegisteredApplications";

/// Registers `exe` (the running program or the installed copy) for Markdown files.
pub fn register(exe: &Path) -> Result<()> {
    let command = format!("\"{}\" \"%1\"", exe.display());
    let icon = format!("\"{}\",0", exe.display());

    set(PROG_ID_KEY, None, &t("register.documentType"))?;
    set(&format!(r"{PROG_ID_KEY}\DefaultIcon"), None, &icon)?;
    set(&format!(r"{PROG_ID_KEY}\shell\open\command"), None, &command)?;

    set(APPLICATION_KEY, Some("FriendlyAppName"), "MDRead")?;
    set(&format!(r"{APPLICATION_KEY}\shell\open\command"), None, &command)?;

    set(CAPABILITIES_KEY, Some("ApplicationName"), "MDRead")?;
    set(CAPABILITIES_KEY, Some("ApplicationDescription"), &t("register.description"))?;
    for extension in EXTENSIONS {
        set(&format!(r"Software\Classes\{extension}\OpenWithProgids"), Some(PROG_ID), "")?;
        set(&format!(r"{APPLICATION_KEY}\SupportedTypes"), Some(extension), "")?;
        set(&format!(r"{CAPABILITIES_KEY}\FileAssociations"), Some(extension), PROG_ID)?;
    }
    set(REGISTERED_APPLICATIONS_KEY, Some("mdread"), CAPABILITIES_KEY)?;

    notify_shell();
    Ok(())
}

/// Removes everything [`register`] added. Missing entries are not an error.
pub fn unregister() {
    unsafe {
        for key in [PROG_ID_KEY, APPLICATION_KEY, APP_KEY] {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key));
        }
        for extension in EXTENSIONS {
            let key = format!(r"Software\Classes\{extension}\OpenWithProgids");
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(&key), &HSTRING::from(PROG_ID));
            // Keys created during registration are removed unless something else is left in them.
            delete_if_empty(&key);
            delete_if_empty(&format!(r"Software\Classes\{extension}"));
        }
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(REGISTERED_APPLICATIONS_KEY), &HSTRING::from("mdread"));
    }
    notify_shell();
}

/// Numeric value (REG_DWORD) in HKCU.
pub fn set_dword(key: &str, name: &str, value: u32) -> Result<()> {
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            REG_DWORD.0,
            Some(&value as *const u32 as _),
            4,
        )
        .ok()
    }
}

/// String value (REG_SZ) in HKCU; `None` as the name means the key's default value.
pub fn set(key: &str, name: Option<&str>, value: &str) -> Result<()> {
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
