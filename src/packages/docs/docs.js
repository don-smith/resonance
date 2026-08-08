function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#039;',
  }[character]));
}

const COLLAPSED_FOLDERS_STORAGE_PREFIX = 'resonance:docs:collapsed-folders:';

function resolveDocumentLink(href, currentPath, documents) {
  if (!href || !currentPath || href.startsWith('/') || href.startsWith('#')) return null;
  let target;
  try { target = new URL(href, `https://resonance.local/${currentPath}`); }
  catch { return null; }
  if (target.origin !== 'https://resonance.local' || target.search || target.hash) return null;
  let documentPath;
  try { documentPath = decodeURIComponent(target.pathname.slice(1)); }
  catch { return null; }
  return documents.includes(documentPath) ? documentPath : null;
}

function getStorage() {
  try { return typeof window !== 'undefined' ? window.localStorage || null : null; }
  catch { return null; }
}

function readCollapsedFolders(storage, key) {
  if (!storage) return new Set();
  try {
    const value = JSON.parse(storage.getItem(key) || '[]');
    return new Set(Array.isArray(value) ? value.filter((folder) => typeof folder === 'string') : []);
  } catch { return new Set(); }
}

function writeCollapsedFolders(storage, key, folders) {
  if (!storage || !key) return;
  try { storage.setItem(key, JSON.stringify([...folders].sort())); }
  catch { /* Browser storage may be unavailable or full. */ }
}

function renderTree(nodes, parentPath = '', collapsedFolders = new Set()) {
  return nodes.map((node) => {
    if (node.type === 'folder') {
      const folderPath = parentPath ? `${parentPath}/${node.name}` : node.name;
      const open = collapsedFolders.has(folderPath) ? '' : ' open';
      return `<details class="tree-folder" data-folder-path="${escapeHtml(folderPath)}"${open}><summary>${escapeHtml(node.name)}</summary><div class="tree-children">${renderTree(node.children, folderPath, collapsedFolders)}</div></details>`;
    }
    return `<button class="tree-file" type="button" data-path="${escapeHtml(node.path)}">${escapeHtml(node.name)}</button>`;
  }).join('');
}

export default function createDocsPackage({ fetchFn = fetch } = {}) {
  let root;
  let repository = { documents: [] };
  let selectedPath = null;
  let treeElement;
  let contentElement;
  let projectNameElement;
  let countElement;
  let pathElement;
  let collapsedFolders = new Set();
  let collapsedFoldersStorage;
  let collapsedFoldersStorageKey;

  function showError(element, error) {
    element.innerHTML = '<p class="docs-error"></p>';
    element.querySelector('.docs-error').textContent = error?.message || String(error);
  }

  async function showDocument(path) {
    pathElement.textContent = path;
    contentElement.innerHTML = '<p class="docs-loading">Loading document…</p>';
    const response = await fetchFn(`/api/docs/document?path=${encodeURIComponent(path)}`);
    if (!response.ok) throw new Error('Document could not be loaded.');
    const documentData = await response.json();
    selectedPath = documentData.path;
    pathElement.textContent = documentData.path;
    root.querySelectorAll('.tree-file').forEach((button) => {
      button.classList.toggle('active', button.dataset.path === documentData.path);
    });
    contentElement.innerHTML = documentData.html;
  }

  async function loadTree() {
    treeElement.innerHTML = '<p class="docs-loading">Loading repository…</p>';
    contentElement.innerHTML = '<p class="docs-loading">Loading repository…</p>';
    const response = await fetchFn('/api/docs/tree');
    if (!response.ok) throw new Error('Repository tree could not be loaded.');
    repository = await response.json();
    projectNameElement.textContent = repository.rootName;
    countElement.textContent = repository.documents.length;
    collapsedFoldersStorage = getStorage();
    collapsedFoldersStorageKey = `${COLLAPSED_FOLDERS_STORAGE_PREFIX}${encodeURIComponent(repository.rootName)}`;
    collapsedFolders = readCollapsedFolders(collapsedFoldersStorage, collapsedFoldersStorageKey);
    treeElement.innerHTML = renderTree(repository.tree, '', collapsedFolders);
    const folderPaths = new Set([...treeElement.querySelectorAll('.tree-folder')].map((folder) => folder.dataset.folderPath));
    collapsedFolders = new Set([...collapsedFolders].filter((folder) => folderPaths.has(folder)));
    writeCollapsedFolders(collapsedFoldersStorage, collapsedFoldersStorageKey, collapsedFolders);
    treeElement.querySelectorAll('.tree-folder').forEach((folder) => {
      folder.addEventListener('toggle', () => {
        const folderPath = folder.dataset.folderPath;
        if (!folderPath) return;
        if (folder.hasAttribute('open')) collapsedFolders.delete(folderPath);
        else collapsedFolders.add(folderPath);
        writeCollapsedFolders(collapsedFoldersStorage, collapsedFoldersStorageKey, collapsedFolders);
      });
    });

    const remembered = selectedPath && repository.documents.includes(selectedPath) ? selectedPath : null;
    const first = remembered || repository.documents.find((path) => /^readme\.md$/i.test(path)) || repository.documents[0];
    if (first) await showDocument(first);
    else {
      selectedPath = null;
      pathElement.textContent = 'docs';
      contentElement.innerHTML = '<p class="docs-loading">This repository has no Markdown documents yet.</p>';
    }
  }

  return {
    mount(mountRoot) {
      root = mountRoot;
      root.innerHTML = `<div class="docs-layout"><aside class="document-sidebar"><div class="document-sidebar-head"><p class="eyebrow">DOCS / MARKDOWN</p><h2 class="project-name">Repository</h2><p class="document-count"><span class="count">—</span> documents</p></div><nav class="document-tree" aria-label="Markdown document tree"><p class="docs-loading">Loading repository…</p></nav></aside><article class="document-pane"><header class="document-header"><span class="section-label">DOCS</span><span class="header-rule" aria-hidden="true"></span><span class="document-path">docs</span></header><div class="document-content" aria-live="polite"><p class="docs-loading">Loading repository…</p></div></article></div>`;
      treeElement = root.querySelector('.document-tree');
      contentElement = root.querySelector('.document-content');
      projectNameElement = root.querySelector('.project-name');
      countElement = root.querySelector('.count');
      pathElement = root.querySelector('.document-path');
      contentElement.addEventListener('click', async (event) => {
        const link = event.target?.closest?.('a[href]');
        if (!link || !contentElement.contains(link)) return;
        const documentPath = resolveDocumentLink(link.getAttribute('href'), selectedPath, repository.documents);
        if (!documentPath) return;
        event.preventDefault();
        try { await showDocument(documentPath); }
        catch (error) { showError(contentElement, error); }
      });
      treeElement.addEventListener('click', async (event) => {
        const button = event.target.closest('[data-path]');
        if (!button || !treeElement.contains(button)) return;
        try {
          await showDocument(button.dataset.path);
        } catch (error) {
          showError(contentElement, error);
        }
      });
    },
    async activate() {
      root.hidden = false;
      try {
        await loadTree();
      } catch (error) {
        showError(treeElement, error);
        showError(contentElement, error);
        throw error;
      }
    },
    deactivate() {
      root.hidden = true;
    },
  };
}
