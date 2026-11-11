//! Updates from GitHub releases. A background check (at most once a day) reads the latest release; a newer
//! version shows a bar on the page. "Install" downloads mdread.exe from that release, verifies its SHA-256
//! against the digest GitHub reports, starts the download with `--update` and closes this window; the
//! downloaded exe waits for the old process and its WebView2 browser, replaces the installed copy and starts it.
//!
//! Only HTTPS to fixed addresses (the API of this repository and its release downloads), size limits and
//! timeouts; nothing is downloaded or run without a click.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{HSTRING, PCWSTR, w};

use crate::i18n::{t, tf};
use crate::{install, settings, webview};

/// Message for the window: results from [`take`] are waiting.
pub const WM_APP_UPDATE: u32 = WM_APP + 4;

const API: &str = "https://api.github.com/repos/jacekkarczmarczyk/mdread/releases/latest";
const API_TAG: &str = "https://api.github.com/repos/jacekkarczmarczyk/mdread/releases/tags/v";
const PAGES: &str = "https://github.com/jacekkarczmarczyk/mdread/releases/";
const DOWNLOADS: &str = "https://github.com/jacekkarczmarczyk/mdread/releases/download/";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_API_RESPONSE: usize = 1024 * 1024;
const MAX_DOWNLOAD: usize = 64 * 1024 * 1024;
/// Error code for the page (ui/app.js) when the download does not match the release's SHA-256.
const CHECKSUM_MISMATCH: &str = "checksum";

/// Results of the background threads, handled on the window thread (settings are written only there).
pub enum Outcome {
    /// The latest release; `true` when the switch started the check (the page then always shows the result).
    Checked(Value, bool),
    /// A check started by the switch failed (an automatic one fails silently).
    CheckFailed(String),
    /// Release notes of this version, saved to a file to open in a tab.
    Notes(PathBuf),
    Downloaded(PathBuf),
    DownloadFailed(String),
}

static OUTCOMES: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());

fn report(hwnd: isize, outcome: Outcome) {
    OUTCOMES.lock().unwrap().push(outcome);
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as _)), WM_APP_UPDATE, WPARAM(0), LPARAM(0));
    }
}

pub fn take() -> Vec<Outcome> {
    std::mem::take(&mut *OUTCOMES.lock().unwrap())
}

/// Version of this program; a debug build can pretend to be older (`MDREAD_PRETEND_VERSION`) to test updates.
pub fn current_version() -> String {
    #[cfg(debug_assertions)]
    if let Ok(version) = std::env::var("MDREAD_PRETEND_VERSION") {
        return version;
    }
    env!("CARGO_PKG_VERSION").to_owned()
}

fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.strip_prefix('v').unwrap_or(text).split('.').map(|part| part.parse().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(new), Some(old)) if new > old)
}

/// Updates apply to the installed copy (a debug build may test them too).
fn may_update() -> bool {
    cfg!(debug_assertions) || install::is_installed()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.as_secs())
}

/// Starts the check in the background: from the saved result if it is less than a day old, otherwise from GitHub.
/// `manual` (the switch was just turned on, or "Check now" in the menu, also with the switch off): always from
/// GitHub (a release may have come out since the saved check), and the page shows the result even when there is
/// nothing new.
pub fn start(hwnd: HWND, manual: bool) {
    if (!manual && !settings::update_check()) || !may_update() {
        return;
    }
    // Set before the check below, so a check already running reports to the page too.
    if manual {
        MANUAL.store(true, Ordering::Release);
    }
    // One check at a time, so toggling the switch quickly cannot start a burst of requests.
    if CHECKING.swap(true, Ordering::AcqRel) {
        return;
    }
    let hwnd = hwnd.0 as isize;
    let cached = settings::update_latest()
        .filter(|latest| !manual && now().saturating_sub(latest["checkedAt"].as_u64().unwrap_or(0)) < CHECK_INTERVAL.as_secs());
    std::thread::spawn(move || {
        let latest = cached.map_or_else(fetch_latest, Ok);
        CHECKING.store(false, Ordering::Release);
        let manual = MANUAL.swap(false, Ordering::AcqRel);
        match latest {
            Ok(latest) => report(hwnd, Outcome::Checked(latest, manual)),
            Err(error) if manual => report(hwnd, Outcome::CheckFailed(error)),
            Err(_) => {}
        }
    });
}

static CHECKING: AtomicBool = AtomicBool::new(false);
static MANUAL: AtomicBool = AtomicBool::new(false);

/// The latest release: version, page, download address and SHA-256 of mdread.exe (addresses checked).
fn fetch_latest() -> Result<Value, String> {
    let body = get(API, "application/vnd.github+json", MAX_API_RESPONSE)?;
    let release: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    let version = release["tag_name"].as_str().filter(|tag| parse_version(tag).is_some()).ok_or("tag_name")?;
    let page = release["html_url"].as_str().filter(|url| url.starts_with(PAGES)).ok_or("html_url")?;
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|asset| asset["name"] == "mdread.exe"))
        .ok_or("mdread.exe")?;
    let url = asset["browser_download_url"].as_str().filter(|url| url.starts_with(DOWNLOADS)).ok_or("download")?;
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64)
        .ok_or("digest")?;
    Ok(json!({
        "version": version.trim_start_matches('v'),
        "page": page,
        "asset": url,
        "sha256": sha256.to_ascii_lowercase(),
        "checkedAt": now(),
    }))
}

/// The first run of a newer version (after the update bar, `--updated`, or installed by hand) opens its release
/// notes in a tab; the first run after installation only records the version. Without a network: nothing.
pub fn notes_after_update(hwnd: HWND, updated: bool) {
    let current = current_version();
    let last = settings::last_version();
    if last.as_deref() == Some(current.as_str()) {
        return;
    }
    settings::set_last_version(&current);
    // `--updated` also covers copies from before `lastVersion` was saved.
    if !(updated || last.is_some_and(|last| is_newer(&current, &last))) || !may_update() {
        return;
    }
    let hwnd = hwnd.0 as isize;
    std::thread::spawn(move || {
        if let Ok(path) = fetch_notes(&current) {
            report(hwnd, Outcome::Notes(path));
        }
    });
}

fn fetch_notes(version: &str) -> Result<PathBuf, String> {
    let body = get(&format!("{API_TAG}{version}"), "application/vnd.github+json", MAX_API_RESPONSE)?;
    let release: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    let notes = release["body"].as_str().ok_or("body")?;
    let mut text = format!("# MDRead {version}: what's new\n\n{}\n", notes.trim());
    if let Some(page) = release["html_url"].as_str().filter(|url| url.starts_with(PAGES)) {
        text.push_str(&format!("\n[Release page]({page})\n"));
    }
    let dir = std::env::temp_dir().join("mdread-update");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("MDRead {version} - what's new.md"));
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    Ok(path)
}

/// On the window thread: saves the check result and shows the bar when the release is newer and not hidden
/// (a check started by the switch shows a hidden one again, and says so when this is the latest version).
pub fn checked(latest: Value, manual: bool) -> Option<Value> {
    settings::set_update_latest(latest.clone());
    let version = latest["version"].as_str()?;
    let current = current_version();
    let dismissed = !manual && settings::update_dismissed().is_some_and(|hidden| hidden == version);
    if is_newer(version, &current) && !dismissed {
        Some(json!({ "type": "updateAvailable", "version": version, "current": current, "page": latest["page"] }))
    } else {
        manual.then(|| json!({ "type": "updateCurrent", "version": current }))
    }
}

/// "Install" on the bar: downloads and verifies the release in the background.
pub fn download(hwnd: HWND) {
    let Some(latest) = settings::update_latest() else { return };
    let hwnd = hwnd.0 as isize;
    std::thread::spawn(move || {
        let outcome = match download_verified(&latest) {
            Ok(path) => Outcome::Downloaded(path),
            Err(message) => Outcome::DownloadFailed(message),
        };
        report(hwnd, outcome);
    });
}

fn download_verified(latest: &Value) -> Result<PathBuf, String> {
    let url = latest["asset"].as_str().filter(|url| url.starts_with(DOWNLOADS)).ok_or("download")?;
    let expected = latest["sha256"].as_str().ok_or("digest")?;
    let bytes = get(url, "application/octet-stream", MAX_DOWNLOAD)?;
    if sha256_hex(&bytes)? != expected {
        // The page shows its own message with a link to the release.
        return Err(CHECKSUM_MISMATCH.to_owned());
    }
    let dir = std::env::temp_dir().join("mdread-update");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("mdread-{}.exe", latest["version"].as_str().unwrap_or("new")));
    std::fs::write(&path, &bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

/// On the window thread: starts the downloaded exe in update mode; the caller closes the window.
pub fn launch(path: &Path) -> Result<(), String> {
    std::process::Command::new(path)
        .arg("--update")
        .arg("--after")
        .arg(std::process::id().to_string())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// `mdread.exe --update` (the downloaded file, after the old process has exited): waits for the old WebView2
/// browser to release the data folder, replaces the installed copy and starts it with the saved tabs.
pub fn apply() {
    let lock = webview::data_dir().join("EBWebView").join("lockfile");
    for _ in 0..100 {
        if !is_locked(&lock) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let target = install::installed_exe();
    let copied = std::env::current_exe().map_err(|error| error.to_string()).and_then(|source| {
        let mut result = Err(String::new());
        for _ in 0..25 {
            result = std::fs::copy(&source, &target).map(|_| ()).map_err(|error| error.to_string());
            if result.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        result
    });
    if let Err(error) = copied {
        crate::show_error(&tf("update.failed", &[("message", &error)]));
    }
    // Start the installed copy either way: the new one (it then opens its release notes), or the old one if
    // replacing it failed.
    let _ = std::process::Command::new(&target).arg("--updated").spawn();
}

/// Whether a WebView2 browser still uses the data folder (it keeps its lock file open without sharing).
fn is_locked(lock: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    lock.exists() && std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(lock).is_err()
}

fn sha256_hex(bytes: &[u8]) -> Result<String, String> {
    let mut hash = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, bytes, &mut hash) }.ok().map_err(|error| error.to_string())?;
    Ok(hash.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// WinHTTP handle closed on drop.
struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn open(handle: *mut core::ffi::c_void) -> Result<Handle, String> {
    if handle.is_null() { Err(describe(windows::core::Error::from_thread())) } else { Ok(Handle(handle)) }
}

/// WinHTTP error texts live in winhttp.dll, so the system message is only the code: name the common case instead.
fn describe(error: windows::core::Error) -> String {
    let hresult = error.code().0 as u32;
    let code = hresult & 0xFFFF;
    if hresult >> 16 == 0x8007 && (12000..12200).contains(&code) {
        tf("update.noConnection", &[("code", &code.to_string())])
    } else {
        error.to_string()
    }
}

/// HTTPS GET with redirects (WinHTTP never follows HTTPS to HTTP), a status check, timeouts and a size limit.
fn get(url: &str, accept: &str, max: usize) -> Result<Vec<u8>, String> {
    // A debug build can test working without a network (`MDREAD_PRETEND_OFFLINE`): `.invalid` never resolves.
    let offline = cfg!(debug_assertions) && std::env::var_os("MDREAD_PRETEND_OFFLINE").is_some();
    let url = if offline { "https://mdread.invalid/" } else { url };
    let rest = url.strip_prefix("https://").ok_or("https")?;
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(host, path)| (host, format!("/{path}")));
    let agent = HSTRING::from(format!("MDRead/{}", env!("CARGO_PKG_VERSION")));
    let error = describe;
    unsafe {
        let session = open(WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0))?;
        WinHttpSetTimeouts(session.0, 10_000, 10_000, 30_000, 60_000).map_err(error)?;
        let connection = open(WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0))?;
        let request = open(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(error)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(error)?;
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as _),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(error)?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available).map_err(error)?;
            if available == 0 {
                return Ok(body);
            }
            if body.len() + available as usize > max {
                return Err(t("update.tooLarge"));
            }
            let start = body.len();
            body.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(request.0, body[start..].as_mut_ptr() as _, available, &mut read).map_err(error)?;
            body.truncate(start + read as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("1.0", "0.9.0"));
        assert!(!is_newer("1.0.0-beta", "0.9.0"));
    }

    #[test]
    fn hashes_sha256() {
        assert_eq!(sha256_hex(b"abc").unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
