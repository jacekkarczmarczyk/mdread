//! Obserwacja otwartych plików: wątek co chwilę sprawdza datę i rozmiar, zmiany trafiają do okna.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Komunikat dla okna: w kolejce czekają zdarzenia z [`take_events`].
pub const WM_APP_FILES_CHANGED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 3;

const INTERVAL: Duration = Duration::from_millis(400);
/// Tyle kolejnych sprawdzeń bez pliku, zanim uznamy go za usunięty (zapis przez plik tymczasowy i podmianę).
const MISSING_CHECKS: u32 = 2;

pub enum Event {
    Changed(PathBuf),
    Deleted(PathBuf),
}

#[derive(Clone, Copy, PartialEq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

struct Watched {
    stamp: Option<Stamp>,
    missing: u32,
    deleted: bool,
}

struct Watcher {
    files: Mutex<HashMap<PathBuf, Watched>>,
    events: Mutex<Option<Receiver<Event>>>,
}

static WATCHER: OnceLock<Watcher> = OnceLock::new();

/// Uruchamia wątek obserwujący; zmiany sygnalizuje oknu `hwnd` komunikatem [`WM_APP_FILES_CHANGED`].
pub fn start(hwnd: HWND) {
    let (sender, receiver) = channel();
    let watcher = WATCHER.get_or_init(|| Watcher { files: Mutex::default(), events: Mutex::new(None) });
    *watcher.events.lock().unwrap() = Some(receiver);
    let hwnd = hwnd.0 as isize;
    thread::spawn(move || loop {
        thread::sleep(INTERVAL);
        if check(watcher, &sender) {
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as _)), WM_APP_FILES_CHANGED, WPARAM(0), LPARAM(0));
            }
        }
    });
}

pub fn watch(path: &Path) {
    if let Some(watcher) = WATCHER.get() {
        let stamp = stamp(path);
        watcher.files.lock().unwrap().insert(path.to_owned(), Watched { stamp, missing: 0, deleted: false });
    }
}

pub fn unwatch(path: &Path) {
    if let Some(watcher) = WATCHER.get() {
        watcher.files.lock().unwrap().remove(path);
    }
}

/// Zdarzenia zebrane od ostatniego komunikatu (wywoływane w wątku okna).
pub fn take_events() -> Vec<Event> {
    let Some(watcher) = WATCHER.get() else { return Vec::new() };
    let events = watcher.events.lock().unwrap();
    events.as_ref().map(|receiver| receiver.try_iter().collect()).unwrap_or_default()
}

fn check(watcher: &Watcher, sender: &Sender<Event>) -> bool {
    let mut sent = false;
    let mut files = watcher.files.lock().unwrap();
    for (path, watched) in files.iter_mut() {
        let current = stamp(path);
        match current {
            None => {
                watched.missing += 1;
                if watched.missing == MISSING_CHECKS && !watched.deleted {
                    watched.deleted = true;
                    sent |= sender.send(Event::Deleted(path.clone())).is_ok();
                }
            }
            Some(_) if watched.deleted || current != watched.stamp => {
                watched.missing = 0;
                watched.deleted = false;
                watched.stamp = current;
                sent |= sender.send(Event::Changed(path.clone())).is_ok();
            }
            Some(_) => watched.missing = 0,
        }
    }
    sent
}

fn stamp(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Stamp { modified: metadata.modified().ok(), len: metadata.len() })
}
