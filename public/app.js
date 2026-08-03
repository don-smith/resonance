const appShell = document.querySelector('.app-shell');
const primaryNavigation = document.querySelector('#primary-navigation');
const documentSidebar = document.querySelector('#document-sidebar');
const treeElement = document.querySelector('#document-tree');
const projectNameElement = document.querySelector('#project-name');
const documentCountElement = document.querySelector('#document-count');
const documentPathElement = document.querySelector('#document-path');
const sectionLabelElement = document.querySelector('#section-label');
const documentContentElement = document.querySelector('#document-content');

let repository = { documents: [] };
let selectedPath = null;

function escapeHtml(value) {
  return value.replace(/[&<>"']/g, (character) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#039;',
  }[character]));
}

function renderTree(nodes) {
  return nodes.map((node) => {
    if (node.type === 'folder') {
      return `<details class="tree-folder" open><summary>${escapeHtml(node.name)}</summary><div class="tree-children">${renderTree(node.children)}</div></details>`;
    }
    return `<button class="tree-file" type="button" data-path="${escapeHtml(node.path)}">${escapeHtml(node.name)}</button>`;
  }).join('');
}

function setActiveDocument(path) {
  selectedPath = path;
  document.querySelectorAll('.tree-file').forEach((button) => {
    button.classList.toggle('active', button.dataset.path === path);
  });
}

function setPrimaryView(view) {
  const docsActive = view === 'docs';
  appShell.classList.toggle('docs-active', docsActive);
  documentSidebar.hidden = !docsActive;
  primaryNavigation.querySelectorAll('[data-view]').forEach((button) => {
    const active = button.dataset.view === view;
    button.classList.toggle('active', active);
    if (active) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  });
}

function showHome() {
  setPrimaryView('home');
  setActiveDocument(null);
  sectionLabelElement.textContent = 'HOME';
  documentPathElement.textContent = 'theview';
  documentContentElement.innerHTML = `
    <p class="eyebrow">A LOCAL DEVELOPMENT COCKPIT</p>
    <h2>See the shape of what you’re building.</h2>
    <p class="lead">theview turns the Markdown already living in a repository into a calm, navigable workspace. Choose Docs to browse the document tree, then select a file to begin reading.</p>
    <div class="welcome-line"><span class="signal-dot"></span><span>Connected to the current repository</span></div>
  `;
}

async function showDocument(path) {
  setPrimaryView('docs');
  sectionLabelElement.textContent = 'DOCS';
  documentPathElement.textContent = path;
  documentContentElement.innerHTML = '<p class="loading">Loading document…</p>';

  const response = await fetch(`/api/document?path=${encodeURIComponent(path)}`);
  if (!response.ok) throw new Error('Document could not be loaded.');
  const document = await response.json();

  setActiveDocument(path);
  documentPathElement.textContent = document.path;
  documentContentElement.innerHTML = document.html;
}

async function showDocs() {
  setPrimaryView('docs');
  sectionLabelElement.textContent = 'DOCS';

  if (selectedPath && repository.documents.includes(selectedPath)) {
    await showDocument(selectedPath);
    return;
  }

  const readme = repository.documents.find((path) => /^readme\.md$/i.test(path));
  const firstDocument = readme || repository.documents[0];
  if (firstDocument) await showDocument(firstDocument);
  else {
    documentPathElement.textContent = 'docs';
    documentContentElement.innerHTML = '<p class="loading">This repository has no Markdown documents yet.</p>';
  }
}

async function loadTree() {
  const response = await fetch('/api/tree');
  if (!response.ok) throw new Error('Repository tree could not be loaded.');
  repository = await response.json();

  projectNameElement.textContent = repository.rootName;
  documentCountElement.textContent = repository.documents.length;
  treeElement.innerHTML = renderTree(repository.tree);
}

primaryNavigation.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-view]');
  if (!button) return;

  try {
    if (button.dataset.view === 'home') showHome();
    if (button.dataset.view === 'docs') await showDocs();
  } catch (error) {
    documentContentElement.innerHTML = `<p class="error">${escapeHtml(error.message)}</p>`;
  }
});

treeElement.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-path]');
  if (!button) return;

  try {
    await showDocument(button.dataset.path);
  } catch (error) {
    documentContentElement.innerHTML = `<p class="error">${escapeHtml(error.message)}</p>`;
  }
});

loadTree().catch((error) => {
  treeElement.innerHTML = `<p class="error">${escapeHtml(error.message)}</p>`;
});
