'use strict';

const host = window.chrome.webview;
const LOCAL_ORIGIN = 'https://mdread.localhost/';

// Teksty interfejsu ze strings.js (generuje aplikacja w wybranym języku; zmiana języka ją restartuje).
const I18N = window.I18N ?? { strings: {}, language: 'en', setting: 'system' };

function t(key, params = {}) {
  let text = I18N.strings[key] ?? key;
  for (const [name, value] of Object.entries(params)) text = text.replaceAll(`{${name}}`, value);
  return text;
}

document.documentElement.lang = I18N.language;
for (const element of document.querySelectorAll('[data-i18n]')) element.textContent = t(element.dataset.i18n);
for (const element of document.querySelectorAll('[data-i18n-html]')) element.innerHTML = t(element.dataset.i18nHtml);
for (const element of document.querySelectorAll('[data-i18n-title]')) element.title = t(element.dataset.i18nTitle);
const tabsEl = document.getElementById('tabs');
const docs = document.getElementById('docs');
const empty = document.getElementById('empty');
const fileBars = document.getElementById('file-bars');
const editTools = document.getElementById('edit-tools');
const tabListTools = document.getElementById('tab-list-tools');
const outlineToggle = document.getElementById('outline-toggle');
const editButton = document.getElementById('edit');
const editMenuButton = document.getElementById('edit-menu-button');
const editMenu = document.getElementById('edit-menu');
const editorList = document.getElementById('editor-list');

/** @type {{ path: string, key: string, title: string, tab: HTMLElement, page: HTMLElement }[]} */
const tabs = [];
let active = null;
let firstRender = true;

function post(message) {
  host.postMessage(message);
}

function openDocument({ path, title, html, error, anchor, notice, position }) {
  const key = path.toLowerCase();
  const existing = findTab(path);
  if (existing) {
    activate(existing);
    if (anchor) scrollToAnchor(existing, anchor);
    return;
  }

  const page = document.createElement('div');
  page.className = 'page';
  page.tabIndex = -1;
  page.hidden = true;
  const content = documentContent(path, html, error, false, notice);
  page.append(content);
  const hasMath = page.querySelector('[data-math-style]') !== null;
  const hasDiagrams = page.querySelector('pre[lang="mermaid"]') !== null;

  const tab = document.createElement('div');
  tab.className = 'tab';
  tab.setAttribute('role', 'tab');
  tab.title = path;
  const label = document.createElement('span');
  label.className = 'tab-title';
  label.textContent = title;
  const closeButton = document.createElement('button');
  closeButton.type = 'button';
  closeButton.className = 'tab-close';
  closeButton.title = t('tab.close');
  closeButton.textContent = '×';
  tab.append(label, closeButton);

  const entry = {
    path, key, title, tab, page, content, hasMath, mathOn: true, math: [], bar: null,
    // Diagramy domyślnie jako kod; Mermaid (5 MB) ładuje się dopiero po włączeniu przyciskiem.
    hasDiagrams, diagramsOn: false, diagrams: [], diagramsRendered: false,
    remoteBar: null, remoteAllowed: false,
    // Pozycja czytania z poprzedniego uruchomienia: odtwarzana, gdy karta jest widoczna, i jeszcze raz
    // po wzorach/diagramach (zmieniają wysokość bloków).
    position: position ?? null, pendingPosition: position ?? null, enhanced: false,
  };
  tab.addEventListener('click', (event) => {
    if (event.target === closeButton) close(entry);
    else activate(entry);
  });
  // Środkowy przycisk: bez autoprzewijania na mousedown, zamknięcie na auxclick.
  tab.addEventListener('mousedown', (event) => { if (event.button === 1) event.preventDefault(); });
  tab.addEventListener('auxclick', (event) => { if (event.button === 1) close(entry); });
  tab.addEventListener('contextmenu', (event) => showTabMenu(entry, event));
  tab.addEventListener('pointerdown', (event) => { if (event.button === 0 && event.target !== closeButton) dragTab(entry, event); });

  tabsEl.append(tab);
  docs.append(page);
  tabs.push(entry);
  activate(entry);
  updateRemoteBar(entry);
  if (anchor) scrollToAnchor(entry, anchor);
  enhance(entry).then(() => {
    entry.enhanced = true;
    applyPendingPosition(entry);
  });

  if (firstRender) {
    firstRender = false;
    // Dwie klatki: pierwsza układa stronę, po drugiej treść jest już narysowana.
    requestAnimationFrame(() => requestAnimationFrame(() => post({ type: 'rendered' })));
  }
}

// Obrazki z internetu nie ładują się same (nadawca pliku dowiedziałby się, że go otwarto i skąd).
// HTML trafia najpierw do <template>, gdzie przeglądarka niczego nie wczytuje; adres zdalnego obrazka
// przenosimy do data-remote-src, a pasek w karcie pozwala go wczytać.
const isRemote = (src) => /^https?:/i.test(src) && !src.startsWith(LOCAL_ORIGIN);

// Zamiast treści: duży plik (pytanie), za duży albo binarny (tylko edytor). Progi liczy host.
function noticeContent(path, { kind, megabytes, limit }) {
  const box = document.createElement('div');
  box.className = 'notice';
  const text = document.createElement('p');
  const size = megabytes.toLocaleString(I18N.language, { maximumFractionDigits: 1 });
  text.textContent = t(`notice.${kind}`, { megabytes: size, limit, seconds: Math.max(1, Math.round(megabytes)) });
  const buttons = document.createElement('div');
  buttons.className = 'notice-buttons';
  if (kind === 'large') {
    const open = iconButton(t('notice.open'), 'open', () => post({ type: 'reload', path }));
    open.classList.add('primary');
    buttons.append(open);
  }
  buttons.append(iconButton(t('notice.openInEditor'), 'edit', () => post({ type: 'edit', path })));
  box.append(text, buttons);
  return box;
}

function documentContent(path, html, error, allowRemote, notice) {
  if (notice) return noticeContent(path, notice);
  if (error) {
    const box = document.createElement('p');
    box.className = 'error';
    box.textContent = `${path}: ${error}`;
    return box;
  }
  const template = document.createElement('template');
  template.innerHTML = html;
  if (!allowRemote) {
    for (const image of template.content.querySelectorAll('img[src]')) {
      if (!isRemote(image.getAttribute('src'))) continue;
      const placeholder = document.createElement('span');
      placeholder.className = 'remote-image';
      placeholder.title = image.alt ? t('remote.altClickToLoad', { alt: image.alt }) : t('remote.clickToLoad');
      remoteImages.set(placeholder, image);
      showPlaceholderAddress(placeholder);
      image.replaceWith(placeholder);
    }
  }
  translateAlertTitles(template.content);
  addCodeHeaders(template.content);
  const doc = document.createElement('article');
  doc.className = 'markdown-body';
  doc.append(template.content);
  return doc;
}

// comrak nadaje alertom angielskie tytuły (Note, Warning…); własne tytuły z pliku zostają bez zmian.
const ALERT_DEFAULT_TITLES = { note: 'Note', tip: 'Tip', important: 'Important', warning: 'Warning', caution: 'Caution' };

function translateAlertTitles(root) {
  for (const title of root.querySelectorAll('.markdown-alert-title')) {
    const kind = title.parentElement.className.match(/markdown-alert-(\w+)/)?.[1];
    if (kind && title.textContent.trim() === ALERT_DEFAULT_TITLES[kind]) title.textContent = t(`alert.${kind}`);
  }
}

// Pasek nad blokiem kodu: język i „Kopiuj”. Osobny element przed <pre> (nie opakowanie), żeby <pre> zostało
// blokiem pozycji czytania; narysowany diagram Mermaid podmienia samo <pre>, a pasek chowa wtedy CSS.
function addCodeHeaders(root) {
  for (const pre of root.querySelectorAll('pre:not([data-math-style])')) {
    const header = document.createElement('div');
    header.className = 'code-header';
    const language = Object.assign(document.createElement('span'), { textContent: pre.getAttribute('lang') ?? '' });
    const copy = iconButton('', 'copy', () => {
      post({ type: 'copyText', text: pre.textContent });
      showCopyState(copy, true);
      clearTimeout(copy.timer);
      copy.timer = setTimeout(() => showCopyState(copy, false), 1500);
    });
    showCopyState(copy, false);
    header.append(language, copy);
    pre.before(header);
  }
}

function showCopyState(button, copied) {
  button.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="${ICONS[copied ? 'keep' : 'copy']}"/></svg>`;
  button.append(t(copied ? 'code.copied' : 'code.copy'));
}

// Znacznik miejsca → oryginalny <img> (wstawiany dopiero po zgodzie użytkownika).
const remoteImages = new WeakMap();

// Znacznik pokazuje adres obrazka; klik pyta w tym samym miejscu, czy go wczytać.
function showPlaceholderAddress(placeholder) {
  const src = remoteImages.get(placeholder).getAttribute('src');
  placeholder.classList.remove('asking');
  placeholder.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="${ICONS.image}"/></svg>`;
  placeholder.append(src.replace(/^https?:\/\//i, ''));
}

function askToLoad(placeholder) {
  const host = new URL(remoteImages.get(placeholder).getAttribute('src')).host;
  placeholder.classList.add('asking');
  const question = document.createElement('span');
  question.textContent = t('remote.ask', { host });
  const load = iconButton(t('remote.show'), 'image', (event) => {
    event.stopPropagation();
    const entry = tabs.find((tab) => tab.content.contains(placeholder));
    placeholder.replaceWith(remoteImages.get(placeholder));
    if (entry) updateRemoteBar(entry);
  });
  load.classList.add('primary');
  const cancel = iconButton(t('remote.cancel'), 'close', (event) => {
    event.stopPropagation();
    showPlaceholderAddress(placeholder);
  });
  placeholder.replaceChildren(question, load, cancel);
}

docs.addEventListener('click', (event) => {
  const placeholder = event.target.closest('.remote-image');
  if (placeholder && !placeholder.classList.contains('asking')) askToLoad(placeholder);
});

function showRemoteImages(entry) {
  entry.remoteAllowed = true;
  for (const placeholder of entry.content.querySelectorAll('.remote-image')) {
    placeholder.replaceWith(remoteImages.get(placeholder));
  }
  updateRemoteBar(entry);
}

function updateRemoteBar(entry) {
  const count = entry.content.querySelectorAll('.remote-image').length;
  if (!count) {
    entry.remoteBar?.remove();
    entry.remoteBar = null;
    return;
  }
  entry.remoteBar ??= document.createElement('div');
  const bar = entry.remoteBar;
  bar.className = 'file-bar file-bar-remote';
  const text = document.createElement('span');
  text.textContent = t('remote.bar', { count });
  const show = iconButton(t('remote.showAll'), 'image', () => showRemoteImages(entry));
  show.classList.add('primary');
  bar.replaceChildren(text, show);
  bar.hidden = entry !== active;
  fileBars.append(bar);
}

function findTab(path) {
  const key = path.toLowerCase();
  return tabs.find((entry) => entry.key === key);
}

// Zmiany pliku na dysku: pasek nad dokumentem i kropka na karcie; treść zmienia się dopiero po „Odśwież”.

// Ikony przycisków paska pliku, w tym samym stylu co w index.html (obrys 16×16, kolor z tekstu przycisku).
const ICONS = {
  refresh: 'M13 8a5 5 0 1 1-1.5-3.5M13 2.5v3h-3',
  ignore: 'M2 8s2.2-4 6-4 6 4 6 4-2.2 4-6 4-6-4-6-4zM6.5 8a1.5 1.5 0 0 0 3 0M2.5 2.5l11 11',
  close: 'M4 4l8 8M12 4l-8 8',
  keep: 'M3 8.5l3 3 7-7',
  image: 'M2.5 3.5h11v9h-11zM2.5 10.5l3-3 3 3 2-2 3 3M10.5 6.5h.01',
  open: 'M3.5 2.5h6l3 3v8h-9zM9.5 2.5v3h3',
  edit: 'M10.5 2.5l3 3L6 13H3v-3zM9 4l3 3',
  install: 'M8 2.5v8M4.5 7L8 10.5 11.5 7M3 13.5h10',
  copy: 'M5.5 5.5h8v8h-8zM2.5 10.5v-8h8',
};

function iconButton(label, icon, action) {
  const button = Object.assign(document.createElement('button'), { type: 'button', className: 'tool' });
  button.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="${ICONS[icon]}"/></svg>`;
  button.append(label);
  button.addEventListener('click', action);
  return button;
}

function showFileBar(entry, kind) {
  entry.bar?.remove();
  const bar = document.createElement('div');
  bar.className = `file-bar file-bar-${kind}`;
  const text = document.createElement('span');
  const buttons = kind === 'changed'
    ? [iconButton(t('file.reload'), 'refresh', () => post({ type: 'reload', path: entry.path })),
      iconButton(t('file.ignore'), 'ignore', () => hideFileBar(entry))]
    : [iconButton(t('file.closeTab'), 'close', () => close(entry)), iconButton(t('file.keep'), 'keep', () => hideFileBar(entry))];
  text.textContent = t(kind === 'changed' ? 'file.changed' : 'file.deleted');
  buttons[0].classList.add('primary');
  bar.append(text, ...buttons);
  // Pasek leży nad obszarem przewijania, więc przesuwa dokument zamiast zasłaniać czytane miejsce.
  bar.hidden = entry !== active;
  fileBars.append(bar);
  entry.bar = bar;
  entry.tab.classList.add('stale');
}

function hideFileBar(entry) {
  entry.bar?.remove();
  entry.bar = null;
  entry.tab.classList.remove('stale');
}

// Pozycja czytania: pierwszy widoczny blok (jego treść i wiersz źródła z data-sourcepos). Po odświeżeniu
// wracamy do bloku o tej samej treści najbliżej dawnego wiersza, więc dopisane wyżej wiersze nie przesuwają
// widoku; gdy takiego bloku już nie ma, do ostatniego bloku zaczynającego się nie dalej niż dawny wiersz.
const blockLine = (block) => parseInt(block.dataset.sourcepos, 10);
const blockSignature = (block) => block.tagName + block.textContent.slice(0, 120);

function readingPosition(page) {
  const top = page.getBoundingClientRect().top;
  for (const block of page.querySelectorAll('.markdown-body > [data-sourcepos]')) {
    const rect = block.getBoundingClientRect();
    if (rect.bottom > top) return { line: blockLine(block), signature: blockSignature(block), offset: rect.top - top };
  }
  return null;
}

function restoreReadingPosition(page, position) {
  if (!position) return;
  const blocks = [...page.querySelectorAll('.markdown-body > [data-sourcepos]')];
  const distance = (block) => Math.abs(blockLine(block) - position.line);
  const same = blocks.filter((block) => blockSignature(block) === position.signature);
  const target = same.length
    ? same.reduce((best, block) => (distance(block) < distance(best) ? block : best))
    : blocks.filter((block) => blockLine(block) <= position.line).at(-1);
  if (target) page.scrollTop += target.getBoundingClientRect().top - page.getBoundingClientRect().top - position.offset;
}

function reloadDocument({ path, html, error, notice }) {
  const entry = findTab(path);
  if (!entry) return;
  const position = readingPosition(entry.page);
  hideFileBar(entry);
  const content = documentContent(path, html, error, entry.remoteAllowed, notice);
  entry.content.replaceWith(content);
  entry.content = content;
  updateRemoteBar(entry);
  entry.math = [];
  entry.hasMath = content.querySelector('[data-math-style]') !== null;
  entry.diagrams = [];
  entry.diagramsRendered = false;
  entry.hasDiagrams = content.querySelector('pre[lang="mermaid"]') !== null;
  if (entry === active) {
    showToggles(entry);
    updateOutline();
  }
  restoreReadingPosition(entry.page, position);
  // Wzory i diagramy zmieniają wysokość bloków, więc po ich narysowaniu jeszcze raz.
  enhance(entry).then(() => restoreReadingPosition(entry.page, position));
}

function activate(entry) {
  // Ukryta karta nie ma wymiarów, więc pozycję czytania zapamiętujemy, zanim zniknie z widoku.
  if (active && active !== entry && tabs.includes(active)) active.position = readingPosition(active.page);
  for (const other of tabs) {
    const isActive = other === entry;
    other.tab.classList.toggle('active', isActive);
    other.tab.setAttribute('aria-selected', String(isActive));
    other.page.hidden = !isActive;
    if (other.bar) other.bar.hidden = !isActive;
    if (other.remoteBar) other.remoteBar.hidden = !isActive;
  }
  active = entry;
  empty.hidden = true;
  editTools.hidden = false;
  tabListTools.hidden = false;
  outlineToggle.hidden = false;
  showToggles(entry);
  entry.tab.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  entry.page.focus({ preventScroll: true });
  post({ type: 'active', path: entry.path, title: entry.title });
  updateOutline();
  applyPendingPosition(entry);
  saveSession();
}

// Sesja (karty, pozycje, aktywna) do odtworzenia przy następnym uruchomieniu. Zmiana kart zapisuje od razu,
// przewijanie z opóźnieniem (debounce: 0,4 s po ostatnim ruchu). Do końca startu (karty z poprzedniej sesji
// i z argumentów) zapis jest wstrzymany; potem jeden zapis całości.

let sessionTimer = null;
let sessionReady = false;

function saveSession(immediate = true) {
  if (!sessionReady) return;
  clearTimeout(sessionTimer);
  const save = () => post({
    type: 'session',
    tabs: tabs.map((entry) => ({
      path: entry.path,
      position: entry === active ? readingPosition(entry.page) ?? entry.position : entry.position,
    })),
    active: active && tabs.includes(active) ? active.path : null,
  });
  if (immediate) save();
  else sessionTimer = setTimeout(save, 400);
}

function applyPendingPosition(entry) {
  if (!entry.pendingPosition || entry !== active) return;
  restoreReadingPosition(entry.page, entry.pendingPosition);
  if (entry.enhanced) entry.pendingPosition = null;
}

docs.addEventListener('scroll', () => { if (active) saveSession(false); }, true);

// Rozszerzenia: biblioteki ładowane dopiero, gdy dokument ich potrzebuje.

const loaded = new Map();

function load(src) {
  if (!loaded.has(src)) {
    loaded.set(src, new Promise((resolve, reject) => {
      const element = src.endsWith('.css')
        ? Object.assign(document.createElement('link'), { rel: 'stylesheet', href: src })
        : Object.assign(document.createElement('script'), { src });
      element.onload = resolve;
      element.onerror = () => reject(new Error(t('load.failed', { src })));
      document.head.append(element);
    }));
  }
  return loaded.get(src);
}

async function enhance(entry) {
  const page = entry.page;
  const code = page.querySelectorAll('pre[lang]:not([lang="mermaid"]):not([data-math-style]) > code');
  const math = page.querySelectorAll('[data-math-style]');
  const diagrams = page.querySelectorAll('pre[lang="mermaid"]');
  const jobs = [];
  if (code.length) jobs.push(highlightCode(code));
  if (math.length) jobs.push(renderMath(entry, math));
  if (diagrams.length && entry.diagramsOn) jobs.push(renderDiagrams(entry, diagrams));
  for (const result of await Promise.allSettled(jobs)) {
    if (result.status === 'rejected') console.error(result.reason);
  }
}

async function highlightCode(blocks) {
  // Kolory tokenów są w themes.css (zmienne --tok-*), więc bez arkuszy highlight.js.
  await load('vendor/highlight/highlight.min.js');
  for (const code of blocks) {
    const language = code.parentElement.getAttribute('lang');
    if (!hljs.getLanguage(language)) continue;
    code.innerHTML = hljs.highlight(code.textContent, { language, ignoreIllegals: true }).value;
    code.classList.add('hljs');
  }
}

async function renderMath(entry, elements) {
  await Promise.all([load('vendor/katex/katex.min.js'), load('vendor/katex/katex.min.css')]);
  for (const source of elements) {
    const display = source.dataset.mathStyle === 'display';
    const rendered = document.createElement(display ? 'div' : 'span');
    rendered.className = display ? 'math math-display' : 'math';
    const tex = source.textContent;
    rendered.dataset.tex = tex;
    katex.render(tex, rendered, { displayMode: display, throwOnError: false });
    // Wyłączone wzory pokazują tekst jak w pliku: $…$ i $$…$$ wracają jako zwykły tekst, `$`…`$` jako kod.
    const plain = source.tagName === 'SPAN'
      ? document.createTextNode(display ? `$$${tex}$$` : `$${tex}$`)
      : source;
    source.replaceWith(entry.mathOn ? rendered : plain);
    entry.math.push({ source: plain, rendered });
  }
}

let diagramCount = 0;

function initializeMermaid() {
  const dark = document.documentElement.dataset.scheme === 'dark';
  mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: dark ? 'dark' : 'default' });
}

async function renderDiagrams(entry, blocks) {
  // Rysowane raz na treść (także gdy wszystkie diagramy są błędne); przełącznik potem tylko podmienia.
  entry.diagramsRendered = true;
  await load('vendor/mermaid/mermaid.min.js');
  initializeMermaid();
  for (const pre of blocks) {
    const id = `mermaid-${++diagramCount}`;
    try {
      const { svg } = await mermaid.render(id, pre.textContent);
      const diagram = document.createElement('div');
      diagram.className = 'mermaid-diagram';
      diagram.dataset.source = pre.textContent;
      diagram.innerHTML = svg;
      entry.diagrams.push({ source: pre, rendered: diagram });
      // Przycisk mógł zostać kliknięty jeszcze raz w trakcie rysowania.
      if (entry.diagramsOn) pre.replaceWith(diagram);
    } catch (error) {
      // Zostaje źródło diagramu, pod nim pierwsza linia błędu (pełny opis w podpowiedzi). Przy wyłączonych
      // diagramach napis chowa CSS (.page bez .diagrams-on).
      document.getElementById(`d${id}`)?.remove();
      const message = error.message ?? String(error);
      pre.classList.add('mermaid-error');
      pre.title = t('mermaid.error', { message });
      const note = document.createElement('div');
      note.className = 'mermaid-error-message';
      note.title = pre.title;
      note.textContent = t('mermaid.error', { message: message.split('\n')[0].replace(/:$/, '') });
      pre.after(note);
    }
  }
}

// Diagramy mają kolory wpisane w SVG, więc po zmianie jasny/ciemny rysujemy je od nowa
// (także te schowane przełącznikiem).
async function rerenderDiagrams() {
  const diagrams = tabs.flatMap((entry) => entry.diagrams.map(({ rendered }) => rendered));
  if (!diagrams.length) return;
  initializeMermaid();
  for (const diagram of diagrams) {
    try {
      const { svg } = await mermaid.render(`mermaid-${++diagramCount}`, diagram.dataset.source);
      diagram.innerHTML = svg;
    } catch (error) {
      console.error(error);
    }
  }
}

const mathToggle = document.getElementById('math-toggle');
const diagramToggle = document.getElementById('diagram-toggle');

// Przełączniki widać tylko przy dokumencie, który ma wzory / diagramy; stan jest osobny dla każdej karty.
function showToggles(entry) {
  mathToggle.hidden = !entry.hasMath;
  mathToggle.setAttribute('aria-pressed', String(entry.mathOn));
  mathToggle.title = t(entry.mathOn ? 'math.on' : 'math.off');
  diagramToggle.hidden = !entry.hasDiagrams;
  diagramToggle.setAttribute('aria-pressed', String(entry.diagramsOn));
  diagramToggle.title = t(entry.diagramsOn ? 'diagrams.on' : 'diagrams.off');
}

diagramToggle.addEventListener('click', async () => {
  const entry = active;
  if (!entry) return;
  entry.diagramsOn = !entry.diagramsOn;
  entry.page.classList.toggle('diagrams-on', entry.diagramsOn);
  showToggles(entry);
  if (entry.diagramsOn && !entry.diagramsRendered) {
    // Pierwsze włączenie: ładowanie Mermaida i rysowanie (chwilę trwa, przycisk w tym czasie nieaktywny).
    diagramToggle.disabled = true;
    try {
      await renderDiagrams(entry, entry.page.querySelectorAll('pre[lang="mermaid"]'));
    } catch (error) {
      console.error(error);
    } finally {
      diagramToggle.disabled = false;
    }
    return;
  }
  for (const { source, rendered } of entry.diagrams) {
    if (entry.diagramsOn) source.replaceWith(rendered);
    else rendered.replaceWith(source);
  }
});

mathToggle.addEventListener('click', () => {
  if (!active) return;
  active.mathOn = !active.mathOn;
  for (const { source, rendered } of active.math) {
    if (active.mathOn) source.replaceWith(rendered);
    else rendered.replaceWith(source);
  }
  showToggles(active);
});

// Wywoływane z menu kontekstowego hosta.
function selectAllInActive() {
  if (!active) return;
  getSelection().selectAllChildren(active.page);
}

function scrollToAnchor(entry, anchor) {
  const id = CSS.escape(decodeURIComponent(anchor));
  entry.page.querySelector(`[id="${id}"], [name="${id}"]`)?.scrollIntoView({ block: 'start' });
}

function close(entry) {
  closeTabs([entry]);
}

// Zamyka karty (jedną albo kilka z menu karty) i zapisuje sesję raz. Zamknięcie ostatniej karty zamyka
// okno jak w Notatniku; „Zamknij wszystkie” (keepWindow) zostawia puste okno jak VS Code.
function closeTabs(list, keepWindow = false) {
  const before = [...tabs];
  for (const entry of list) {
    const index = tabs.indexOf(entry);
    if (index < 0) continue;
    tabs.splice(index, 1);
    entry.tab.remove();
    entry.page.remove();
    entry.bar?.remove();
    entry.remoteBar?.remove();
    post({ type: 'closed', path: entry.path });
  }
  if (tabs.length === 0 && !keepWindow) {
    // Najpierw pusta sesja (od razu, nie z opóźnieniem), potem zamknięcie okna.
    saveSession(true);
    post({ type: 'close' });
    return;
  }
  if (tabs.length === 0) {
    active = null;
    empty.hidden = false;
    editTools.hidden = true;
    tabListTools.hidden = true;
    outlineToggle.hidden = true;
    mathToggle.hidden = true;
    diagramToggle.hidden = true;
    post({ type: 'active', path: null, title: null });
    updateOutline();
  } else if (!tabs.includes(active)) {
    // Następna karta za zamkniętą aktywną, a gdy jej nie ma, poprzednia.
    const index = before.indexOf(active);
    activate(before.slice(index).find((entry) => tabs.includes(entry)) ?? before.slice(0, index).findLast((entry) => tabs.includes(entry)));
  }
  saveSession();
}

function cycle(step) {
  if (tabs.length < 2) return;
  const index = tabs.indexOf(active);
  activate(tabs[(index + step + tabs.length) % tabs.length]);
}

// Przeciąganie karty jak w Terminalu Windows i Chrome: karta jedzie za kursorem, sąsiednie rozsuwają się
// (transform z animacją), gdy kursor minie ich środek; po puszczeniu karta dojeżdża na swoje miejsce.
function dragTab(entry, down) {
  const from = tabs.indexOf(entry);
  let dragging = false;
  let slots, step, target = from, offset = 0;

  const start = () => {
    dragging = true;
    activate(entry);
    entry.tab.setPointerCapture(down.pointerId);
    entry.tab.classList.add('dragging');
    // Za kursorem bez opóźnienia; animacja wraca dopiero przy dojeździe po puszczeniu.
    entry.tab.style.transition = 'none';
    slots = tabs.map((other) => other.tab.getBoundingClientRect());
    step = slots[from].width + (parseFloat(getComputedStyle(tabsEl).columnGap) || 0);
  };
  const move = (event) => {
    if (!dragging && Math.abs(event.clientX - down.clientX) < 5) return;
    if (!dragging) start();
    // Karta nie wyjeżdża poza pierwszą i ostatnią pozycję.
    const min = slots[0].left - slots[from].left;
    const max = slots[slots.length - 1].right - slots[from].right;
    offset = Math.min(max, Math.max(min, event.clientX - down.clientX));
    entry.tab.style.transform = `translateX(${offset}px)`;
    const center = slots[from].left + slots[from].width / 2 + offset;
    target = tabs.filter((other, index) => index !== from && slots[index].left + slots[index].width / 2 < center).length;
    tabs.forEach((other, index) => {
      if (other === entry) return;
      const shift = index > from && index <= target ? -step : index < from && index >= target ? step : 0;
      other.tab.style.transform = shift ? `translateX(${shift}px)` : '';
    });
  };
  const end = () => {
    entry.tab.removeEventListener('pointermove', move);
    entry.tab.removeEventListener('pointerup', end);
    entry.tab.removeEventListener('lostpointercapture', end);
    if (!dragging) return;
    dragging = false;
    // Nowa kolejność w DOM bez animacji (sąsiednie karty już stoją na nowych miejscach), potem dojazd
    // przeciąganej karty z miejsca, w którym ją puszczono.
    const visual = slots[from].left + offset;
    tabs.forEach((other) => { other.tab.style.transition = 'none'; other.tab.style.transform = ''; });
    tabs.splice(from, 1);
    tabs.splice(target, 0, entry);
    tabsEl.insertBefore(entry.tab, tabs[target + 1]?.tab ?? null);
    entry.tab.style.transform = `translateX(${visual - entry.tab.getBoundingClientRect().left}px)`;
    entry.tab.getBoundingClientRect();
    tabs.forEach((other) => { other.tab.style.transition = ''; });
    entry.tab.style.transform = '';
    entry.tab.addEventListener('transitionend', () => entry.tab.classList.remove('dragging'), { once: true });
    if (!offset) entry.tab.classList.remove('dragging');
    if (target !== from) saveSession();
  };
  entry.tab.addEventListener('pointermove', move);
  entry.tab.addEventListener('pointerup', end);
  entry.tab.addEventListener('lostpointercapture', end);
}

document.getElementById('help').addEventListener('click', () => post({ type: 'help' }));

window.addEventListener('keydown', (event) => {
  if (event.key === 'F1' && !event.ctrlKey && !event.altKey && !event.shiftKey) {
    event.preventDefault();
    post({ type: 'help' });
    return;
  }
  if (!event.ctrlKey || event.altKey) return;
  const key = event.key.toLowerCase();
  let handled = true;
  if (key === 'c' && event.shiftKey) copySelectionAsText();
  else if (key === 'o' && event.shiftKey) setOutlineOpen(!outlineOpen);
  else if (key === 'o') post({ type: 'openDialog' });
  else if (key === 'e') editActive();
  else if ((key === 'w' || key === 'f4') && active) close(active);
  else if (key === 'tab') cycle(event.shiftKey ? -1 : 1);
  else if (key === 'pagedown') cycle(1);
  else if (key === 'pageup') cycle(-1);
  else if (key >= '1' && key <= '8' && tabs[key - 1]) activate(tabs[key - 1]);
  else if (key === '9' && tabs.length) activate(tabs[tabs.length - 1]);
  else handled = false;
  if (handled) event.preventDefault();
}, true);

document.getElementById('new-tab').addEventListener('click', () => post({ type: 'openDialog' }));

// Edycja w zewnętrznym programie

function editActive() {
  if (active) post({ type: 'edit', path: active.path });
}

// Lista edytorów od hosta: Notatnik i zapisane programy, każdy z ikoną z własnego pliku exe.
function showEditors({ items, selectedName }) {
  editButton.title = t('edit.title', { name: selectedName });
  editorList.replaceChildren(...items.map((item) => {
    const row = document.createElement('div');
    row.className = 'menu-row';
    const choose = Object.assign(document.createElement('button'), { type: 'button' });
    choose.setAttribute('role', 'menuitemradio');
    choose.setAttribute('aria-checked', String(item.selected));
    Object.assign(choose.dataset, { action: 'selectEditor', path: item.path });
    if (item.icon) choose.append(Object.assign(document.createElement('img'), { src: item.icon, alt: '', className: 'menu-icon' }));
    choose.append(item.name);
    row.append(choose);
    if (item.removable) {
      const remove = Object.assign(document.createElement('button'), { type: 'button', title: t('edit.remove'), className: 'menu-remove' });
      remove.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="${ICONS.close}"/></svg>`;
      Object.assign(remove.dataset, { action: 'removeEditor', path: item.path });
      row.append(remove);
    }
    return row;
  }));
}

editButton.addEventListener('click', editActive);

// Instalator (pobrany exe uruchomiony bez plików): strona zamiast kart, wybory idą do hosta.

const installerForm = document.getElementById('installer-form');
const installerError = document.getElementById('installer-error');

function showInstaller({ editors, directory }) {
  document.getElementById('installer').hidden = false;
  document.getElementById('installer-intro').textContent = t('install.intro', { directory });
  document.getElementById('installer-editors').replaceChildren(...editors.map((item) => {
    const label = document.createElement('label');
    label.className = 'choice';
    const radio = Object.assign(document.createElement('input'), { type: 'radio', name: 'editor', value: item.path, checked: item.selected });
    label.append(radio);
    if (item.icon) label.append(Object.assign(document.createElement('img'), { src: item.icon, alt: '', className: 'menu-icon' }));
    label.append(item.name);
    return label;
  }));
  const install = iconButton(t('install.install'), 'install', () => {
    const form = new FormData(installerForm);
    installerError.hidden = true;
    for (const button of installerForm.querySelectorAll('button')) button.disabled = true;
    post({
      type: 'install',
      associate: form.has('associate'),
      startMenu: form.has('startMenu'),
      desktop: form.has('desktop'),
      editor: form.get('editor'),
    });
  });
  install.classList.add('primary');
  const skip = iconButton(t('install.skip'), 'open', () => {
    // Host zamienia małe okno instalatora na zwykłe.
    delete document.documentElement.dataset.installer;
    document.getElementById('installer').hidden = true;
    post({ type: 'skipInstall' });
  });
  document.getElementById('installer-buttons').replaceChildren(install, skip);
  // Okno w wysokości panelu (w pikselach ekranu, z uwzględnieniem skalowania i powiększenia).
  requestAnimationFrame(() => post({ type: 'installerHeight', height: installerForm.offsetHeight * devicePixelRatio }));
}

function showInstallError({ message }) {
  installerError.textContent = `${t('install.failed')} ${message}`;
  installerError.hidden = false;
  for (const button of installerForm.querySelectorAll('button')) button.disabled = false;
}

installerForm.addEventListener('submit', (event) => event.preventDefault());

// Rozwijane menu: otwiera przycisk, zamyka wybór pozycji, klik obok, Esc albo utrata fokusu okna.

const menus = [];

function dropdown(button, menu, onSelect) {
  // setOpen tylko ustawia stan; inne menu zamykamy przez ich setOpen, bez ponownego przełączania
  // (wcześniej wzajemne zamykanie wywoływało się w kółko i kończyło przepełnieniem stosu).
  const setOpen = (open) => {
    menu.hidden = !open;
    button.setAttribute('aria-expanded', String(open));
  };
  const toggle = (open = menu.hidden) => {
    if (open) for (const other of menus) if (other !== setOpen) other(false);
    setOpen(open);
  };
  menus.push(setOpen);
  button.addEventListener('click', (event) => {
    event.stopPropagation();
    toggle();
  });
  menu.addEventListener('click', (event) => {
    event.stopPropagation();
    const item = event.target.closest('button');
    if (!item) return;
    onSelect(item);
    toggle(false);
  });
}

const closeMenus = () => { for (const setOpen of menus) setOpen(false); };
document.addEventListener('click', closeMenus);
window.addEventListener('blur', closeMenus);
document.addEventListener('keydown', (event) => { if (event.key === 'Escape') closeMenus(); });

// Lista wszystkich kart (gdy nie mieszczą się na pasku): nazwa pliku i folder, żeby odróżnić np. kilka README.md.

const tabListButton = document.getElementById('tab-list-button');
const tabList = document.getElementById('tab-list');

function fillTabList() {
  tabList.replaceChildren(...tabs.map((entry, index) => {
    const row = document.createElement('div');
    row.className = 'menu-row';
    const item = Object.assign(document.createElement('button'), { type: 'button', title: entry.path });
    item.setAttribute('role', 'menuitemradio');
    item.setAttribute('aria-checked', String(entry === active));
    item.dataset.index = index;
    item.append(entry.title);
    const folder = entry.path.split(/[\\/]/).at(-2);
    if (folder) item.append(Object.assign(document.createElement('small'), { textContent: folder }));
    const remove = Object.assign(document.createElement('button'), { type: 'button', title: t('tab.close'), className: 'menu-remove' });
    remove.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="${ICONS.close}"/></svg>`;
    remove.dataset.close = index;
    row.append(item, remove);
    return row;
  }));
}

// „×” w liście zamyka kartę, a lista zostaje otwarta (przechwycone, zanim dropdown zamknie menu).
tabList.addEventListener('click', (event) => {
  const remove = event.target.closest('[data-close]');
  if (!remove) return;
  event.stopPropagation();
  close(tabs[remove.dataset.close]);
  if (tabs.length) fillTabList();
}, true);

tabListButton.addEventListener('click', () => {
  fillTabList();
  // Przy prawej połowie okna lista rozwija się w lewo, żeby nie wyjść poza krawędź.
  tabList.classList.toggle('menu-end', tabListButton.getBoundingClientRect().left > innerWidth / 2);
  requestAnimationFrame(() => tabList.querySelector('[aria-checked="true"]')?.scrollIntoView({ block: 'nearest' }));
});
dropdown(tabListButton, tabList, (item) => activate(tabs[item.dataset.index]));

// Kółko myszy przewija pasek kart (ruch pionowy kółka zamieniony na poziomy).
tabsEl.addEventListener('wheel', (event) => {
  if (event.ctrlKey) return;
  tabsEl.scrollLeft += event.deltaY || event.deltaX;
  event.preventDefault();
}, { passive: false });

// Menu karty pod prawym przyciskiem (jak w VS Code i PhpStorm).

const tabMenu = document.getElementById('tab-menu');
let tabMenuEntry = null;
menus.push((open) => { tabMenu.hidden = !open; });

function showTabMenu(entry, event) {
  event.preventDefault();
  event.stopPropagation();
  closeMenus();
  tabMenuEntry = entry;
  const index = tabs.indexOf(entry);
  const enabled = { closeOthers: tabs.length > 1, closeLeft: index > 0, closeRight: index < tabs.length - 1 };
  for (const item of tabMenu.querySelectorAll('button')) item.disabled = enabled[item.dataset.action] === false;
  tabMenu.hidden = false;
  const box = tabMenu.getBoundingClientRect();
  tabMenu.style.left = `${Math.min(event.clientX, innerWidth - box.width - 4)}px`;
  tabMenu.style.top = `${Math.min(event.clientY, innerHeight - box.height - 4)}px`;
}

tabMenu.addEventListener('click', (event) => {
  event.stopPropagation();
  const item = event.target.closest('button');
  if (!item || item.disabled) return;
  tabMenu.hidden = true;
  const entry = tabMenuEntry;
  const index = tabs.indexOf(entry);
  if (index < 0) return;
  const action = item.dataset.action;
  if (action === 'close') return closeTabs([entry]);
  if (action === 'closeAll') return closeTabs([...tabs], true);
  // Jak w VS Code: karta, na której otwarto menu, zostaje aktywna.
  if (action === 'closeOthers') closeTabs(tabs.filter((other) => other !== entry));
  else if (action === 'closeLeft') closeTabs(tabs.slice(0, index));
  else if (action === 'closeRight') closeTabs(tabs.slice(index + 1));
  activate(entry);
});

// Spis treści: nagłówki aktywnej karty w panelu z lewej. Otwarcie i szerokość są wspólne dla wszystkich
// plików i zapamiętane w localStorage (dane WebView2 w profilu użytkownika); poziom osobno dla każdej karty.

const outline = document.getElementById('outline');
const outlineList = document.getElementById('outline-list');
const outlineLevels = document.getElementById('outline-levels');
const outlineResizer = document.getElementById('outline-resizer');

function stored(key, fallback) {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}

function store(key, value) {
  try { localStorage.setItem(key, String(value)); } catch { /* bez zapamiętania */ }
}

let outlineOpen = stored('outline.open', '0') === '1';
let outlineDepth = Number(stored('outline.depth', '3'));
let outlineItems = [];
outline.style.setProperty('--outline-width', `${Number(stored('outline.width', '260'))}px`);

function setOutlineOpen(open) {
  outlineOpen = open;
  store('outline.open', open ? '1' : '0');
  updateOutline();
}

function updateOutline() {
  const visible = outlineOpen && active !== null;
  outline.hidden = !visible;
  outlineToggle.setAttribute('aria-pressed', String(outlineOpen));
  if (!visible) return;
  const headings = [...active.content.querySelectorAll('h1, h2, h3, h4, h5, h6')];
  const level = (heading) => Number(heading.tagName[1]);
  // Przycisków poziomów tyle, ile poziomów (do 4) ma dokument; wybrany poziom ograniczony do nich.
  const deepest = Math.min(4, Math.max(1, ...headings.map(level)));
  const depth = Math.min(active.outlineDepth ?? outlineDepth, deepest);
  for (const button of outlineLevels.children) {
    const buttonDepth = Number(button.dataset.depth);
    button.hidden = buttonDepth > deepest;
    button.setAttribute('aria-pressed', String(buttonDepth <= depth));
    button.title = t('outline.depth', { marks: ['#', '##', '###', '####'].slice(0, buttonDepth).join(', ') });
  }
  const shown = headings.filter((heading) => level(heading) <= depth);
  const top = Math.min(...shown.map(level));
  outlineItems = shown.map((heading) => {
    const item = Object.assign(document.createElement('button'), { type: 'button', textContent: heading.textContent.trim() });
    item.title = item.textContent;
    item.style.paddingLeft = `${8 + (level(heading) - top) * 14}px`;
    // Czcionka maleje z poziomem liczonym od najwyższego nagłówka w dokumencie.
    item.dataset.rank = level(heading) - top + 1;
    item.addEventListener('click', () => heading.scrollIntoView({ block: 'start' }));
    return { heading, item };
  });
  outlineList.replaceChildren(...outlineItems.map(({ item }) => item));
  if (!outlineItems.length) {
    outlineList.append(Object.assign(document.createElement('div'), { className: 'outline-empty', textContent: t('outline.empty') }));
  }
  markCurrentHeading();
}

// Podświetlony nagłówek sekcji, którą właśnie widać (ostatni nagłówek nad górną krawędzią strony).
function markCurrentHeading() {
  if (outline.hidden || !active) return;
  const top = active.page.getBoundingClientRect().top + 8;
  let current = outlineItems[0];
  for (const entry of outlineItems) {
    if (entry.heading.getBoundingClientRect().top <= top) current = entry;
    else break;
  }
  for (const { item } of outlineItems) item.classList.toggle('current', item === current?.item);
  current?.item.scrollIntoView({ block: 'nearest' });
}

let outlineFrame = 0;
docs.addEventListener('scroll', () => {
  cancelAnimationFrame(outlineFrame);
  outlineFrame = requestAnimationFrame(markCurrentHeading);
}, true);

outlineToggle.addEventListener('click', () => setOutlineOpen(!outlineOpen));
document.getElementById('outline-close').addEventListener('click', () => setOutlineOpen(false));

outlineLevels.addEventListener('click', (event) => {
  const button = event.target.closest('button');
  if (!button) return;
  // Poziom zapamiętany dla tej karty (tylko do zamknięcia programu); ostatni wybór to też domyślny
  // poziom dla nowo otwieranych plików.
  outlineDepth = Number(button.dataset.depth);
  if (active) active.outlineDepth = outlineDepth;
  store('outline.depth', outlineDepth);
  updateOutline();
});

// Przeciąganie prawej krawędzi panelu zmienia szerokość (180 px – połowa okna).
outlineResizer.addEventListener('pointerdown', (down) => {
  if (down.button !== 0) return;
  down.preventDefault();
  outlineResizer.setPointerCapture(down.pointerId);
  outlineResizer.classList.add('dragging');
  const startWidth = outline.getBoundingClientRect().width;
  let width = startWidth;
  const move = (event) => {
    width = Math.round(Math.min(innerWidth / 2, Math.max(180, startWidth + event.clientX - down.clientX)));
    outline.style.setProperty('--outline-width', `${width}px`);
  };
  const end = () => {
    outlineResizer.removeEventListener('pointermove', move);
    outlineResizer.removeEventListener('pointerup', end);
    outlineResizer.removeEventListener('lostpointercapture', end);
    outlineResizer.classList.remove('dragging');
    store('outline.width', width);
  };
  outlineResizer.addEventListener('pointermove', move);
  outlineResizer.addEventListener('pointerup', end);
  outlineResizer.addEventListener('lostpointercapture', end);
});

dropdown(editMenuButton, editMenu, (item) => {
  const { action, path } = item.dataset;
  if (action === 'chooseEditor') post({ type: action });
  else if (action === 'removeEditor') post({ type: action, path });
  else if (action === 'selectEditor') {
    // Wybór z listy: program staje się domyślny i od razu otwiera bieżący plik.
    post({ type: action, path });
    editActive();
  }
});

// Wygląd: motyw i tryb. Host wstawia startowe data-theme/data-mode/data-scheme do <html>,
// żeby pierwsza klatka miała już właściwe kolory.

const root = document.documentElement;
const appearanceMenu = document.getElementById('appearance-menu');
const systemDark = matchMedia('(prefers-color-scheme: dark)');
const appearance = { theme: root.dataset.theme || 'github', mode: root.dataset.mode || 'system' };

function applyAppearance() {
  const scheme = appearance.mode === 'system' ? (systemDark.matches ? 'dark' : 'light') : appearance.mode;
  const schemeChanged = root.dataset.scheme !== scheme;
  root.dataset.theme = appearance.theme;
  root.dataset.mode = appearance.mode;
  root.dataset.scheme = scheme;
  for (const item of appearanceMenu.querySelectorAll('[data-theme]')) {
    item.setAttribute('aria-checked', String(item.dataset.theme === appearance.theme));
  }
  for (const item of appearanceMenu.querySelectorAll('[data-mode]')) {
    item.setAttribute('aria-checked', String(item.dataset.mode === appearance.mode));
  }
  // Kolory dla paska tytułu okna i tła WebView przed załadowaniem strony.
  const style = getComputedStyle(root);
  const color = (name) => style.getPropertyValue(name).trim();
  post({
    type: 'appearance',
    theme: appearance.theme,
    mode: appearance.mode,
    caption: color('--tabbar-bg'),
    text: color('--fg'),
    background: color('--bg'),
  });
  if (schemeChanged) rerenderDiagrams();
}

dropdown(document.getElementById('appearance-button'), appearanceMenu, (item) => {
  if (item.dataset.language) {
    // Zmiana języka uruchamia aplikację od nowa z tymi samymi kartami (aktywna na końcu).
    if (item.dataset.language !== I18N.setting) {
      post({ type: 'setLanguage', language: item.dataset.language, paths: tabs.map((entry) => entry.path), active: active?.path });
    }
    return;
  }
  if (item.dataset.theme) appearance.theme = item.dataset.theme;
  if (item.dataset.mode) appearance.mode = item.dataset.mode;
  applyAppearance();
});
for (const item of appearanceMenu.querySelectorAll('[data-language]')) {
  item.setAttribute('aria-checked', String(item.dataset.language === I18N.setting));
}
systemDark.addEventListener('change', () => { if (appearance.mode === 'system') applyAppearance(); });
applyAppearance();

// Kopiowanie z formatowaniem: sam HTML zaznaczenia, bez stylów wyliczonych z motywu
// (inaczej np. jasny tekst z ciemnego motywu trafiałby do Worda jako biały).
// Wzory KaTeX wracają jako źródło $…$, bo ich tekst to zlepek MathML i HTML.
function selectionContent() {
  const selection = getSelection();
  if (selection.isCollapsed) return null;
  const container = document.createElement('div');
  for (let i = 0; i < selection.rangeCount; i++) container.append(selection.getRangeAt(i).cloneContents());
  for (const element of container.querySelectorAll('[data-sourcepos]')) element.removeAttribute('data-sourcepos');
  // Paski nad kodem i napisy o błędach diagramów to interfejs, nie treść dokumentu.
  for (const element of container.querySelectorAll('.code-header, .mermaid-error-message')) element.remove();
  for (const math of container.querySelectorAll('.math')) {
    const fence = math.classList.contains('math-display') ? '$$' : '$';
    math.replaceWith(`${fence}${math.dataset.tex}${fence}`);
  }
  // innerText (z podziałem na wiersze) liczy się tylko dla elementu w dokumencie.
  container.style.cssText = 'position: fixed; left: -10000px; top: 0; white-space: normal;';
  document.body.append(container);
  const text = container.innerText;
  container.remove();
  container.removeAttribute('style');
  return { html: container.innerHTML, text };
}

// Ctrl+Shift+C i „Kopiuj jako tekst” z menu hosta.
function copySelectionAsText() {
  const content = selectionContent();
  if (content) post({ type: 'copyText', text: content.text });
}

document.addEventListener('copy', (event) => {
  const content = selectionContent();
  if (!content) return;
  event.clipboardData.setData('text/html', content.html);
  event.clipboardData.setData('text/plain', content.text);
  event.preventDefault();
});

// Linki: zwykły klik tylko dla kotwic, resztę otwiera Ctrl+klik (host decyduje: karta, przeglądarka, Eksplorator).

const LOCAL_PREFIX = 'https://mdread.localhost/local/';
const linkTip = document.createElement('div');
linkTip.id = 'link-tip';
linkTip.hidden = true;
document.body.append(linkTip);

function linkAddress(link) {
  const href = link.getAttribute('href');
  if (href.startsWith('#')) return href;
  if (!link.href.startsWith(LOCAL_PREFIX)) return link.href;
  const [path, anchor] = link.href.slice(LOCAL_PREFIX.length).split('#');
  return decodeURIComponent(path).replaceAll('/', '\\') + (anchor ? `#${decodeURIComponent(anchor)}` : '');
}

docs.addEventListener('click', (event) => {
  const link = event.target.closest('a[href]');
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute('href');
  if (href.startsWith('#')) scrollToAnchor(active, href.slice(1));
  else if (event.ctrlKey) post({ type: 'link', url: link.href });
});

docs.addEventListener('mouseover', (event) => {
  const link = event.target.closest('a[href]');
  if (!link) return;
  // Własny tooltip zamiast natywnego, żeby nie było dwóch naraz.
  if (link.title) {
    link.dataset.title = link.title;
    link.removeAttribute('title');
  }
  const isAnchor = link.getAttribute('href').startsWith('#');
  linkTip.replaceChildren();
  if (link.dataset.title) linkTip.append(Object.assign(document.createElement('strong'), { textContent: link.dataset.title }));
  linkTip.append(Object.assign(document.createElement('span'), { textContent: linkAddress(link) }));
  if (!isAnchor) linkTip.append(Object.assign(document.createElement('small'), { textContent: t('link.ctrlClick') }));
  linkTip.hidden = false;
});

docs.addEventListener('mousemove', (event) => {
  if (linkTip.hidden) return;
  const margin = 16;
  const x = Math.min(event.clientX + margin, innerWidth - linkTip.offsetWidth - 4);
  const y = event.clientY + margin + linkTip.offsetHeight > innerHeight
    ? event.clientY - margin - linkTip.offsetHeight
    : event.clientY + margin;
  linkTip.style.transform = `translate(${Math.max(4, x)}px, ${y}px)`;
});

docs.addEventListener('mouseout', (event) => {
  if (event.target.closest('a[href]') && !event.relatedTarget?.closest?.('a[href]')) linkTip.hidden = true;
});
docs.addEventListener('scroll', () => { linkTip.hidden = true; }, true);

// Upuszczone pliki idą do hosta jako obiekty File (host odczyta z nich ścieżki); bez tego WebView nawigowałby do pliku.
window.addEventListener('dragover', (event) => {
  event.preventDefault();
  event.dataTransfer.dropEffect = 'copy';
});
window.addEventListener('drop', (event) => {
  event.preventDefault();
  const files = [...event.dataTransfer.files];
  if (files.length) host.postMessageWithAdditionalObjects({ type: 'drop' }, files);
});

host.addEventListener('message', (event) => {
  const message = event.data;
  if (message.type === 'open') openDocument(message);
  else if (message.type === 'reload') reloadDocument(message);
  else if (message.type === 'fileChanged' || message.type === 'fileDeleted') {
    const entry = findTab(message.path);
    if (entry) showFileBar(entry, message.type === 'fileChanged' ? 'changed' : 'deleted');
  }
  else if (message.type === 'editors') showEditors(message);
  else if (message.type === 'installer') showInstaller(message);
  else if (message.type === 'installError') showInstallError(message);
  else if (message.type === 'startupDone') {
    sessionReady = true;
    saveSession();
  }
});

post({ type: 'ready' });
