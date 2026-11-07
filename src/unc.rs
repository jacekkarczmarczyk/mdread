//! Ścieżki sieciowe (UNC, `\\serwer\udział\…`). Samo odczytanie pliku z cudzego serwera wysyła
//! skrót hasła Windows (NTLM), więc obrazki z UNC ładujemy tylko z serwerów, z których użytkownik
//! sam otworzył dokument.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Component, Path, Prefix};

thread_local! {
    static TRUSTED_HOSTS: RefCell<HashSet<String>> = RefCell::default();
}

/// Nazwa serwera ze ścieżki UNC (małymi literami); `None` dla ścieżek lokalnych.
pub fn host(path: &Path) -> Option<String> {
    match path.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::UNC(server, _) | Prefix::VerbatimUNC(server, _) => {
                Some(server.to_string_lossy().to_lowercase())
            }
            _ => None,
        },
        _ => None,
    }
}

/// Zapamiętuje serwer dokumentu otwartego przez użytkownika.
pub fn trust_document(path: &Path) {
    if let Some(host) = host(path) {
        TRUSTED_HOSTS.with_borrow_mut(|hosts| hosts.insert(host));
    }
}

/// Czy wolno samodzielnie (bez działania użytkownika) czytać plik spod tej ścieżki.
pub fn may_read(path: &Path) -> bool {
    host(path).is_none_or(|host| TRUSTED_HOSTS.with_borrow(|hosts| hosts.contains(&host)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn local_paths_are_readable() {
        assert!(may_read(Path::new(r"C:\docs\img.png")));
    }

    #[test]
    fn unc_paths_need_a_trusted_host() {
        let image = Path::new(r"\\Attacker\share\x.png");
        assert_eq!(host(image).as_deref(), Some("attacker"));
        assert!(!may_read(image));
        trust_document(Path::new(r"\\attacker\docs\readme.md"));
        assert!(may_read(image));
    }

    #[test]
    fn forward_slash_unc_is_detected() {
        assert_eq!(host(Path::new("//evil/share/x.png")).as_deref(), Some("evil"));
    }
}
