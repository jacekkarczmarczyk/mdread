//! Zasoby exe: ikona, manifest i wersja (Windows) oraz tabela plików UI z `ui/` (poza licencjami).

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    windows_resources();
    ui_assets();
}

fn windows_resources() {
    println!("cargo:rerun-if-changed=res");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon_with_id("res/mdread.ico", "1")
        .set_manifest_file("res/mdread.manifest")
        .set("FileDescription", "mdread - przeglądarka plików Markdown")
        .set("ProductName", "mdread")
        .set("OriginalFilename", "mdread.exe");
    resource.compile().expect("zasoby Windows (rc.exe z Windows SDK)");
}

fn ui_assets() {
    println!("cargo:rerun-if-changed=ui");
    let ui = fs::canonicalize("ui").expect("katalog ui");
    let mut files = Vec::new();
    collect(&ui, &mut files);
    files.sort();

    let mut code = String::from("pub fn asset(path: &str) -> Option<(&'static [u8], &'static str)> {\n    Some(match path {\n");
    for file in files {
        let relative = file.strip_prefix(&ui).unwrap().to_string_lossy().replace('\\', "/");
        let Some(mime) = mime(&relative) else { continue };
        code.push_str(&format!("        {relative:?} => (include_bytes!({:?}), {mime:?}),\n", file.to_string_lossy()));
    }
    code.push_str("        _ => return None,\n    })\n}\n");

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("assets.rs");
    fs::write(out, code).unwrap();
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn mime(path: &str) -> Option<&'static str> {
    Some(match path.rsplit('.').next()? {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "woff2" => "font/woff2",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}
