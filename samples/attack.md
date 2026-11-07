# Próbny złośliwy plik (MDR-14)

Nic z poniższych nie może wykonać skryptu, otworzyć niczego bez Ctrl+klik ani sięgnąć po pliki z cudzego serwera.

<script>document.title = 'XSS: script'</script>

<img src="x" onerror="document.title = 'XSS: onerror'">

<iframe src="https://example.invalid/"></iframe>

<style>body { display: none }</style>

[link javascript](javascript:document.title='XSS:link')

[link data](data:text/html,<script>alert(1)</script>)

[link do programu](file:///C:/Windows/System32/calc.exe)

![obrazek UNC](\\mdread-test-host.invalid\share\x.png)

![obrazek file UNC](file://mdread-test-host.invalid/share/x.png)

![obrazek //UNC](//mdread-test-host.invalid/share/y.png)

![obrazek zdalny](https://example.invalid/pixel.png)

$\href{javascript:document.title='XSS:katex'}{wzór z linkiem}$

```mermaid
flowchart LR
    A["<img src=x onerror=document.title='XSS:mermaid'>"] --> B
    click A call document.title='XSS:click'
```
