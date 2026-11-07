use std::path::PathBuf;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, IDC_ARROW,
    IsIconic, LoadCursorW, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_RESTORE,
    SW_SHOWDEFAULT, SWP_NOACTIVATE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos,
    SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_COPYDATA,
    WM_DESTROY, WM_DPICHANGED, WM_MOVE, WM_SETFOCUS, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, PCWSTR, Result, w};

use crate::{instance, webview};

pub const CLASS_NAME: PCWSTR = w!("mdread.window");

/// Okno dialogowe otwierania plików pokazujemy z pętli komunikatów, a nie z callbacku WebView2.
const WM_APP_OPEN_DIALOG: u32 = WM_APP + 1;

pub fn create() -> Result<HWND> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("mdread"),
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
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        Ok(hwnd)
    }
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
        Some(document) => format!("{document} - mdread"),
        None => "mdread".to_owned(),
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

fn show_open_dialog(hwnd: HWND) -> Result<Vec<PathBuf>> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(dialog.GetOptions()? | FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST)?;
        dialog.SetFileTypes(&[
            COMDLG_FILTERSPEC { pszName: w!("Markdown"), pszSpec: w!("*.md;*.markdown;*.mdown;*.mkd") },
            COMDLG_FILTERSPEC { pszName: w!("Wszystkie pliki"), pszSpec: w!("*.*") },
        ])?;
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
        WM_APP_OPEN_DIALOG => {
            if let Ok(paths) = show_open_dialog(hwnd) {
                crate::open_paths(&paths);
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
