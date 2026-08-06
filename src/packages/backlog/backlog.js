function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#039;' }[character]));
}

const statuses = ['recently-done', 'in-progress', 'is-ready', 'in-planning'];

export default function createBacklog({ fetchFn = fetch } = {}) {
  let root;
  let items = [];
  let selectedPath = null;
  let list;
  let content;
  let pathLabel;

  function showError(error) {
    content.innerHTML = '<p class="backlog-error"></p>';
    content.querySelector('.backlog-error').textContent = error?.message || String(error);
  }

  function renderItems() {
    const groups = statuses.map((status) => {
      const decisions = items.filter((item) => item.status === status);
      return decisions.length ? `<section class="backlog-group"><h3>${escapeHtml(status)}</h3>${decisions.map((item) => `<button type="button" class="backlog-item${item.path === selectedPath ? ' active' : ''}" data-path="${escapeHtml(item.path)}"><span class="backlog-priority">${escapeHtml(item.priority)}</span><span>${escapeHtml(item.title)}</span></button>`).join('')}</section>` : '';
    }).join('');
    list.innerHTML = groups || '<p class="backlog-empty">No decisions found.</p>';
  }

  async function showPlan(itemPath) {
    content.innerHTML = '<p class="backlog-loading">Loading plan…</p>';
    const response = await fetchFn(`/api/backlog/plan?path=${encodeURIComponent(itemPath)}`);
    if (!response.ok) throw new Error('Backlog plan could not be loaded.');
    const plan = await response.json();
    selectedPath = plan.path;
    pathLabel.textContent = plan.path;
    renderItems();
    content.innerHTML = plan.html;
  }

  async function loadItems() {
    list.innerHTML = '<p class="backlog-loading">Loading decisions…</p>';
    const response = await fetchFn('/api/backlog/items');
    if (!response.ok) throw new Error('Backlog decisions could not be loaded.');
    ({ items } = await response.json());
    renderItems();
    const selected = items.find((item) => item.path === selectedPath) || items.find((item) => item.status !== 'recently-done') || items[0];
    if (selected) await showPlan(selected.path);
    else {
      selectedPath = null;
      pathLabel.textContent = 'backlog';
      content.innerHTML = '<p class="backlog-empty">No linked plans are available.</p>';
    }
  }

  return {
    mount(mountRoot) {
      root = mountRoot;
      root.innerHTML = '<section class="backlog-workspace" aria-label="Backlog"><aside class="backlog-list"><p class="eyebrow">BACKLOG / DECISIONS</p><h2>Decisions</h2><nav class="backlog-items" aria-label="Decisions"></nav></aside><article class="backlog-plan"><header><span>PLAN</span><span class="backlog-rule" aria-hidden="true"></span><span class="backlog-path">backlog</span></header><div class="backlog-content" aria-live="polite"></div></article></section>';
      list = root.querySelector('.backlog-items');
      content = root.querySelector('.backlog-content');
      pathLabel = root.querySelector('.backlog-path');
      list.addEventListener('click', async (event) => {
        const button = event.target.closest('[data-path]');
        if (!button || !list.contains(button)) return;
        try { await showPlan(button.dataset.path); }
        catch (error) { showError(error); }
      });
    },
    async activate() {
      root.hidden = false;
      try { await loadItems(); }
      catch (error) { showError(error); throw error; }
    },
    deactivate() { root.hidden = true; },
  };
}
