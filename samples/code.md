# Bloki kodu

Krótkie wiersze: przycisk **Zawijaj** powinien być nieaktywny.

```js
const answer = 42;
console.log(answer);
```

Długi wiersz: **Zawijaj** aktywny, blok przewija się w poziomie, po kliknięciu wiersz się łamie.

```text
2026-11-07 22:14:09 INFO mdread: window shown in 35 ms, WebView2 ready in 310 ms, document rendered in 392 ms, outline built with 20 headings, session saved
```

Wiersz tuż na granicy: zwęź okno albo otwórz spis treści, a **Zawijaj** powinien się uaktywnić.

```sql
SELECT id, name, created_at FROM documents WHERE folder = 'samples' ORDER BY created_at DESC LIMIT 20;
```

Bez języka i z bardzo długim słowem bez spacji:

```
https://example.com/a/very/long/address/without/any/spaces/that/cannot/be/broken/at/a/space/character/so/it/needs/overflow-wrap/anywhere/to/fit
```

Rust z kilkoma długimi wierszami:

```rust
fn main() {
    let files = std::env::args().skip(1).map(std::path::PathBuf::from).filter(|path| path.extension().is_some_and(|ext| ext == "md")).collect::<Vec<_>>();
    for file in &files {
        println!("{}", file.display());
    }
}
```
