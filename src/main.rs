#![windows_subsystem = "windows"]

mod clipboard;
mod editor;
mod instance;
mod links;
mod menu;
mod markdown;
mod settings;
mod timing;
mod webview;
mod window;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW};
use windows::core::{HSTRING, w};

fn main() {
    timing::mark("main");
    let paths: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(|arg| std::path::absolute(&arg).unwrap_or_else(|_| PathBuf::from(arg)))
        .collect();
    if !instance::claim(&paths) {
        return;
    }

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if let Err(error) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            fatal(&format!("CoInitializeEx: {error}"));
        }
    }

    let hwnd = window::create().unwrap_or_else(|error| fatal(&format!("CreateWindow: {error}")));
    timing::mark("window shown");

    // Callback środowiska potrafi przyjść już w trakcie tworzenia okna, więc start dopiero, gdy HWND istnieje.
    if let Err(error) = webview::start(hwnd) {
        fatal(&format!(
            "Nie znaleziono środowiska WebView2. Zainstaluj Microsoft Edge WebView2 Runtime.\n\n{error}"
        ));
    }

    webview::send(editor::info());
    open_paths(&paths);
    window::run_loop();
}

/// Otwiera pliki w kartach (strona przełącza na już otwartą kartę tego samego pliku).
pub fn open_paths(paths: &[PathBuf]) {
    for path in paths {
        open_path(path, None);
    }
}

/// Otwiera plik w karcie i przewija do kotwicy `anchor`, jeśli podana.
pub fn open_path(path: &Path, anchor: Option<&str>) {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    webview::send(open_message(&path, anchor));
}

fn open_message(path: &Path, anchor: Option<&str>) -> Value {
    let title = file_name(path);
    match read_source(path) {
        Ok(source) => {
            let html = markdown::render(&source, path.parent().unwrap_or(path));
            timing::mark("markdown rendered");
            json!({ "type": "open", "path": path, "title": title, "html": html, "anchor": anchor })
        }
        Err(error) => json!({ "type": "open", "path": path, "title": title, "error": error.to_string() }),
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
        MessageBoxW(webview::window(), &HSTRING::from(message), w!("mdread"), MB_ICONERROR);
    }
}

pub fn fatal(message: &str) -> ! {
    show_error(message);
    std::process::exit(1);
}
