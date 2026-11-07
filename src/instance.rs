//! Jedna instancja: kolejne uruchomienia przekazują ścieżki do działającego okna przez WM_COPYDATA.

use std::path::PathBuf;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::{CreateMutexW, Sleep};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, SendMessageW, WM_COPYDATA,
};
use windows::core::w;

use crate::window::CLASS_NAME;

/// Znacznik w `COPYDATASTRUCT::dwData`, żeby nie brać obcych WM_COPYDATA za listę plików.
pub const COPYDATA_OPEN: usize = 0x4D44_4F50;

/// Zwraca `true`, gdy to pierwsza instancja. W przeciwnym razie przekazuje ścieżki
/// do działającego okna i zwraca `false`.
pub fn claim(paths: &[PathBuf]) -> bool {
    unsafe {
        // Uchwyt mutexu zostaje otwarty do końca procesu.
        let created = CreateMutexW(None, false, w!("Local\\mdread.single-instance"));
        if created.is_err() || GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
    }
    // Pierwsza instancja mogła jeszcze nie utworzyć okna (np. kilka plików otwartych naraz z Eksploratora).
    for _ in 0..40 {
        if let Ok(hwnd) = unsafe { FindWindowW(CLASS_NAME, None) } {
            forward(hwnd, paths);
            return false;
        }
        unsafe { Sleep(50) };
    }
    true
}

fn forward(hwnd: HWND, paths: &[PathBuf]) {
    let joined = paths.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>().join("\n");
    let data: Vec<u16> = joined.encode_utf16().collect();
    let copy = COPYDATASTRUCT {
        dwData: COPYDATA_OPEN,
        cbData: (data.len() * 2) as u32,
        lpData: data.as_ptr() as *mut _,
    };
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let _ = AllowSetForegroundWindow(pid);
        SendMessageW(hwnd, WM_COPYDATA, Some(WPARAM(0)), Some(LPARAM(&copy as *const _ as isize)));
    }
}

/// Odczytuje ścieżki z WM_COPYDATA wysłanego przez [`forward`].
pub fn received_paths(copy: &COPYDATASTRUCT) -> Option<Vec<PathBuf>> {
    if copy.dwData != COPYDATA_OPEN {
        return None;
    }
    let data = unsafe { std::slice::from_raw_parts(copy.lpData as *const u16, copy.cbData as usize / 2) };
    let joined = String::from_utf16_lossy(data);
    Some(joined.split('\n').filter(|path| !path.is_empty()).map(PathBuf::from).collect())
}
