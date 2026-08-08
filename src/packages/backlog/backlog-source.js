import createAgentPanel from '../../ui/agent-panel.js';
import createCollapsibleSection from '../../ui/collapsible-section.js';

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#039;' }[character]));
}

const statuses = ['recently-done', 'in-progress', 'is-ready', 'in-planning'];
const priorities = ['P0', 'P1', 'P2', 'P3'];

export default function createBacklog({ fetchFn = fetch, eventSourceFactory = (url) => typeof EventSource === 'function' ? new EventSource(url) : null } = {}) {
  let root;
  let items = [];
  let selectedPath = null;
  let list;
  let content;
  let pathLabel;
  let workspace;
  let agentPanel;
  let agentUi;
  let agentToggle;
  let transcript;
  let statusLabel;
  let promptInput;
  let sendButton;
  let credentialPanel;
  let credentialInput;
  let credentialForm;
  let retryButton;
  let confirmationPanel;
  let confirmButton;
  let eventSource = null;
  let active = false;
  let planRequest = 0;
  let refreshRequested = 0;
  let refreshCompleted = 0;
  let refreshRunning = false;
  let pendingPrompt = null;
  let lastPrompt = null;
  let pendingConfirmation = null;
  let credentialRequired = false;
  let retryVisible = false;
  let agentVisible = true;
  const collapsedGroups = new Set();
  let chatState = { messages: [], status: 'idle', error: null, pendingDeletion: null };
  agentUi = createAgentPanel({
    prefix: 'backlog',
    label: 'AGENT / CHAT',
    ariaLabel: 'Backlog agent',
    placeholder: 'Ask about this decision…',
    onSend: (prompt) => submitPrompt(prompt),
    onReset: () => reset(),
    onRetry: () => { if (lastPrompt) return submitPrompt(lastPrompt); },
    onCredential: (key) => saveCredentialValue(key),
    onError: showError,
  });

  function setAgentVisible(show) {
    agentVisible = show;
    agentUi.setVisible(show);
    workspace.classList.toggle('backlog-agent-hidden', !show);
    agentToggle.setAttribute('aria-expanded', String(show));
    const label = show ? 'Hide agent panel' : 'Show agent panel';
    agentToggle.setAttribute('aria-label', label);
    agentToggle.title = label;
  }
  function showError(error) {
    content.innerHTML = '<p class="backlog-error"></p>';
    content.querySelector('.backlog-error').textContent = error?.message || String(error);
  }
  function renderItems() {
    list.replaceChildren();
    for (const status of statuses) {
      const decisions = items.filter((item) => item.status === status);
      if (!decisions.length) continue;
      const section = createCollapsibleSection({ documentRoot: list.ownerDocument, id: status, label: status, collapsed: collapsedGroups.has(status), sectionClass: 'backlog-group', toggleClass: 'backlog-group-toggle', itemsClass: 'backlog-group-items', indicatorClass: 'backlog-group-indicator', itemsId: `backlog-group-${status}` });
      section.toggle.dataset.backlogGroupToggle = status;
      for (const item of decisions) {
        const button = list.ownerDocument.createElement('button');
        button.type = 'button'; button.className = `backlog-item${item.path === selectedPath ? ' active' : ''}`; button.dataset.path = item.path;
        const priority = list.ownerDocument.createElement('span'); priority.className = 'backlog-priority'; priority.textContent = item.priority;
        button.append(priority, list.ownerDocument.createTextNode(item.title)); section.items.append(button);
      }
      section.toggle.addEventListener('click', () => { if (section.collapsed) collapsedGroups.add(status); else collapsedGroups.delete(status); renderItems(); });
      list.append(section.element);
    }
    if (!list.children.length) list.innerHTML = '<p class="backlog-empty">No decisions found.</p>';
  }
  function renderTranscript() {
    agentUi.update({ messages: chatState.messages, status: chatState.status, error: chatState.error, canSend: () => Boolean(selectedPath && agentUi.prompt.trim()), credentialRequired, retryVisible });
    if (chatState.pendingDeletion) showConfirmation(chatState.pendingDeletion);
  }
  function showCredential(show = true) {
    credentialRequired = show;
    agentUi.update({ credentialRequired });
    if (show) agentUi.focusCredential();
  }
  function showConfirmation(confirmation) {
    pendingConfirmation = confirmation;
    confirmationPanel.hidden = false;
    confirmationPanel.querySelector('.backlog-confirmation-title').textContent = `Delete “${confirmation.title}”?`;
    confirmButton.disabled = chatState.status === 'working';
  }
  function hideConfirmation() {
    pendingConfirmation = null;
    confirmationPanel.hidden = true;
  }
  function applySnapshot(snapshot, replaceMessages = false) {
    const incoming = Array.isArray(snapshot.messages) ? snapshot.messages : [];
    const messages = replaceMessages ? incoming : [...chatState.messages];
    if (!replaceMessages) {
      for (const message of incoming) {
        const index = messages.findIndex((item) => item.id === message.id);
        if (index < 0) messages.push(message);
        else messages[index] = message;
      }
    }
    chatState = { messages, status: snapshot.status || 'idle', error: snapshot.error || null, pendingDeletion: snapshot.pendingDeletion || null };
    if (!chatState.pendingDeletion) hideConfirmation();
    renderTranscript();
  }
  function applyMessage(message) {
    const index = chatState.messages.findIndex((item) => item.id === message.id);
    if (index < 0) chatState.messages = [...chatState.messages, message];
    else chatState.messages = chatState.messages.map((item, itemIndex) => itemIndex === index ? message : item);
    renderTranscript();
  }
  function handleEvent(event) {
    const value = event?.data ? JSON.parse(event.data) : event;
    if (!value || typeof value.type !== 'string') return;
    if (value.type === 'snapshot') applySnapshot(value.snapshot);
    else if (value.type === 'message') applyMessage(value.message);
    else if (value.type === 'status') { chatState.status = value.status; renderTranscript(); }
    else if (value.type === 'error') { chatState.error = value.message; retryVisible = Boolean(lastPrompt); renderTranscript(); }
    else if (value.type === 'credential-required') showCredential(true);
    else if (value.type === 'deletion-confirmation') showConfirmation(value.confirmation);
    else if (value.type === 'mutation-committed') {
      chatState.error = null;
      chatState.messages = [...chatState.messages, { id: `mutation-${value.revision}`, role: 'assistant', content: `Committed ${value.affectedPaths.join(', ')}.` }];
      renderTranscript();
      queueRefresh(value.revision);
    } else if (value.type === 'done') renderTranscript();
  }
  function connectEvents() {
    if (eventSource || !active) return;
    eventSource = eventSourceFactory('/api/backlog/agent/events');
    if (!eventSource) return;
    eventSource.onmessage = handleEvent;
    eventSource.onerror = () => { if (active) statusLabel.textContent = 'Connection interrupted'; };
  }
  function closeEvents() {
    if (!eventSource) return;
    eventSource.close();
    eventSource = null;
  }
  function renderMetadata(plan) {
    const option = (value, selected) => `<option value="${escapeHtml(value)}"${value === selected ? ' selected' : ''}>${escapeHtml(value)}</option>`;
    return `<div class="backlog-metadata" aria-label="Decision metadata"><label>Priority<select class="backlog-metadata-priority" data-metadata-field="priority" aria-label="Priority">${priorities.map((priority) => option(priority, plan.priority)).join('')}</select></label><label>Status<select class="backlog-metadata-status" data-metadata-field="status" aria-label="Status">${statuses.map((status) => option(status, plan.status)).join('')}</select></label></div>`;
  }
  async function updateMetadata(field, value) {
    const path = selectedPath;
    if (!path) return;
    const response = await fetchFn('/api/backlog/metadata', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ path, [field]: value }) });
    if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Decision metadata could not be updated.');
    if (selectedPath === path) await loadItems();
  }
  async function showPlan(itemPath) {
    const request = ++planRequest;
    content.innerHTML = '<p class="backlog-loading">Loading plan…</p>';
    const response = await fetchFn(`/api/backlog/plan?path=${encodeURIComponent(itemPath)}`);
    if (!response.ok) throw new Error('Backlog plan could not be loaded.');
    const plan = await response.json();
    if (!active || request !== planRequest) return;
    selectedPath = plan.path;
    pathLabel.textContent = plan.path;
    renderItems();
    const selectedItem = items.find((item) => item.path === plan.path);
    const metadata = { ...selectedItem, ...plan, priority: plan.priority ?? selectedItem?.priority, status: plan.status ?? selectedItem?.status };
    content.innerHTML = `${renderMetadata(metadata)}<div class="backlog-plan-markdown">${plan.html}</div>`;
    renderTranscript();
  }
  async function loadItems() {
    const response = await fetchFn('/api/backlog/items');
    if (!response.ok) throw new Error('Backlog decisions could not be loaded.');
    const result = await response.json();
    if (!active) return;
    items = result.items || [];
    renderItems();
    const selected = items.find((item) => item.path === selectedPath) || items.find((item) => item.status !== 'recently-done') || items[0];
    if (selected) await showPlan(selected.path);
    else { selectedPath = null; pathLabel.textContent = 'backlog'; content.innerHTML = '<p class="backlog-empty">No linked plans are available.</p>'; renderTranscript(); }
  }
  function queueRefresh(revision) {
    refreshRequested = Math.max(refreshRequested, Number(revision) || refreshRequested + 1);
    if (refreshRunning) return;
    refreshRunning = true;
    void (async () => {
      try {
        while (active && refreshCompleted < refreshRequested) {
          const target = refreshRequested;
          await loadItems();
          refreshCompleted = target;
        }
      } catch (error) { if (active) showError(error); }
      finally { refreshRunning = false; if (active && refreshCompleted < refreshRequested) queueRefresh(refreshRequested); }
    })();
  }
  async function submitPrompt(prompt = agentUi.prompt) {
    const contentValue = prompt.trim();
    if (!contentValue || !selectedPath || chatState.status === 'working') return;
    pendingPrompt = contentValue;
    lastPrompt = contentValue;
    const response = await fetchFn('/api/backlog/agent/prompt', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: contentValue, selectedPath }) });
    if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Prompt could not be submitted.');
    const result = await response.json();
    if (result.credentialRequired) showCredential(true);
    else { agentUi.clearPrompt(); pendingPrompt = null; retryVisible = false; }
    renderTranscript();
  }
  async function saveCredentialValue(key) {
    const response = await fetchFn('/api/backlog/agent/credential', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey: key }) });
    if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Credential could not be saved.');
    showCredential(false);
    retryVisible = true;
  }
  async function confirmDeletion() {
    if (!pendingConfirmation) return;
    const response = await fetchFn('/api/backlog/agent/confirm-deletion', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ id: pendingConfirmation.id }) });
    if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Deletion could not be confirmed.');
    hideConfirmation();
  }
  async function reset() {
    const response = await fetchFn('/api/backlog/agent/reset', { method: 'POST' });
    if (!response.ok) throw new Error('Chat could not be reset.');
    pendingPrompt = null;
    lastPrompt = null;
    retryVisible = false;
    agentUi.clearPrompt();
    applySnapshot((await response.json()).state, true);
    hideConfirmation();
    showCredential(false);
  }

  return {
    mount(mountRoot) {
      root = mountRoot;
      root.innerHTML = '<section class="backlog-workspace" aria-label="Backlog"><aside class="backlog-list"><p class="eyebrow">WORKSPACE</p><h2>Backlog</h2><nav class="backlog-items" aria-label="Decisions"></nav></aside><article class="backlog-plan"><header><span>PLAN</span><span class="backlog-rule" aria-hidden="true"></span><span class="backlog-path">backlog</span><button type="button" class="backlog-agent-toggle" aria-controls="backlog-agent-panel" aria-expanded="true" aria-label="Hide agent panel" title="Hide agent panel"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5.5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z"></path></svg></button></header><div class="backlog-content" aria-live="polite"></div></article><div class="backlog-agent-slot"></div></section>';
      workspace = root.querySelector('.backlog-workspace'); list = root.querySelector('.backlog-items'); content = root.querySelector('.backlog-content'); pathLabel = root.querySelector('.backlog-path'); agentUi.mount(root.querySelector('.backlog-agent-slot')); agentPanel = agentUi.root; agentToggle = root.querySelector('.backlog-agent-toggle'); transcript = root.querySelector('.backlog-transcript'); statusLabel = root.querySelector('.backlog-status'); promptInput = root.querySelector('.backlog-composer textarea'); sendButton = root.querySelector('.backlog-composer button[type="submit"]'); credentialPanel = root.querySelector('.backlog-credential'); credentialInput = credentialPanel.querySelector('input'); retryButton = root.querySelector('.backlog-retry'); agentUi.auxiliary.innerHTML = '<div class="backlog-confirmation" hidden><p class="backlog-confirmation-title"></p><button type="button" class="backlog-confirm-delete">Confirm deletion</button></div>'; confirmationPanel = root.querySelector('.backlog-confirmation'); confirmButton = root.querySelector('.backlog-confirm-delete');
      setAgentVisible(agentVisible);
      list.addEventListener('click', async (event) => {
        const toggle = event.target.closest('[data-backlog-group-toggle]');
        if (toggle && list.contains(toggle)) {
          const status = toggle.dataset.backlogGroupToggle;
          if (collapsedGroups.has(status)) collapsedGroups.delete(status); else collapsedGroups.add(status);
          renderItems();
          return;
        }
        const button = event.target.closest('[data-path]');
        if (!button || !list.contains(button)) return;
        try { await showPlan(button.dataset.path); } catch (error) { showError(error); }
      });
      content.addEventListener('change', (event) => { const select = event.target.closest('[data-metadata-field]'); if (!select || !content.contains(select)) return; void updateMetadata(select.dataset.metadataField, select.value).catch(showError); });
      confirmButton.addEventListener('click', () => { void confirmDeletion().catch(showError); });
      agentToggle.addEventListener('click', () => setAgentVisible(!agentVisible));
      renderItems(); renderTranscript();
    },
    async activate() { active = true; root.hidden = false; connectEvents(); try { await loadItems(); } catch (error) { showError(error); throw error; } },
    deactivate() { active = false; planRequest += 1; closeEvents(); root.hidden = true; },
  };
}
