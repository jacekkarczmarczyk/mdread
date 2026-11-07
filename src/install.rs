//! Instalator wbudowany w exe (MDR-25): kopia do %LOCALAPPDATA%\Programs\MDRead, skróty, rejestracja,
//! wpis w „Aplikacje i funkcje”; odinstalowanie przez `--uninstall`. Wszystko w profilu użytkownika (HKCU).

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteTreeW};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink,
};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::Sleep;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IDYES, MB_ICONQUESTION, MB_YESNO, MessageBoxW, PostMessageW, WM_CLOSE,
};
use windows::core::{GUID, HSTRING, Interface, Result, w};

use crate::i18n::t;
use crate::window::CLASS_NAME;
use crate::{assoc, editor};

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MDRead";
const SHORTCUT: &str = "MDRead.lnk";
/// Proces bez okna konsoli (do sprzątania katalogu po odinstalowaniu).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn install_dir() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Programs\MDRead")
}

fn installed_exe() -> PathBuf {
    install_dir().join("mdread.exe")
}

/// Czy działa kopia zainstalowana (a nie np. plik pobrany do Pobranych).
pub fn is_installed() -> bool {
    std::env::current_exe().is_ok_and(|exe| same(&exe, &installed_exe()))
}

/// Wybory ze strony instalacji: `associate`, `editor` (ścieżka), `startMenu`, `desktop`.
pub fn install(choices: &Value) -> std::result::Result<(), String> {
    let source = std::env::current_exe().map_err(|error| error.to_string())?;
    let target = installed_exe();
    std::fs::create_dir_all(install_dir()).map_err(|error| error.to_string())?;
    if !same(&source, &target) {
        // Działająca zainstalowana kopia blokuje plik.
        std::fs::copy(&source, &target).map_err(|error| format!("{}\n\n{error}", t("install.inUse")))?;
    }

    let flag = |name: &str| choices[name].as_bool().unwrap_or(false);
    for (folder, wanted) in [(&FOLDERID_Programs, flag("startMenu")), (&FOLDERID_Desktop, flag("desktop"))] {
        let Some(shortcut) = known_folder(folder).map(|dir| dir.join(SHORTCUT)) else { continue };
        if wanted {
            create_shortcut(&shortcut, &target).map_err(|error| error.to_string())?;
        } else {
            let _ = std::fs::remove_file(shortcut);
        }
    }

    if flag("associate") {
        assoc::register(&target).map_err(|error| error.to_string())?;
    }
    if let Some(program) = choices["editor"].as_str() {
        editor::select(Path::new(program));
    }
    register_uninstall(&target).map_err(|error| error.to_string())?;
    Ok(())
}

/// Uruchamia zainstalowaną kopię (po instalacji) i, jeśli trzeba, ustawienia aplikacji domyślnych.
pub fn launch_installed(open_default_apps: bool) {
    let mut command = Command::new(installed_exe());
    command.arg("--after").arg(std::process::id().to_string()).arg("--welcome");
    let _ = command.spawn();
    if open_default_apps {
        crate::open_default_apps();
    }
}

fn register_uninstall(exe: &Path) -> Result<()> {
    let exe_text = exe.display().to_string();
    assoc::set(UNINSTALL_KEY, Some("DisplayName"), "MDRead")?;
    assoc::set(UNINSTALL_KEY, Some("DisplayIcon"), &format!("\"{exe_text}\",0"))?;
    assoc::set(UNINSTALL_KEY, Some("DisplayVersion"), env!("CARGO_PKG_VERSION"))?;
    assoc::set(UNINSTALL_KEY, Some("Publisher"), "MDRead")?;
    assoc::set(UNINSTALL_KEY, Some("InstallLocation"), &install_dir().display().to_string())?;
    assoc::set(UNINSTALL_KEY, Some("UninstallString"), &format!("\"{exe_text}\" --uninstall"))?;
    assoc::set_dword(UNINSTALL_KEY, "NoModify", 1)?;
    assoc::set_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    let size_kb = std::fs::metadata(exe).map(|metadata| metadata.len() / 1024).unwrap_or(0);
    assoc::set_dword(UNINSTALL_KEY, "EstimatedSize", size_kb as u32)
}

/// `mdread.exe --uninstall`: po potwierdzeniu usuwa rejestrację, skróty, wpis odinstalowania, dane WebView2
/// i katalog programu (ustawienia w %APPDATA%\mdread zostają, jak w większości programów).
pub fn uninstall() {
    let answer = unsafe { MessageBoxW(None, &HSTRING::from(t("uninstall.confirm")), w!("MDRead"), MB_YESNO | MB_ICONQUESTION) };
    if answer != IDYES {
        return;
    }
    close_running();
    assoc::unregister();
    for folder in [&FOLDERID_Programs, &FOLDERID_Desktop] {
        if let Some(dir) = known_folder(folder) {
            let _ = std::fs::remove_file(dir.join(SHORTCUT));
        }
    }
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY));
    }
    let webview_data = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("mdread");
    let dir = install_dir();
    // Działający exe nie usunie sam siebie: robi to cmd chwilę po zakończeniu tego procesu.
    // Usuwamy tylko dokładnie te katalogi, i tylko gdy w katalogu programu jest nasz exe.
    if installed_exe().exists() {
        let script = format!(
            "ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\" & rmdir /s /q \"{}\"",
            dir.display(),
            webview_data.display()
        );
        let _ = Command::new("cmd.exe").raw_arg("/c").raw_arg(script).creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

/// Zamyka otwarte okna MDRead (zablokowany exe nie dałby się usunąć); czeka do ~3 s.
fn close_running() {
    for _ in 0..30 {
        let Ok(hwnd) = (unsafe { FindWindowW(CLASS_NAME, None) }) else { return };
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            Sleep(100);
        }
    }
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as _));
        text.map(PathBuf::from)
    }
}

fn create_shortcut(shortcut: &Path, target: &Path) -> Result<()> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(target.as_os_str()))?;
        link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0)?;
        link.SetDescription(&HSTRING::from(t("register.description")))?;
        link.cast::<IPersistFile>()?.Save(&HSTRING::from(shortcut.as_os_str()), true)
    }
}

fn same(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}
