# Diagramy Mermaid

Każdy diagram poniżej to zwykły blok kodu z językiem `mermaid`. Bez Mermaida byłby pokazany jako tekst:

```text
sequenceDiagram
    Przeglądarka->>Serwer: GET /zamowienia
```

## Schemat blokowy

```mermaid
flowchart LR
    A[Klik w plik .md] --> B{Okno już otwarte?}
    B -- tak --> C[Nowa karta]
    B -- nie --> D[Nowe okno]
```

## Diagram sekwencji

```mermaid
sequenceDiagram
    participant P as Przeglądarka
    participant S as Serwer
    participant B as Baza
    P->>S: GET /zamowienia
    S->>B: SELECT ...
    B-->>S: 12 wierszy
    S-->>P: 200 OK (JSON)
```

## Harmonogram

```mermaid
gantt
    dateFormat YYYY-MM-DD
    section Wydanie 1.0
    Projekt      :done, a1, 2026-11-02, 3d
    Implementacja:active, a2, after a1, 5d
    Testy        :a3, after a2, 3d
```

## Wykres kołowy

```mermaid
pie title Rozmiar mdread.exe
    "Mermaid" : 5.3
    "Kod programu" : 2.0
    "KaTeX" : 0.5
    "Reszta" : 0.2
```

## Stany

```mermaid
stateDiagram-v2
    [*] --> Otwarty
    Otwarty --> Zmieniony: plik zmieniony na dysku
    Zmieniony --> Otwarty: Odśwież
    Otwarty --> Usunięty: plik usunięty
    Usunięty --> [*]: Zamknij kartę
```
