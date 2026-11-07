'use strict';

const host = window.chrome.webview;
const tabsEl = document.getElementById('tabs');
const docs = document.getElementById('docs');
const empty = document.getElementById('empty');

/** @type {{ key: string, title: string, tab: HTMLElement, page: HTMLElement }[]} */
const tabs = [];
let active = null;
let firstRender = true;

function post(message) {
  host.postMessage(message);
}

function openDocument({ path, title, html, error }) {
  const key = path.toLowerCase();
  const existing = tabs.find((entry) => entry.key === key);
  if (existing) {
    activate(existing);
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

  const entry = { key, title, tab, page };
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
  entry.tab.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  entry.page.focus({ preventScroll: true });
  post({ type: 'title', title: entry.title });
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
  if (key === 'o') post({ type: 'openDialog' });
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
});

post({ type: 'ready' });
