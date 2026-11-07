use std::path::{Path, PathBuf};

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, format_html, parse_document};

/// Pod tym adresem strona dostaje lokalne pliki (obrazki), a linki do plików niosą ścieżkę.
pub const LOCAL_PREFIX: &str = "https://mdread.localhost/local/";

/// Renderuje Markdown; względne adresy linków i obrazków rozwiązuje względem `base_dir`.
pub fn render(source: &str, base_dir: &Path) -> String {
    let options = options();
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options);
    for node in root.descendants() {
        if let NodeValue::Link(link) | NodeValue::Image(link) = &mut node.data.borrow_mut().value
            && let Some(url) = local_url(&link.url, base_dir)
        {
            link.url = url;
        }
    }
    let mut html = String::new();
    let _ = format_html(root, &options, &mut html);
    html
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    let ext = &mut options.extension;
    ext.strikethrough = true;
    ext.table = true;
    ext.autolink = true;
    ext.tasklist = true;
    ext.footnotes = true;
    ext.front_matter_delimiter = Some("---".into());
    ext.alerts = true;
    ext.math_dollars = true;
    ext.math_code = true;
    ext.header_id_prefix = Some(String::new());
    options.render.github_pre_lang = true;
    options.render.sourcepos = true;
    options
}

/// Adres pod [`LOCAL_PREFIX`] dla ścieżki w pliku; `None` dla kotwic i adresów z innym schematem.
fn local_url(url: &str, base_dir: &Path) -> Option<String> {
    if url.is_empty() || url.starts_with('#') {
        return None;
    }
    let (target, fragment) = match url.split_once('#') {
        Some((target, fragment)) => (target, Some(fragment)),
        None => (url, None),
    };
    let path = match scheme(target) {
        Some("file") => file_url_path(&target["file:".len()..]),
        Some(_) => return None,
        None => base_dir.join(percent_decode(target)),
    };
    let mut local = format!("{LOCAL_PREFIX}{}", percent_encode(&path.to_string_lossy().replace('\\', "/")));
    if let Some(fragment) = fragment {
        local.push('#');
        local.push_str(fragment);
    }
    Some(local)
}

/// Ścieżka z adresu `file:` (bez schematu): `///C:/x` i `C:/x` to dysk lokalny, `//serwer/udział/x` to UNC.
fn file_url_path(rest: &str) -> PathBuf {
    let rest = percent_decode(rest);
    let path = match rest.strip_prefix("//") {
        Some(rest) => match rest.strip_prefix('/') {
            Some(local) => local.to_owned(),
            None if rest.get(1..2) == Some(":") => rest.to_owned(),
            None => format!("//{rest}"),
        },
        None => rest,
    };
    PathBuf::from(path.replace('/', "\\"))
}

/// Schemat adresu (`http`, `mailto`…). Jedna litera to dysk Windows (`C:`), nie schemat.
fn scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    let valid = scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    valid.then_some(scheme)
}

pub fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|hex| std::str::from_utf8(hex).ok());
        match (bytes[i], hex.and_then(|hex| u8::from_str_radix(hex, 16).ok())) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                i += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn html(source: &str) -> String {
        render(source, Path::new(r"C:\docs"))
    }

    #[test]
    fn renders_gfm() {
        let html = html("| a |\n|---|\n| b |\n\n- [x] done\n\n~~gone~~");
        assert!(html.contains("<table"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("<del"));
    }

    #[test]
    fn strips_front_matter() {
        let html = html("---\ntitle: x\n---\n\n# Hi");
        assert!(!html.contains("title: x"));
        assert!(html.contains("<h1"));
    }

    #[test]
    fn escapes_raw_html() {
        assert!(!html("<script>alert(1)</script>").contains("<script>"));
    }

    #[test]
    fn rewrites_relative_links_and_images() {
        let html = html("[a](other%20file.md#part) ![i](img/x.png)");
        assert!(html.contains(r#"href="https://mdread.localhost/local/C%3A/docs/other%20file.md#part""#), "{html}");
        assert!(html.contains(r#"src="https://mdread.localhost/local/C%3A/docs/img/x.png""#), "{html}");
    }

    #[test]
    fn keeps_anchors_and_web_links() {
        let html = html("[a](#tabela) [b](https://example.com/x) [c](mailto:a@b.pl)");
        assert!(html.contains(r##"href="#tabela""##));
        assert!(html.contains(r#"href="https://example.com/x""#));
        assert!(html.contains(r#"href="mailto:a@b.pl""#));
    }

    #[test]
    fn drops_javascript_links() {
        assert!(!html("[x](javascript:alert(1))").contains("javascript:"));
    }

    #[test]
    fn parses_file_urls() {
        assert_eq!(file_url_path("///C:/docs/a%20b.png"), PathBuf::from(r"C:\docs\a b.png"));
        assert_eq!(file_url_path("//C:/docs/a.png"), PathBuf::from(r"C:\docs\a.png"));
        assert_eq!(file_url_path("//server/share/a.png"), PathBuf::from(r"\\server\share\a.png"));
        assert_eq!(file_url_path("C:/docs/a.png"), PathBuf::from(r"C:\docs\a.png"));
    }

    #[test]
    fn decodes_what_it_encodes() {
        let path = "C:/Users/Jacek/Mój plik #1.md";
        assert_eq!(percent_decode(&percent_encode(path)), path);
    }
}
