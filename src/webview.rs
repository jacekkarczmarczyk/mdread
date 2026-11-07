//! WebView2: środowisko, kontroler, wbudowane zasoby UI i wymiana wiadomości ze stroną.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use serde_json::Value;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Environment, ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs,
    ICoreWebView2WebMessageReceivedEventArgs2, ICoreWebView2WebResourceRequestedEventArgs,
    ICoreWebView2WebResourceResponse,
};
use webview2_com::{
    CoTaskMemPWSTR, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, WebMessageReceivedEventHandler,
    WebResourceRequestedEventHandler,
};
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Com::IStream;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{Error, HSTRING, Interface, PWSTR, Result};

use crate::{editor, settings, timing, window};

// `.localhost` rozwiązuje się bez DNS; przy `.local` Windows pytał mDNS i nawigacja trwała ~2 s.
const ORIGIN: &str = "https://mdread.localhost/";

const CSP: &str = "default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'";

#[derive(Default)]
struct State {
    hwnd: HWND,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    page_ready: bool,
    pending: Vec<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Uruchamia tworzenie środowiska WebView2 dla okna `hwnd`; dalsze kroki idą przez callbacki.
pub fn start(hwnd: HWND) -> Result<()> {
    STATE.with_borrow_mut(|state| state.hwnd = hwnd);
    let data_dir = std::env::var("LOCALAPPDATA").unwrap_or_default() + "\\mdread";
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(|result, env| {
        result.and_then(|()| env.ok_or_else(|| Error::from(E_POINTER))).and_then(on_environment).or_else(fatal)
    }));
    unsafe {
        CreateCoreWebView2EnvironmentWithOptions(None, &HSTRING::from(data_dir), None, &handler)
    }
}

/// Wysyła wiadomość JSON do strony albo kolejkuje ją do czasu, aż strona się załaduje.
pub fn send(message: Value) {
    let json = message.to_string();
    let webview = STATE.with_borrow_mut(|state| {
        if state.page_ready {
            state.webview.clone()
        } else {
            state.pending.push(json.clone());
            None
        }
    });
    if let Some(webview) = webview {
        unsafe {
            let _ = webview.PostWebMessageAsJson(&HSTRING::from(json));
        }
    }
}

pub fn resize(hwnd: HWND) {
    if let Some(controller) = controller() {
        unsafe {
            let _ = controller.SetBounds(client_rect(hwnd));
        }
    }
}

pub fn parent_moved() {
    if let Some(controller) = controller() {
        unsafe {
            let _ = controller.NotifyParentWindowPositionChanged();
        }
    }
}

pub fn focus() {
    if let Some(controller) = controller() {
        unsafe {
            let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
        }
    }
}

fn controller() -> Option<ICoreWebView2Controller> {
    STATE.with_borrow(|state| state.controller.clone())
}

fn client_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    rect
}

fn on_environment(env: ICoreWebView2Environment) -> Result<()> {
    timing::mark("webview environment");
    let hwnd = STATE.with_borrow(|state| state.hwnd);
    let env_for_controller = env.clone();
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |result, controller| {
        result
            .and_then(|()| controller.ok_or_else(|| Error::from(E_POINTER)))
            .and_then(|controller| on_controller(env_for_controller, controller))
            .or_else(fatal)
    }));
    unsafe { env.CreateCoreWebView2Controller(hwnd, &handler) }
}

fn on_controller(env: ICoreWebView2Environment, controller: ICoreWebView2Controller) -> Result<()> {
    timing::mark("webview controller");
    let hwnd = STATE.with_borrow(|state| state.hwnd);
    unsafe {
        controller.SetBounds(client_rect(hwnd))?;
        let webview = controller.CoreWebView2()?;

        let settings = webview.Settings()?;
        settings.SetAreDevToolsEnabled(cfg!(debug_assertions))?;
        settings.SetIsStatusBarEnabled(false)?;

        webview.AddWebResourceRequestedFilter(
            &HSTRING::from(format!("{ORIGIN}*")),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        )?;
        let mut token = 0;
        webview.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| match args {
                Some(args) => serve_resource(&env, &args),
                None => Ok(()),
            })),
            &mut token,
        )?;
        webview.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    let mut json = PWSTR::null();
                    args.WebMessageAsJson(&mut json)?;
                    let json = CoTaskMemPWSTR::from(json).to_string();
                    if let Ok(message) = serde_json::from_str::<Value>(&json) {
                        on_page_message(&message, &args);
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;

        webview.Navigate(&HSTRING::from(format!("{ORIGIN}index.html")))?;

        STATE.with_borrow_mut(|state| {
            state.controller = Some(controller);
            state.webview = Some(webview);
        });
    }
    Ok(())
}

fn on_page_message(message: &Value, args: &ICoreWebView2WebMessageReceivedEventArgs) {
    let hwnd = STATE.with_borrow(|state| state.hwnd);
    match message["type"].as_str() {
        Some("title") => window::set_title(hwnd, message["title"].as_str()),
        Some("openDialog") => window::request_open_dialog(hwnd),
        Some("edit") => {
            if let Some(path) = message["path"].as_str() {
                editor::open(Path::new(path));
            }
        }
        Some("chooseEditor") => window::request_choose_editor(hwnd),
        Some("resetEditor") => {
            settings::set_editor(None);
            send(editor::info());
        }
        Some("drop") => crate::open_paths(&dropped_paths(args)),
        Some("close") => window::request_close(hwnd),
        Some("ready") => {
            timing::mark("page ready");
            let (webview, pending) = STATE.with_borrow_mut(|state| {
                state.page_ready = true;
                (state.webview.clone(), std::mem::take(&mut state.pending))
            });
            if let Some(webview) = webview {
                for json in pending {
                    unsafe {
                        let _ = webview.PostWebMessageAsJson(&HSTRING::from(json));
                    }
                }
            }
        }
        Some("rendered") => timing::mark("content rendered"),
        _ => {}
    }
}

/// Ścieżki plików upuszczonych na stronę (przekazane przez `postMessageWithAdditionalObjects`).
fn dropped_paths(args: &ICoreWebView2WebMessageReceivedEventArgs) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Ok(args) = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>() else {
        return paths;
    };
    unsafe {
        let Ok(objects) = args.AdditionalObjects() else {
            return paths;
        };
        let mut count = 0;
        let _ = objects.Count(&mut count);
        for index in 0..count {
            let Ok(file) = objects.GetValueAtIndex(index).and_then(|object| object.cast::<ICoreWebView2File>())
            else {
                continue;
            };
            let mut path = PWSTR::null();
            if file.Path(&mut path).is_ok() {
                paths.push(PathBuf::from(CoTaskMemPWSTR::from(path).to_string()));
            }
        }
    }
    paths
}

fn serve_resource(env: &ICoreWebView2Environment, args: &ICoreWebView2WebResourceRequestedEventArgs) -> Result<()> {
    let uri = unsafe {
        let mut uri = PWSTR::null();
        args.Request()?.Uri(&mut uri)?;
        CoTaskMemPWSTR::from(uri).to_string()
    };
    let path = uri.strip_prefix(ORIGIN).unwrap_or_default();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let response = match asset(path) {
        Some((body, mime)) => respond(env, Some(body), 200, "OK", mime)?,
        None => respond(env, None, 404, "Not Found", "text/plain")?,
    };
    unsafe { args.SetResponse(&response) }
}

fn respond(
    env: &ICoreWebView2Environment,
    body: Option<&[u8]>,
    status: i32,
    reason: &str,
    mime: &str,
) -> Result<ICoreWebView2WebResourceResponse> {
    let headers = format!("Content-Type: {mime}\r\nContent-Security-Policy: {CSP}");
    let stream: Option<IStream> = body.and_then(|body| unsafe { SHCreateMemStream(Some(body)) });
    unsafe {
        env.CreateWebResourceResponse(stream.as_ref(), status, &HSTRING::from(reason), &HSTRING::from(headers))
    }
}

fn asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match path {
        "index.html" => (include_bytes!("../ui/index.html"), "text/html; charset=utf-8"),
        "app.css" => (include_bytes!("../ui/app.css"), "text/css; charset=utf-8"),
        "app.js" => (include_bytes!("../ui/app.js"), "text/javascript; charset=utf-8"),
        _ => return None,
    })
}

fn fatal(error: Error) -> Result<()> {
    crate::fatal(&format!("Nie udało się uruchomić WebView2.\n\n{error}"));
}
