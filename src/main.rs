#![windows_subsystem = "windows"]

mod assets;
mod assoc;
mod clipboard;
mod editor;
mod exeinfo;
mod help;
mod i18n;
mod install;
mod instance;
mod links;
mod menu;
mod markdown;
mod settings;
mod timing;
mod unc;
mod update;
mod watcher;
mod webview;
mod window;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use i18n::t;
use serde_json::{Value, json};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_YESNO, MessageBoxW, SW_SHOWNORMAL,
};
use windows::core::{HSTRING, w};

fn main() {
    timing::mark("main");
    i18n::init();
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let flag = |args: &[OsString], name: &str| args.iter().any(|arg| arg == name);
    if flag(&args, "--register") {
        return register(flag(&args, "--quiet"));
    }
    if flag(&args, "--unregister") {
        return assoc::unregister();
    }
    if flag(&args, "--uninstall") {
        return install::uninstall();
    }
    // Installer page: on request, or when a downloaded exe is started without files (a debug build then
    // opens a plain empty window so it does not get in the way during development).
    let installer = flag(&args, "--install")
        || (args.is_empty() && !install::is_installed() && !cfg!(debug_assertions));
    args.retain(|arg| arg != "--install");
    // `--welcome`: the first run after installation opens the help.
    let welcome = flag(&args, "--welcome");
    args.retain(|arg| arg != "--welcome");
    // `--after <pid>`: a restart (e.g. after a language change) waits for the previous window to close,
    // otherwise it would hand the files over to it as a second instance.
    if let Some(index) = args.iter().position(|arg| arg == "--after") {
        let pid = args.get(index + 1).and_then(|pid| pid.to_str()?.parse().ok());
        args.drain(index..(index + 2).min(args.len()));
        if let Some(pid) = pid {
            wait_for_exit(pid);
        }
    }
    // `--update`: a downloaded new version replaces the installed copy and starts it (see update.rs).
    if flag(&args, "--update") {
        return update::apply();
    }
    let paths: Vec<PathBuf> = args
        .into_iter()
        .map(|arg| std::path::absolute(&arg).unwrap_or_else(|_| PathBuf::from(arg)))
        .collect();
    // The installer runs alongside any MDRead already running and does not hand control over to it.
    if !installer && !instance::claim(&paths) {
        return;
    }

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if let Err(error) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            fatal(&format!("CoInitializeEx: {error}"));
        }
    }

    let hwnd = window::create(installer).unwrap_or_else(|error| fatal(&format!("CreateWindow: {error}")));
    webview::set_installer(installer);
    timing::mark("window shown");
    watcher::start(hwnd);

    // The environment callback can arrive while the window is still being created, so start only once the HWND exists.
    if let Err(error) = webview::start(hwnd) {
        fatal(&format!("{}\n\n{error}", t("error.webviewMissing")));
    }

    let editors = editor::info();
    if installer {
        webview::send(json!({
            "type": "installer",
            "editors": editors["items"],
            "directory": install::install_dir(),
        }));
    }
    webview::send(editors);
    // The "Show in Total Commander" tab menu item only when it is installed.
    webview::send(json!({ "type": "totalCommander", "available": links::total_commander().is_some() }));
    if !installer {
        help::path();
        restore_session();
    }
    open_paths(&paths);
    if welcome {
        help::open();
    }
    // From now on the page saves the session (before this, every restored tab would mean a separate disk write).
    webview::send(json!({ "type": "startupDone" }));
    if !installer {
        update::start(hwnd);
    }
    window::run_loop();
}

fn wait_for_exit(pid: u32) {
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, 5000);
            let _ = CloseHandle(process);
        }
    }
}

/// Restarts mdread with the same tabs (the last path is the active tab, which the page then
/// switches to) and closes the current window.
pub fn restart(paths: &[PathBuf], active: Option<&Path>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut command = Command::new(exe);
    command.arg("--after").arg(std::process::id().to_string()).args(paths);
    if let Some(active) = active {
        command.arg(active);
    }
    if command.spawn().is_ok()
        && let Some(hwnd) = webview::window()
    {
        window::request_close(hwnd);
    }
}

/// `mdread.exe --register [--quiet]`: registry entries and (without `--quiet`) an offer to open the
/// default apps settings, because Windows makes the user choose the default program.
fn register(quiet: bool) {
    let registered = std::env::current_exe().map_err(Into::into).and_then(|exe| assoc::register(&exe));
    if let Err(error) = registered {
        if !quiet {
            show_error(&format!("{}\n\n{error}", t("register.failed")));
        }
        std::process::exit(1);
    }
    if quiet {
        return;
    }
    let answer = unsafe {
        MessageBoxW(None, &HSTRING::from(t("register.done")), w!("MDRead"), MB_YESNO | MB_ICONINFORMATION)
    };
    if answer == IDYES {
        open_default_apps();
    }
}

/// Settings → Default apps on the MDRead page (the user chooses the default program).
pub fn open_default_apps() {
    unsafe {
        ShellExecuteW(None, w!("open"), w!("ms-settings:defaultapps?registeredAppUser=mdread"), None, None, SW_SHOWNORMAL);
    }
}

/// Tabs from the previous run (as in Notepad): in the same order, with the reading position, and
/// the active tab again at the end (the page switches to the already open one). Missing files are skipped.
fn restore_session() {
    let Some(session) = settings::session() else { return };
    for tab in session["tabs"].as_array().into_iter().flatten() {
        let Some(path) = tab["path"].as_str().map(Path::new).filter(|path| path.exists()) else { continue };
        open_document(path, None, Some(&tab["position"]));
    }
    if let Some(active) = session["active"].as_str().map(Path::new).filter(|path| path.exists()) {
        open_path(active, None);
    }
}

/// Opens files in tabs (the page switches to an already open tab of the same file).
pub fn open_paths(paths: &[PathBuf]) {
    for path in paths {
        open_path(path, None);
    }
}

/// From this size on the tab asks first (measured: ~1 s of opening per MB, MDR-19).
const WARN_SIZE: u64 = 2 * 1024 * 1024;
/// From this size on the preview refuses and suggests an editor.
const MAX_SIZE: u64 = 20 * 1024 * 1024;

/// Opens a file in a tab and scrolls to `anchor`, if given. A large or binary file gets
/// a message with choices instead of its content (see [`notice`]).
pub fn open_path(path: &Path, anchor: Option<&str>) {
    open_document(path, anchor, None);
}

/// Like [`open_path`], with a reading position to restore (a tab from the previous run).
fn open_document(path: &Path, anchor: Option<&str>, position: Option<&Value>) {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    unc::trust_document(&path);
    let mut message = match notice(&path, false) {
        Some(notice) => json!({ "type": "open", "path": path, "title": file_name(&path), "notice": notice }),
        None => document_message("open", &path),
    };
    message["anchor"] = json!(anchor);
    message["position"] = position.cloned().unwrap_or(Value::Null);
    if message.get("html").is_some() {
        watcher::watch(&path);
    }
    webview::send(message);
}

/// Reloads a file into its tab (after "Reload" or after a large file is confirmed).
pub fn reload_path(path: &Path) {
    let message = match notice(path, true) {
        Some(notice) => json!({ "type": "reload", "path": path, "title": file_name(path), "notice": notice }),
        None => {
            watcher::watch(path);
            document_message("reload", path)
        }
    };
    webview::send(message);
}

/// A message instead of content: a binary file, a too large one, or (without `confirmed`) a large one to confirm.
fn notice(path: &Path, confirmed: bool) -> Option<Value> {
    use std::io::Read;

    let size = std::fs::metadata(path).ok()?.len();
    let megabytes = size as f64 / (1024.0 * 1024.0);
    let mut start = Vec::with_capacity(8192);
    let _ = std::fs::File::open(path).ok()?.take(8192).read_to_end(&mut start);
    let kind = if size >= MAX_SIZE {
        "tooLarge"
    } else if start.contains(&0) {
        "binary"
    } else if size >= WARN_SIZE && !confirmed {
        "large"
    } else {
        return None;
    };
    Some(json!({ "kind": kind, "megabytes": megabytes, "limit": MAX_SIZE / (1024 * 1024) }))
}

fn document_message(kind: &str, path: &Path) -> Value {
    let title = file_name(path);
    match read_source(path) {
        Ok(source) => {
            let html = markdown::render(&source, path.parent().unwrap_or(path));
            timing::mark("markdown rendered");
            json!({ "type": kind, "path": path, "title": title, "html": html })
        }
        Err(error) => json!({ "type": kind, "path": path, "title": title, "error": error.to_string() }),
    }
}

/// File content as text (UTF-8, without BOM).
pub fn read_source(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

/// Error message box over the app window (or standalone if there is no window yet).
pub fn show_error(message: &str) {
    unsafe {
        MessageBoxW(webview::window(), &HSTRING::from(message), w!("MDRead"), MB_ICONERROR);
    }
}

pub fn fatal(message: &str) -> ! {
    show_error(message);
    std::process::exit(1);
}
