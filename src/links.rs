//! Ctrl+klik w link: pliki Markdown w karcie, strony w przeglądarce, inne pliki w Eksploratorze.
//! Też „Pokaż w Eksploratorze / Total Commanderze” z menu karty.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};

use crate::i18n::tf;
use crate::markdown::{LOCAL_PREFIX, percent_decode};

const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];
const WEB_SCHEMES: &[&str] = &["http://", "https://", "mailto:"];

pub fn open(url: &str) {
    if let Some((path, anchor)) = local_target(url) {
        if !path.exists() {
            crate::show_error(&tf("error.fileMissing", &[("path", &path.display().to_string())]));
        } else if is_markdown(&path) {
            crate::open_path(&path, anchor.as_deref());
        } else {
            reveal(&path);
        }
    } else if WEB_SCHEMES.iter().any(|scheme| url.starts_with(scheme)) {
        unsafe {
            ShellExecuteW(None, w!("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL);
        }
    }
}

/// Eksplorator z zaznaczonym plikiem (też „Pokaż w Eksploratorze” z menu karty).
pub fn reveal(path: &Path) {
    let _ = Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

/// Total Commander, jeśli zainstalowany (katalog z wpisu instalatora Ghisler w rejestrze).
pub fn total_commander() -> Option<PathBuf> {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(|root| crate::editor::registry_path(root, r"Software\Ghisler\Total Commander", Some("InstallDir")))
        .flat_map(|dir| [dir.join("TOTALCMD64.EXE"), dir.join("TOTALCMD.EXE")])
        .find(|exe| exe.exists())
}

/// Total Commander z kursorem na pliku: w już otwartym oknie (/O), w nowej zakładce (/T) aktywnego panelu
/// (/S: /L to panel aktywny).
pub fn reveal_in_total_commander(path: &Path) {
    if let Some(exe) = total_commander() {
        let _ = Command::new(exe).args(["/O", "/T", "/S"]).raw_arg(format!("/L=\"{}\"", path.display())).spawn();
    }
}

/// Adres do pokazania lub skopiowania: dla plików lokalnych ścieżka, dla reszty adres bez zmian.
pub fn display(url: &str) -> String {
    match local_target(url) {
        Some((path, Some(anchor))) => format!("{}#{anchor}", path.display()),
        Some((path, None)) => path.display().to_string(),
        None => url.to_owned(),
    }
}

fn local_target(url: &str) -> Option<(PathBuf, Option<String>)> {
    let rest = url.strip_prefix(LOCAL_PREFIX)?;
    let (path, anchor) = match rest.split_once('#') {
        Some((path, anchor)) => (path, Some(percent_decode(anchor))),
        None => (rest, None),
    };
    Some((PathBuf::from(percent_decode(path).replace('/', "\\")), anchor))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| MARKDOWN_EXTENSIONS.iter().any(|md| ext.eq_ignore_ascii_case(md)))
}
