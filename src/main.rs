#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod instance;
mod markdown;
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

    open_paths(&paths);
    window::run_loop();
}

/// Otwiera pliki w kartach (strona przełącza na już otwartą kartę tego samego pliku).
pub fn open_paths(paths: &[PathBuf]) {
    for path in paths {
        webview::send(open_message(path));
    }
}

fn open_message(path: &Path) -> Value {
    let title = file_name(path);
    match std::fs::read(path) {
        Ok(bytes) => {
            let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
            let source = String::from_utf8_lossy(bytes);
            let html = markdown::render(&source);
            timing::mark("markdown rendered");
            json!({ "type": "open", "path": path, "title": title, "html": html })
        }
        Err(error) => json!({ "type": "open", "path": path, "title": title, "error": error.to_string() }),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

pub fn fatal(message: &str) -> ! {
    unsafe {
        MessageBoxW(None, &HSTRING::from(message), w!("mdread"), MB_ICONERROR);
    }
    std::process::exit(1);
}
