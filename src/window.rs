use std::path::PathBuf;

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, IDC_ARROW,
    IsIconic, LoadCursorW, LoadIconW, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_RESTORE,
    SW_SHOWDEFAULT, SWP_NOACTIVATE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos,
    SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_COPYDATA,
    WM_DESTROY, WM_DPICHANGED, WM_MOVE, WM_SETFOCUS, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{BOOL, HSTRING, PCWSTR, Result, w};

use crate::i18n::t;
use crate::{editor, instance, settings, watcher, webview};

pub const CLASS_NAME: PCWSTR = w!("mdread.window");

/// Okna dialogowe pokazujemy z pętli komunikatów, a nie z callbacku WebView2.
const WM_APP_OPEN_DIALOG: u32 = WM_APP + 1;
const WM_APP_CHOOSE_EDITOR: u32 = WM_APP + 2;

/// Filtry okna dialogowego: (klucz nazwy w słowniku, wzorzec).
const MARKDOWN_FILTERS: &[(&str, &str)] =
    &[("dialog.markdown", "*.md;*.markdown;*.mdown;*.mkd"), ("dialog.allFiles", "*.*")];
const PROGRAM_FILTERS: &[(&str, &str)] = &[("dialog.programs", "*.exe")];

pub fn create() -> Result<HWND> {
    let appearance = settings::appearance();
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        // Tło w kolorze ostatniego motywu, żeby przed załadowaniem strony nie błysnęło białe okno.
        let background = appearance.background.map(|color| CreateSolidBrush(COLORREF(color))).unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // Ikona z zasobów exe (identyfikator 1, patrz build.rs).
            hIcon: LoadIconW(Some(instance), PCWSTR(1 as _)).unwrap_or_default(),
            hbrBackground: background,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("MDRead"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(instance),
            None,
        )?;
        apply_frame(hwnd, &appearance);
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        Ok(hwnd)
    }
}

/// Pasek tytułu w kolorach motywu: tryb ciemny/jasny ramki oraz (Windows 11) kolor tła i tekstu.
pub fn apply_frame(hwnd: HWND, appearance: &settings::Appearance) {
    let dark = if appearance.mode == "system" { system_dark() } else { appearance.mode == "dark" };
    unsafe {
        let dark = BOOL::from(dark);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, size_of::<BOOL>() as u32);
        for (attribute, color) in [(DWMWA_CAPTION_COLOR, appearance.caption), (DWMWA_TEXT_COLOR, appearance.text)] {
            if let Some(color) = color {
                let color = COLORREF(color);
                let _ = DwmSetWindowAttribute(hwnd, attribute, &color as *const _ as _, size_of::<COLORREF>() as u32);
            }
        }
    }
}

/// Czy Windows używa ciemnego motywu aplikacji.
pub fn system_dark() -> bool {
    let mut value = 1u32;
    let mut size = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut _ as _),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 0
}

pub fn run_loop() {
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn set_title(hwnd: HWND, document: Option<&str>) {
    let title = match document {
        Some(document) => format!("{document} - MDRead"),
        None => "MDRead".to_owned(),
    };
    unsafe {
        let _ = SetWindowTextW(hwnd, &HSTRING::from(title));
    }
}

pub fn request_open_dialog(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_APP_OPEN_DIALOG, WPARAM(0), LPARAM(0));
    }
}

pub fn request_choose_editor(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_APP_CHOOSE_EDITOR, WPARAM(0), LPARAM(0));
    }
}

pub fn request_close(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn bring_to_front(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

fn show_open_dialog(hwnd: HWND, filters: &[(&str, &str)], multiple: bool) -> Result<Vec<PathBuf>> {
    // Teksty filtrów muszą żyć do końca wywołania SetFileTypes.
    let texts: Vec<(HSTRING, HSTRING)> =
        filters.iter().map(|(name, spec)| (HSTRING::from(t(name)), HSTRING::from(*spec))).collect();
    let filters: Vec<COMDLG_FILTERSPEC> = texts
        .iter()
        .map(|(name, spec)| COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) })
        .collect();
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut options = dialog.GetOptions()? | FOS_FILEMUSTEXIST;
        if multiple {
            options |= FOS_ALLOWMULTISELECT;
        }
        dialog.SetOptions(options)?;
        dialog.SetFileTypes(&filters)?;
        if dialog.Show(Some(hwnd)).is_err() {
            return Ok(Vec::new()); // anulowane
        }
        let items = dialog.GetResults()?;
        let mut paths = Vec::new();
        for index in 0..items.GetCount()? {
            let name = items.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)?;
            paths.push(PathBuf::from(name.to_string()?));
            CoTaskMemFree(Some(name.0 as _));
        }
        Ok(paths)
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            webview::resize(hwnd);
            LRESULT(0)
        }
        WM_MOVE => {
            webview::parent_moved();
            LRESULT(0)
        }
        WM_SETFOCUS => {
            webview::focus();
            LRESULT(0)
        }
        WM_COPYDATA => {
            let copy = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
            match instance::received_paths(copy) {
                Some(paths) => {
                    bring_to_front(hwnd);
                    crate::open_paths(&paths);
                    LRESULT(1)
                }
                None => LRESULT(0),
            }
        }
        watcher::WM_APP_FILES_CHANGED => {
            for event in watcher::take_events() {
                let (kind, path) = match event {
                    watcher::Event::Changed(path) => ("fileChanged", path),
                    watcher::Event::Deleted(path) => ("fileDeleted", path),
                };
                webview::send(serde_json::json!({ "type": kind, "path": path }));
            }
            LRESULT(0)
        }
        WM_APP_OPEN_DIALOG => {
            if let Ok(paths) = show_open_dialog(hwnd, MARKDOWN_FILTERS, true) {
                crate::open_paths(&paths);
            }
            LRESULT(0)
        }
        WM_APP_CHOOSE_EDITOR => {
            if let Some(program) = show_open_dialog(hwnd, PROGRAM_FILTERS, false).ok().and_then(|paths| paths.into_iter().next()) {
                editor::add(&program);
                webview::send(editor::info());
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let rect = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
