//! Watching open files: a thread polls date and size at short intervals, changes go to the window.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Message for the window: events from [`take_events`] are waiting in the queue.
pub const WM_APP_FILES_CHANGED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 3;

const INTERVAL: Duration = Duration::from_millis(400);
/// Consecutive checks without the file before it counts as deleted (saving via a temp file and a swap).
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

/// Starts the watcher thread; it signals changes to window `hwnd` with the [`WM_APP_FILES_CHANGED`] message.
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

/// Events collected since the last message (called on the window thread).
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
