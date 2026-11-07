//! Ctrl+click on a link: Markdown files in a tab, web pages in the browser, other files in Explorer.
//! Also "Show in Explorer / Total Commander" from the tab menu.

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

/// Explorer with the file selected (also "Show in Explorer" from the tab menu).
pub fn reveal(path: &Path) {
    let _ = Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

/// Total Commander, if installed (directory from the Ghisler installer's registry entry).
pub fn total_commander() -> Option<PathBuf> {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(|root| crate::editor::registry_path(root, r"Software\Ghisler\Total Commander", Some("InstallDir")))
        .flat_map(|dir| [dir.join("TOTALCMD64.EXE"), dir.join("TOTALCMD.EXE")])
        .find(|exe| exe.exists())
}

/// Total Commander with the cursor on the file: in the already open window (/O), in a new tab (/T) of the
/// active panel (/S: /L is the active panel).
pub fn reveal_in_total_commander(path: &Path) {
    if let Some(exe) = total_commander() {
        let _ = Command::new(exe).args(["/O", "/T", "/S"]).raw_arg(format!("/L=\"{}\"", path.display())).spawn();
    }
}

/// Address to show or copy: the path for local files, the address unchanged for everything else.
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
