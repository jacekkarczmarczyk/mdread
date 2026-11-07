//! Menu kontekstowe: zostają tylko pozycje do kopiowania i zaznaczania, dochodzą własne.

use std::path::Path;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR, ICoreWebView2, ICoreWebView2_11,
    ICoreWebView2ContextMenuItem, ICoreWebView2ContextMenuRequestedEventArgs, ICoreWebView2Environment,
    ICoreWebView2Environment9,
};
use webview2_com::{ContextMenuRequestedEventHandler, CoTaskMemPWSTR, CustomItemSelectedEventHandler};
use windows::core::{BOOL, HSTRING, Interface, PWSTR, Result};

use crate::{clipboard, links};

/// Wbudowane pozycje WebView2, które zostają (reszta: Wstecz, Odśwież, Zapisz, Drukuj… znika).
const KEPT: &[&str] = &["copy", "copyImage", "inspectElement"];

pub fn attach(webview: &ICoreWebView2, env: &ICoreWebView2Environment) -> Result<()> {
    let webview = webview.cast::<ICoreWebView2_11>()?;
    let env = env.cast::<ICoreWebView2Environment9>()?;
    let mut token = 0;
    unsafe {
        webview.add_ContextMenuRequested(
            &ContextMenuRequestedEventHandler::create(Box::new(move |_, args| match args {
                Some(args) => build(&env, &args),
                None => Ok(()),
            })),
            &mut token,
        )
    }
}

fn build(env: &ICoreWebView2Environment9, args: &ICoreWebView2ContextMenuRequestedEventArgs) -> Result<()> {
    unsafe {
        let items = args.MenuItems()?;
        let target = args.ContextMenuTarget()?;

        let mut count = 0;
        items.Count(&mut count)?;
        let mut builtin = Vec::new();
        for index in 0..count {
            let item = items.GetValueAtIndex(index)?;
            builtin.push((name(&item), item));
        }
        for index in (0..count).rev() {
            items.RemoveValueAtIndex(index)?;
        }
        let take = |wanted: &str| {
            debug_assert!(KEPT.contains(&wanted));
            builtin.iter().find(|(name, _)| name == wanted).map(|(_, item)| item.clone())
        };

        let mut menu: Vec<ICoreWebView2ContextMenuItem> = Vec::new();
        menu.extend(take("copy"));
        let mut has_selection = BOOL(0);
        target.HasSelection(&mut has_selection)?;
        if has_selection.as_bool() {
            let mut text = PWSTR::null();
            target.SelectionText(&mut text)?;
            let text = CoTaskMemPWSTR::from(text).to_string();
            menu.push(command(env, "Kopiuj jako tekst", move || clipboard::set_text(&text))?);
        }
        let mut has_link = BOOL(0);
        target.HasLinkUri(&mut has_link)?;
        if has_link.as_bool() {
            let mut uri = PWSTR::null();
            target.LinkUri(&mut uri)?;
            let address = links::display(&CoTaskMemPWSTR::from(uri).to_string());
            menu.push(command(env, "Kopiuj adres linku", move || clipboard::set_text(&address))?);
        }
        menu.extend(take("copyImage"));
        if !menu.is_empty() {
            menu.push(separator(env)?);
        }
        if let Some(path) = crate::webview::active_path() {
            menu.push(command(env, "Kopiuj źródło Markdown", move || copy_source(&path))?);
        }
        // Wbudowane „Zaznacz wszystko” WebView2 pokazuje tylko w polach edycji.
        menu.push(command(env, "Zaznacz wszystko", || crate::webview::execute("selectAllInActive()"))?);
        if cfg!(debug_assertions) {
            menu.extend(take("inspectElement"));
        }

        for (index, item) in menu.iter().enumerate() {
            items.InsertValueAtIndex(index as u32, item)?;
        }
    }
    Ok(())
}

fn copy_source(path: &Path) {
    match crate::read_source(path) {
        Ok(source) => clipboard::set_text(&source),
        Err(error) => crate::show_error(&format!("{}: {error}", path.display())),
    }
}

fn name(item: &ICoreWebView2ContextMenuItem) -> String {
    let mut name = PWSTR::null();
    unsafe {
        let _ = item.Name(&mut name);
    }
    CoTaskMemPWSTR::from(name).to_string()
}

fn command(
    env: &ICoreWebView2Environment9,
    label: &str,
    action: impl Fn() + 'static,
) -> Result<ICoreWebView2ContextMenuItem> {
    let item = create(env, label, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND)?;
    let mut token = 0;
    unsafe {
        item.add_CustomItemSelected(
            &CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                action();
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(item)
}

fn separator(env: &ICoreWebView2Environment9) -> Result<ICoreWebView2ContextMenuItem> {
    create(env, "", COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR)
}

fn create(
    env: &ICoreWebView2Environment9,
    label: &str,
    kind: COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND,
) -> Result<ICoreWebView2ContextMenuItem> {
    unsafe { env.CreateContextMenuItem(&HSTRING::from(label), None, kind) }
}
