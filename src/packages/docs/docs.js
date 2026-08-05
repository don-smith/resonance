function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({
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

export default function createDocsPackage({ fetchFn = fetch } = {}) {
  let root;
  let repository = { documents: [] };
  let selectedPath = null;
  let treeElement;
  let contentElement;
  let projectNameElement;
  let countElement;
  let pathElement;

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
    treeElement.innerHTML = renderTree(repository.tree);

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
