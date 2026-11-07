use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, IDC_ARROW,
    LoadCursorW, MSG, PostQuitMessage, RegisterClassW, SW_SHOWDEFAULT, SWP_NOACTIVATE,
    SWP_NOZORDER, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_DESTROY,
    WM_DPICHANGED, WM_MOVE, WM_SETFOCUS, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, Result, w};

use crate::webview;

const CLASS_NAME: windows::core::PCWSTR = w!("mdread.window");

pub fn create(title: &str) -> Result<HWND> {
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
            &HSTRING::from(title),
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
