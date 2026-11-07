---
title: Przykład podstawowy
tags: [test, gfm]
---

# Nagłówek 1

Akapit z **pogrubieniem**, *kursywą*, ~~przekreśleniem~~ i `kodem w linii`.
Autolink: https://github.com i [link z tytułem](https://example.com "Przykład").

## Listy

- punkt
  - zagnieżdżony
- [x] zadanie zrobione
- [ ] zadanie do zrobienia

1. pierwszy
2. drugi

## Tabela

| Kolumna | Wyrównana do prawej | Środek |
|---------|--------------------:|:------:|
| a       |                  10 |   x    |
| b       |                 200 |   y    |

## Kod

```rust
fn main() {
    println!("Cześć, {}!", "świecie");
}
```

```js
const x = [1, 2, 3].map(n => n * 2);
```

```
blok bez języka
```

```nieznany-jezyk
blok z nieznanym językiem
```

> Cytat z **formatowaniem**.

> [!NOTE]
> Alert w stylu GitHub.

> [!WARNING]
> Ostrzeżenie.

Tekst z przypisem[^1].

[^1]: Treść przypisu.

---

Linki lokalne: [inny plik](links.md), [kotwica](#tabela), [obrazek](img/diagram.svg).
