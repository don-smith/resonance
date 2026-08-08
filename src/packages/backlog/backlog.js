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
  let agentVisible = true;
  let chatState = { messages: [], status: 'idle', error: null, pendingDeletion: null };

  function setAgentVisible(show) {
    agentVisible = show;
    agentPanel.hidden = !show;
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
    const groups = statuses.map((status) => {
      const decisions = items.filter((item) => item.status === status);
      return decisions.length ? `<section class="backlog-group"><h3>${escapeHtml(status)}</h3>${decisions.map((item) => `<button type="button" class="backlog-item${item.path === selectedPath ? ' active' : ''}" data-path="${escapeHtml(item.path)}"><span class="backlog-priority">${escapeHtml(item.priority)}</span><span>${escapeHtml(item.title)}</span></button>`).join('')}</section>` : '';
    }).join('');
    list.innerHTML = groups || '<p class="backlog-empty">No decisions found.</p>';
  }
  function renderTranscript() {
    transcript.innerHTML = chatState.messages.map((message) => `<p class="backlog-message backlog-message-${message.role}"><strong>${message.role === 'user' ? 'You' : 'Agent'}</strong><span>${escapeHtml(message.content)}</span></p>`).join('') || '<p class="backlog-chat-empty">Ask about the selected decision.</p>';
    transcript.scrollTop = transcript.scrollHeight;
    const working = chatState.status === 'working';
    statusLabel.textContent = working ? 'Working…' : chatState.error ? 'Needs attention' : 'Ready';
    statusLabel.dataset.status = chatState.error ? 'error' : chatState.status;
    sendButton.disabled = working || !selectedPath || !promptInput.value.trim();
    if (chatState.error) {
      const error = document.createElement('p'); error.className = 'backlog-chat-error'; error.textContent = chatState.error; transcript.append(error);
    }
    if (chatState.pendingDeletion) showConfirmation(chatState.pendingDeletion);
  }
  function showCredential(show = true) {
    credentialPanel.hidden = !show;
    if (show) credentialInput.focus();
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
    else if (value.type === 'error') { chatState.error = value.message; retryButton.hidden = !lastPrompt; renderTranscript(); }
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
  async function submitPrompt(prompt = promptInput.value) {
    const contentValue = prompt.trim();
    if (!contentValue || !selectedPath || chatState.status === 'working') return;
    pendingPrompt = contentValue;
    lastPrompt = contentValue;
    const response = await fetchFn('/api/backlog/agent/prompt', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: contentValue, selectedPath }) });
    if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Prompt could not be submitted.');
    const result = await response.json();
    if (result.credentialRequired) showCredential(true);
    else { promptInput.value = ''; pendingPrompt = null; retryButton.hidden = true; }
    renderTranscript();
  }
  async function saveCredential(event) {
    event.preventDefault();
    const key = credentialInput.value;
    const response = await fetchFn('/api/backlog/agent/credential', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey: key }) });
    if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Credential could not be saved.');
    credentialInput.value = '';
    showCredential(false);
    retryButton.hidden = false;
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
    retryButton.hidden = true;
    promptInput.value = '';
    applySnapshot((await response.json()).state, true);
    hideConfirmation();
    showCredential(false);
  }

  return {
    mount(mountRoot) {
      root = mountRoot;
      root.innerHTML = '<section class="backlog-workspace" aria-label="Backlog"><aside class="backlog-list"><p class="eyebrow">BACKLOG / DECISIONS</p><h2>Decisions</h2><nav class="backlog-items" aria-label="Decisions"></nav></aside><article class="backlog-plan"><header><span>PLAN</span><span class="backlog-rule" aria-hidden="true"></span><span class="backlog-path">backlog</span><button type="button" class="backlog-agent-toggle" aria-controls="backlog-agent-panel" aria-expanded="true" aria-label="Hide agent panel" title="Hide agent panel"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5.5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z"></path></svg></button></header><div class="backlog-content" aria-live="polite"></div></article><aside id="backlog-agent-panel" class="backlog-agent" aria-label="Backlog agent"><header class="backlog-agent-header"><span class="eyebrow">AGENT / CHAT</span><p class="backlog-status" data-status="idle">Ready</p></header><div class="backlog-transcript" aria-live="polite"></div><div class="backlog-agent-state"><button type="button" class="backlog-retry" hidden>Retry</button></div><div class="backlog-credential" hidden><p>Enter a local provider API key to start the agent.</p><form><input type="password" autocomplete="off" aria-label="Provider API key"><button type="submit">Save key</button></form></div><div class="backlog-confirmation" hidden><p class="backlog-confirmation-title"></p><button type="button" class="backlog-confirm-delete">Confirm deletion</button></div><form class="backlog-composer"><textarea rows="3" aria-label="Message" placeholder="Ask about this decision…"></textarea><div class="backlog-composer-actions"><button type="button" class="backlog-reset">New Chat</button><button type="submit">Send</button></div></form></aside></section>';
      workspace = root.querySelector('.backlog-workspace'); list = root.querySelector('.backlog-items'); content = root.querySelector('.backlog-content'); pathLabel = root.querySelector('.backlog-path'); agentPanel = root.querySelector('.backlog-agent'); agentToggle = root.querySelector('.backlog-agent-toggle'); transcript = root.querySelector('.backlog-transcript'); statusLabel = root.querySelector('.backlog-status'); promptInput = root.querySelector('.backlog-composer textarea'); sendButton = root.querySelector('.backlog-composer button[type="submit"]'); credentialPanel = root.querySelector('.backlog-credential'); credentialForm = credentialPanel.querySelector('form'); credentialInput = credentialPanel.querySelector('input'); retryButton = root.querySelector('.backlog-retry'); confirmationPanel = root.querySelector('.backlog-confirmation'); confirmButton = root.querySelector('.backlog-confirm-delete');
      setAgentVisible(agentVisible);
      list.addEventListener('click', async (event) => { const button = event.target.closest('[data-path]'); if (!button || !list.contains(button)) return; try { await showPlan(button.dataset.path); } catch (error) { showError(error); } });
      content.addEventListener('change', (event) => { const select = event.target.closest('[data-metadata-field]'); if (!select || !content.contains(select)) return; void updateMetadata(select.dataset.metadataField, select.value).catch(showError); });
      root.querySelector('.backlog-composer').addEventListener('submit', (event) => { event.preventDefault(); void submitPrompt().catch(showError); });
      credentialForm.addEventListener('submit', (event) => { void saveCredential(event).catch(showError); });
      retryButton.addEventListener('click', () => { if (lastPrompt) void submitPrompt(lastPrompt).catch(showError); });
      confirmButton.addEventListener('click', () => { void confirmDeletion().catch(showError); });
      root.querySelector('.backlog-reset').addEventListener('click', () => { void reset().catch(showError); });
      agentToggle.addEventListener('click', () => setAgentVisible(!agentVisible));
      promptInput.addEventListener('input', renderTranscript);
      renderItems(); renderTranscript();
    },
    async activate() { active = true; root.hidden = false; connectEvents(); try { await loadItems(); } catch (error) { showError(error); throw error; } },
    deactivate() { active = false; planRequest += 1; closeEvents(); root.hidden = true; },
  };
}
