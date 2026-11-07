'use strict';

const host = window.chrome.webview;
const docs = document.getElementById('docs');

function post(message) {
  host.postMessage(message);
}

function openDocument({ html }) {
  const doc = document.createElement('article');
  doc.className = 'markdown-body';
  doc.innerHTML = html;
  docs.replaceChildren(doc);
  // Dwie klatki: pierwsza układa stronę, po drugiej treść jest już narysowana.
  requestAnimationFrame(() => requestAnimationFrame(() => post({ type: 'rendered' })));
}

function showError({ message }) {
  const box = document.createElement('p');
  box.className = 'error';
  box.textContent = message;
  docs.replaceChildren(box);
}

host.addEventListener('message', (event) => {
  const message = event.data;
  switch (message.type) {
    case 'open': openDocument(message); break;
    case 'error': showError(message); break;
  }
});

post({ type: 'ready' });
