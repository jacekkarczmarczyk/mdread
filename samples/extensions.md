# Rozszerzenia

## Matematyka

W linii: $E = mc^2$ oraz $`\sqrt{a^2 + b^2}`$.

Blok:

$$
\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

Fałszywy alarm (nie powinien być wzorem): kosztuje 5$ a tamto 10$.

## Mermaid

```mermaid
flowchart LR
    A[Klik w plik .md] --> B{Okno już otwarte?}
    B -- tak --> C[Nowa karta]
    B -- nie --> D[Nowe okno]
```

## Surowy HTML (sanityzacja)

<details>
<summary>Rozwiń</summary>

Treść w `<details>` powinna działać.

</details>

<script>alert('to nie powinno się wykonać')</script>

<img src="x" onerror="alert('ani to')">

[link javascript](javascript:alert(1))
