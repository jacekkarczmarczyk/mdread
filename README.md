# MDRead

Szybka przeglądarka plików Markdown dla Windows: karty, jedna instancja, motywy jasne i ciemne,
kopiowanie z formatowaniem albo jako tekst, wzory (KaTeX), diagramy (Mermaid), kolorowanie kodu,
podpowiedź o zmianie pliku na dysku. Bez edycji; przycisk „Edytuj” otwiera plik w wybranym edytorze.

Rust + Win32 + WebView2 (wbudowany w Windows 11), Markdown przez [comrak](https://github.com/kivikakk/comrak).

## Budowanie

Wymagane: Rust (rustup, toolchain `stable-x86_64-pc-windows-msvc`) i Visual Studio Build Tools
z „Desktop development with C++” (kompilator, linker i `rc.exe` z Windows SDK).

```
cargo build --release
```

Wynik: `target\release\mdread.exe` (jeden plik, interfejs i biblioteki wbudowane).

## Skojarzenie z plikami .md

```
mdread.exe --register            wpisy w HKCU i pytanie o otwarcie ustawień aplikacji domyślnych
mdread.exe --register --quiet    to samo bez okien (dla instalatora)
mdread.exe --unregister          usuwa wpisy
```

Rejestracja dotyczy tylko bieżącego użytkownika i nie wymaga uprawnień administratora. Windows nie pozwala
programowi ustawić się jako domyślny: MDRead pojawia się w „Otwórz za pomocą” i w Ustawieniach →
Aplikacje domyślne, gdzie wybiera się go ręcznie.

Ustawienia (motyw, powiększenie, edytor): `%APPDATA%\mdread\settings.json`.
Dane WebView2: `%LOCALAPPDATA%\mdread`.

## Pomiary (release, Windows 11, 2026-11-07)

| | |
|---|---|
| rozmiar `mdread.exe` | 7,4 MB (w tym Mermaid ~5,5 MB) |
| okno widoczne | ~35 ms od startu procesu |
| treść na ekranie | ~390–400 ms od startu procesu (najwięcej zajmuje start WebView2) |

Pomiar: zmienna `MDREAD_TIMING=<plik logu>` zapisuje czasy kolejnych etapów startu.
