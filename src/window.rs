use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, MONITOR_DEFAULTTONULL, MonitorFromRect};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWL_STYLE, GetClientRect, GetMessageW,
    GetSystemMetrics, GetWindowPlacement, GetWindowRect, IDC_ARROW, IsIconic, LoadCursorW, LoadIconW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_RESTORE, SW_SHOWDEFAULT,
    SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, SW_SHOWNORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, SetWindowTextW, ShowWindow,
    TranslateMessage, WINDOW_EX_STYLE, WINDOWPLACEMENT, WM_APP, WM_CLOSE, WM_COPYDATA, WM_DESTROY, WM_DPICHANGED,
    WM_MOVE, WM_SETFOCUS, WM_SIZE, WNDCLASSW, WPF_RESTORETOMAXIMIZED, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED,
    WS_OVERLAPPEDWINDOW, WS_SYSMENU, WS_VISIBLE,
};
use windows::core::{BOOL, HSTRING, PCWSTR, Result, w};

use crate::i18n::t;
use crate::{editor, instance, settings, update, watcher, webview};

pub const CLASS_NAME: PCWSTR = w!("mdread.window");

/// Dialogs are shown from the message loop, not from a WebView2 callback.
const WM_APP_OPEN_DIALOG: u32 = WM_APP + 1;
const WM_APP_CHOOSE_EDITOR: u32 = WM_APP + 2;

/// Dialog filters: (name key in the string table, pattern).
const MARKDOWN_FILTERS: &[(&str, &str)] =
    &[("dialog.markdown", "*.md;*.markdown;*.mdown;*.mkd"), ("dialog.allFiles", "*.*")];
const PROGRAM_FILTERS: &[(&str, &str)] = &[("dialog.programs", "*.exe")];

/// Installer window size (pixels at 100% scaling): just the panel, no tabs.
const INSTALLER_SIZE: (i32, i32) = (600, 700);

pub fn create(installer: bool) -> Result<HWND> {
    let appearance = settings::appearance();
    // Installer: a small dialog-like window (no maximizing or resizing), centered on the screen.
    let (style, x, y, width, height) = if installer {
        let scale = unsafe { GetDpiForSystem() } as i32;
        let (width, height) = (INSTALLER_SIZE.0 * scale / 96, INSTALLER_SIZE.1 * scale / 96);
        let (screen_width, screen_height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        (style, (screen_width - width) / 2, (screen_height - height) / 2, width, height)
    } else {
        (WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT)
    };
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        // Background in the last theme's color so a white window does not flash before the page loads.
        let background = appearance.background.map(|color| CreateSolidBrush(COLORREF(color))).unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // Icon from the exe resources (id 1, see build.rs).
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
            style,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance),
            None,
        )?;
        apply_frame(hwnd, &appearance);
        // The main window comes back where it was closed (also maximized); the installer always opens centered.
        let restored = !installer && restore_placement(hwnd);
        if !restored {
            let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        }
        SAVE_PLACEMENT.store(!installer, Ordering::Relaxed);
        Ok(hwnd)
    }
}

/// Whether the window's placement is saved when it closes (main window, not the installer).
static SAVE_PLACEMENT: AtomicBool = AtomicBool::new(false);

/// Shows the window with the saved placement; `false` when there is none or it is off every screen (e.g. after
/// unplugging a monitor), so the caller shows it at the default position.
fn restore_placement(hwnd: HWND) -> bool {
    let Some(saved) = settings::window_placement() else { return false };
    let rect = RECT { left: saved.left, top: saved.top, right: saved.right, bottom: saved.bottom };
    if rect.right <= rect.left || rect.bottom <= rect.top || unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) }.is_invalid() {
        return false;
    }
    let placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        showCmd: if saved.maximized { SW_SHOWMAXIMIZED.0 as u32 } else { SW_SHOWNORMAL.0 as u32 },
        rcNormalPosition: rect,
        ..Default::default()
    };
    unsafe { SetWindowPlacement(hwnd, &placement) }.is_ok()
}

/// Saves the normal rectangle and the maximized state (also when the window is closed while minimized).
fn save_placement(hwnd: HWND) {
    let mut placement = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
    if unsafe { GetWindowPlacement(hwnd, &mut placement) }.is_err() {
        return;
    }
    let maximized = placement.showCmd == SW_SHOWMAXIMIZED.0 as u32
        || (placement.showCmd == SW_SHOWMINIMIZED.0 as u32 && placement.flags.contains(WPF_RESTORETOMAXIMIZED));
    let rect = placement.rcNormalPosition;
    settings::set_window_placement(&settings::WindowPlacement {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        maximized,
    });
}

/// Fits the installer window height to its content (`client_height` in screen pixels) and centers it.
pub fn fit_client_height(hwnd: HWND, client_height: i32) {
    unsafe {
        let mut window = RECT::default();
        let mut client = RECT::default();
        let _ = GetWindowRect(hwnd, &mut window);
        let _ = GetClientRect(hwnd, &mut client);
        let frame = (window.bottom - window.top) - (client.bottom - client.top);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        let height = (client_height + frame).min(screen_height);
        let _ = SetWindowPos(
            hwnd,
            None,
            window.left,
            (screen_height - height) / 2,
            window.right - window.left,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// "Run without installing": the installer window becomes a regular window (resizing, maximizing).
pub fn make_normal(hwnd: HWND) {
    SAVE_PLACEMENT.store(true, Ordering::Relaxed);
    unsafe {
        let scale = GetDpiForSystem() as i32;
        let (width, height) = (1200 * scale / 96, 800 * scale / 96);
        let (screen_width, screen_height) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_OVERLAPPEDWINDOW | WS_VISIBLE).0 as isize);
        let _ = SetWindowPos(
            hwnd,
            None,
            (screen_width - width) / 2,
            (screen_height - height) / 2,
            width,
            height,
            SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

/// Title bar in the theme colors: dark/light frame mode and (Windows 11) background and text color.
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

/// Whether Windows uses the dark app theme.
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
    // The filter texts must live until the SetFileTypes call returns.
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
            return Ok(Vec::new()); // cancelled
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
        update::WM_APP_UPDATE => {
            for outcome in update::take() {
                match outcome {
                    update::Outcome::Checked(latest) => {
                        if let Some(message) = update::checked(latest) {
                            webview::send(message);
                        }
                    }
                    // The downloaded version takes over: it waits for this process to exit, then replaces it.
                    update::Outcome::Downloaded(path) => match update::launch(&path) {
                        Ok(()) => request_close(hwnd),
                        Err(error) => webview::send(serde_json::json!({ "type": "updateError", "message": error })),
                    },
                    update::Outcome::DownloadFailed(error) => {
                        webview::send(serde_json::json!({ "type": "updateError", "message": error }));
                    }
                }
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
                // The added program becomes the default and opens the current file at once (like a list pick).
                editor::add(&program);
                webview::send(editor::info());
                if let Some(document) = webview::active_path() {
                    editor::open(&document);
                }
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
            if SAVE_PLACEMENT.load(Ordering::Relaxed) {
                save_placement(hwnd);
            }
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
