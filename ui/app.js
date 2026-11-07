'use strict';

const host = window.chrome.webview;
const tabsEl = document.getElementById('tabs');
const docs = document.getElementById('docs');
const empty = document.getElementById('empty');
const tools = document.getElementById('tools');
const editButton = document.getElementById('edit');
const editMenuButton = document.getElementById('edit-menu-button');
const editMenu = document.getElementById('edit-menu');
const customEditor = document.getElementById('custom-editor');

/** @type {{ path: string, key: string, title: string, tab: HTMLElement, page: HTMLElement }[]} */
const tabs = [];
let active = null;
let firstRender = true;

function post(message) {
  host.postMessage(message);
}

function openDocument({ path, title, html, error, anchor }) {
  const key = path.toLowerCase();
  const existing = tabs.find((entry) => entry.key === key);
  if (existing) {
    activate(existing);
    if (anchor) scrollToAnchor(existing, anchor);
    return;
  }

  const page = document.createElement('div');
  page.className = 'page';
  page.tabIndex = -1;
  page.hidden = true;
  if (error) {
    const box = document.createElement('p');
    box.className = 'error';
    box.textContent = `${path}: ${error}`;
    page.append(box);
  } else {
    const doc = document.createElement('article');
    doc.className = 'markdown-body';
    doc.innerHTML = html;
    page.append(doc);
  }

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
  closeButton.title = 'Zamknij (Ctrl+W)';
  closeButton.textContent = '×';
  tab.append(label, closeButton);

  const entry = { path, key, title, tab, page };
  tab.addEventListener('click', (event) => {
    if (event.target === closeButton) close(entry);
    else activate(entry);
  });
  // Środkowy przycisk: bez autoprzewijania na mousedown, zamknięcie na auxclick.
  tab.addEventListener('mousedown', (event) => { if (event.button === 1) event.preventDefault(); });
  tab.addEventListener('auxclick', (event) => { if (event.button === 1) close(entry); });

  tabsEl.append(tab);
  docs.append(page);
  tabs.push(entry);
  activate(entry);
  if (anchor) scrollToAnchor(entry, anchor);

  if (firstRender) {
    firstRender = false;
    // Dwie klatki: pierwsza układa stronę, po drugiej treść jest już narysowana.
    requestAnimationFrame(() => requestAnimationFrame(() => post({ type: 'rendered' })));
  }
}

function activate(entry) {
  for (const other of tabs) {
    const isActive = other === entry;
    other.tab.classList.toggle('active', isActive);
    other.tab.setAttribute('aria-selected', String(isActive));
    other.page.hidden = !isActive;
  }
  active = entry;
  empty.hidden = true;
  tools.hidden = false;
  entry.tab.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  entry.page.focus({ preventScroll: true });
  post({ type: 'active', path: entry.path, title: entry.title });
}

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
  const index = tabs.indexOf(entry);
  if (index < 0) return;
  tabs.splice(index, 1);
  entry.tab.remove();
  entry.page.remove();
  if (tabs.length === 0) {
    post({ type: 'close' });
    return;
  }
  if (active === entry) activate(tabs[Math.min(index, tabs.length - 1)]);
}

function cycle(step) {
  if (tabs.length < 2) return;
  const index = tabs.indexOf(active);
  activate(tabs[(index + step + tabs.length) % tabs.length]);
}

window.addEventListener('keydown', (event) => {
  if (!event.ctrlKey || event.altKey) return;
  const key = event.key.toLowerCase();
  let handled = true;
  if (key === 'c' && event.shiftKey) post({ type: 'copyText', text: getSelection().toString() });
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

function showEditor({ name, custom }) {
  editButton.title = `Edytuj w: ${name} (Ctrl+E)`;
  customEditor.hidden = !custom;
  customEditor.textContent = custom ? name : '';
  editMenu.querySelector('[data-action="resetEditor"]').setAttribute('aria-checked', String(!custom));
  customEditor.setAttribute('aria-checked', String(custom));
}

function toggleEditMenu(open = editMenu.hidden) {
  editMenu.hidden = !open;
  editMenuButton.setAttribute('aria-expanded', String(open));
}

editButton.addEventListener('click', editActive);
editMenuButton.addEventListener('click', (event) => {
  event.stopPropagation();
  toggleEditMenu();
});
editMenu.addEventListener('click', (event) => {
  const action = event.target.closest('button')?.dataset.action;
  if (action === 'resetEditor' || action === 'chooseEditor') post({ type: action });
  toggleEditMenu(false);
});
document.addEventListener('click', () => toggleEditMenu(false));
window.addEventListener('blur', () => toggleEditMenu(false));
document.addEventListener('keydown', (event) => { if (event.key === 'Escape') toggleEditMenu(false); });

// Kopiowanie z formatowaniem: sam HTML zaznaczenia, bez stylów wyliczonych z motywu
// (inaczej np. jasny tekst z ciemnego motywu trafiałby do Worda jako biały).
document.addEventListener('copy', (event) => {
  const selection = getSelection();
  if (selection.isCollapsed) return;
  const container = document.createElement('div');
  for (let i = 0; i < selection.rangeCount; i++) container.append(selection.getRangeAt(i).cloneContents());
  for (const element of container.querySelectorAll('[data-sourcepos]')) element.removeAttribute('data-sourcepos');
  event.clipboardData.setData('text/html', container.innerHTML);
  event.clipboardData.setData('text/plain', selection.toString());
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
  if (!isAnchor) linkTip.append(Object.assign(document.createElement('small'), { textContent: 'Ctrl+klik, aby otworzyć' }));
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
  else if (message.type === 'editor') showEditor(message);
});

post({ type: 'ready' });
