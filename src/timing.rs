//! Time measured since process creation. Written only when the `MDREAD_TIMING` variable points to a log file.

use std::fs::OpenOptions;
use std::io::Write;

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

pub fn mark(label: &str) {
    let Some(path) = std::env::var_os("MDREAD_TIMING") else {
        return;
    };
    let ms = since_process_start_ms();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{ms:8.1} ms  {label}");
    }
}

fn since_process_start_ms() -> f64 {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let now = unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user);
        GetSystemTimePreciseAsFileTime()
    };
    (ticks(now) - ticks(creation)) as f64 / 10_000.0
}

fn ticks(time: FILETIME) -> i64 {
    ((time.dwHighDateTime as i64) << 32) | time.dwLowDateTime as i64
}
