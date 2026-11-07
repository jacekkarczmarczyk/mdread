#![windows_subsystem = "windows"]

mod assets;
mod assoc;
mod clipboard;
mod editor;
mod exeinfo;
mod i18n;
mod install;
mod instance;
mod links;
mod menu;
mod markdown;
mod settings;
mod timing;
mod unc;
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
    // Strona instalacji: na życzenie albo gdy pobrany exe uruchomiono bez plików (build debug wtedy
    // otwiera zwykłe puste okno, żeby nie przeszkadzać przy pracy).
    let installer = flag(&args, "--install")
        || (args.is_empty() && !install::is_installed() && !cfg!(debug_assertions));
    args.retain(|arg| arg != "--install");
    // `--after <pid>`: ponowne uruchomienie (np. po zmianie języka) czeka, aż poprzednie okno się zamknie,
    // inaczej oddałoby mu pliki jako druga instancja.
    if let Some(index) = args.iter().position(|arg| arg == "--after") {
        let pid = args.get(index + 1).and_then(|pid| pid.to_str()?.parse().ok());
        args.drain(index..(index + 2).min(args.len()));
        if let Some(pid) = pid {
            wait_for_exit(pid);
        }
    }
    let paths: Vec<PathBuf> = args
        .into_iter()
        .map(|arg| std::path::absolute(&arg).unwrap_or_else(|_| PathBuf::from(arg)))
        .collect();
    // Instalator działa obok ewentualnie uruchomionego MDRead, nie oddaje mu sterowania.
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

    // Callback środowiska potrafi przyjść już w trakcie tworzenia okna, więc start dopiero, gdy HWND istnieje.
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
    if !installer {
        restore_session();
    }
    open_paths(&paths);
    // Od teraz strona zapisuje sesję (wcześniej każda odtwarzana karta oznaczałaby osobny zapis na dysk).
    webview::send(json!({ "type": "startupDone" }));
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

/// Uruchamia mdread od nowa z tymi samymi kartami (ostatnia ścieżka to karta aktywna, którą strona
/// wtedy przełącza) i zamyka bieżące okno.
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

/// `mdread.exe --register [--quiet]`: wpisy w rejestrze i (bez `--quiet`) pytanie o otwarcie
/// ustawień aplikacji domyślnych, bo domyślny program Windows każe wybrać użytkownikowi.
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

/// Ustawienia → Aplikacje domyślne na stronie MDRead (domyślny program wybiera użytkownik).
pub fn open_default_apps() {
    unsafe {
        ShellExecuteW(None, w!("open"), w!("ms-settings:defaultapps?registeredAppUser=mdread"), None, None, SW_SHOWNORMAL);
    }
}

/// Karty z poprzedniego uruchomienia (jak w Notatniku): w tej samej kolejności, z pozycją czytania,
/// na końcu ponownie aktywna karta (strona przełącza na już otwartą). Pliki, których nie ma, są pomijane.
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

/// Otwiera pliki w kartach (strona przełącza na już otwartą kartę tego samego pliku).
pub fn open_paths(paths: &[PathBuf]) {
    for path in paths {
        open_path(path, None);
    }
}

/// Od tego rozmiaru karta najpierw pyta (pomiar: ~1 s otwierania na każdy MB, MDR-19).
const WARN_SIZE: u64 = 2 * 1024 * 1024;
/// Od tego rozmiaru podgląd odmawia i proponuje edytor.
const MAX_SIZE: u64 = 20 * 1024 * 1024;

/// Otwiera plik w karcie i przewija do kotwicy `anchor`, jeśli podana. Duży albo binarny plik
/// dostaje zamiast treści komunikat z wyborem (patrz [`notice`]).
pub fn open_path(path: &Path, anchor: Option<&str>) {
    open_document(path, anchor, None);
}

/// Jak [`open_path`], z pozycją czytania do odtworzenia (karta z poprzedniego uruchomienia).
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

/// Wczytuje plik ponownie do jego karty (po „Odśwież” albo po potwierdzeniu dużego pliku).
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

/// Komunikat zamiast treści: plik binarny, za duży albo (bez `confirmed`) duży do potwierdzenia.
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

/// Treść pliku jako tekst (UTF-8, bez BOM).
pub fn read_source(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

/// Komunikat błędu nad oknem aplikacji (albo samodzielny, jeśli okna jeszcze nie ma).
pub fn show_error(message: &str) {
    unsafe {
        MessageBoxW(webview::window(), &HSTRING::from(message), w!("MDRead"), MB_ICONERROR);
    }
}

pub fn fatal(message: &str) -> ! {
    show_error(message);
    std::process::exit(1);
}
