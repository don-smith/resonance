import { createLikeC4Renderer } from './architecture-likec4.tsx';

function escapeId(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, '-'); }
function formatTokenCount(value) { if (value >= 1000000) { const rounded = Math.floor(value / 100000) / 10; return `${rounded % 1 === 0 ? rounded : rounded.toFixed(1)}M`; } if (value >= 1000) return `${Math.floor(value / 1000)}k`; return String(Math.floor(value)); }
function formatContextUsage(context) { return context ? `${formatTokenCount(context.inputTokens)} / ${formatTokenCount(context.maxInputTokens)}` : ''; }
function element(name, text, attributes = {}) { const node = document.createElement(name); for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value)); if (text !== undefined) node.textContent = text; return node; }

export default function createArchitecture({ fetchFn = fetch, eventSourceFactory = (url) => typeof EventSource === 'function' ? new EventSource(url) : null } = {}) {
  let root; let likec4Dump; let views = []; let activeView = 'systemContext'; let selectedId = ''; let graph; let diagramRenderer; let active = false; let eventSource = null;
  let workspace; let agentPanel; let agentToggle; let validationButton; let transcript; let statusLabel; let promptInput; let sendButton; let contextUsage; let credentialPanel; let credentialInput; let retryButton;
  let lastPrompt = null; let agentVisible = true; let validationResults = null;
  let chatState = { messages: [], status: 'idle', error: null, context: null };

  async function json(url, options) { const response = await fetchFn(url, options); if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Architecture data could not be loaded.'); return response.json(); }
  function showError(error) { root.textContent = ''; root.append(element('article', error?.message || String(error), { class: 'architecture-error', role: 'alert' })); }
  function setAgentVisible(show) { agentVisible = show; if (!agentPanel) return; agentPanel.hidden = !show; workspace.classList.toggle('architecture-agent-hidden', !show); agentToggle.setAttribute('aria-expanded', String(show)); agentToggle.setAttribute('aria-label', show ? 'Hide agent panel' : 'Show agent panel'); agentToggle.title = show ? 'Hide agent panel' : 'Show agent panel'; }
  const navigationGroups = [{ id: 'containers', label: 'Containers' }, { id: 'components', label: 'Components' }, { id: 'code', label: 'Code' }];
  const collapsedNavigationGroups = new Set();
  function updateViewHeader(viewId) { const view = views.find((candidate) => candidate.id === viewId); const title = root.querySelector('.architecture-view-title'); if (title) title.textContent = view?.name || viewId; }
  function navigationGroupFor(view) { const id = String(view.id).toLowerCase(); if (id.includes('container')) return 'containers'; if (id.includes('component')) return 'components'; if (id.includes('code')) return 'code'; return 'components'; }
  function isSystemContextView(view) { return view.id === 'systemContext' || view.id === 'system-context'; }
  function renderNavigation(nav) {
    nav.textContent = '';
    function appendViewTo(parent, view) { const button = element('button', view.name, { type: 'button', class: 'architecture-nav-view', 'data-view-id': view.id }); if (view.id === activeView) { button.classList.add('active'); button.setAttribute('aria-current', 'page'); } button.addEventListener('click', async () => { activeView = view.id; updateViewHeader(view.id); try { await refreshWorkspace(); } catch (error) { renderGraphError(error); } }); parent.append(button); }
    const specialViews = [views.find((candidate) => candidate.id === 'validation'), views.find(isSystemContextView)];
    for (const view of specialViews.filter(Boolean)) appendViewTo(nav, view);
    const remainingViews = views.filter((candidate) => candidate.id !== 'validation' && !isSystemContextView(candidate));
    for (const group of navigationGroups) {
      const section = element('section', undefined, { class: 'architecture-nav-group', 'data-nav-group': group.id });
      const collapsed = collapsedNavigationGroups.has(group.id);
      const itemsId = `architecture-nav-group-${group.id}`;
      const toggle = element('button', undefined, { type: 'button', class: 'architecture-nav-group-toggle', 'aria-expanded': String(!collapsed), 'aria-controls': itemsId });
      toggle.append(element('span', group.label), element('span', collapsed ? '▸' : '▾', { class: 'architecture-nav-group-indicator', 'aria-hidden': 'true' }));
      const items = element('div', undefined, { id: itemsId, class: 'architecture-nav-group-items' });
      items.hidden = collapsed;
      for (const view of remainingViews.filter((candidate) => navigationGroupFor(candidate) === group.id)) appendViewTo(items, view);
      toggle.addEventListener('click', () => { if (collapsedNavigationGroups.has(group.id)) collapsedNavigationGroups.delete(group.id); else collapsedNavigationGroups.add(group.id); renderNavigation(nav); });
      section.append(toggle, items); nav.append(section);
    }
  }

  function renderGraph(view) {
    const panel = root.querySelector('.architecture-graph');
    if (diagramRenderer) { diagramRenderer.unmount(); diagramRenderer = null; }
    panel.textContent = '';
    if (likec4Dump) {
      diagramRenderer = createLikeC4Renderer({ root: panel, dump: likec4Dump, viewId: activeView, onNodeClick: (node) => { selectedId = node.modelRef || node.id; }, onNavigate: async (viewId) => { activeView = viewId; updateViewHeader(viewId); await refreshWorkspace(); } });
      return;
    }
    const svg = element('svg', undefined, { class: 'architecture-svg', viewBox: '0 0 900 620', role: 'img', 'aria-label': `${view.name} architecture graph` });
    const defs = element('defs'); const marker = element('marker', undefined, { id: 'architecture-arrow', markerWidth: 8, markerHeight: 8, refX: 7, refY: 4, orient: 'auto' }); marker.append(element('path', undefined, { d: 'M0,0 L8,4 L0,8 z' })); defs.append(marker); svg.append(defs);
    const layout = view.presentation?.layout || {}; const point = new Map(graph.nodes.map((node, index) => [node.id, layout[node.id] || { x: 80 + (index % 4) * 210, y: 70 + Math.floor(index / 4) * 140 }])); const group = element('g');
    for (const edge of graph.edges) { const from = point.get(edge.source); const to = point.get(edge.target); if (!from || !to) continue; group.append(element('line', undefined, { x1: from.x + 82, y1: from.y + 28, x2: to.x + 82, y2: to.y + 28, class: `architecture-edge edge-${escapeId(edge.type)}` })); }
    for (const node of graph.nodes) { const location = point.get(node.id); const item = element('g', undefined, { class: `architecture-node${selectedId === node.id ? ' selected' : ''}`, tabindex: '0', role: 'button', 'aria-label': `${node.name}, ${node.c4?.level || node.type}`, 'data-entity-id': node.id, transform: `translate(${location.x},${location.y})` }); item.append(element('rect', undefined, { width: 164, height: 58, rx: 8 }), element('text', node.name, { x: 12, y: 23 }), element('text', node.c4?.level || node.type, { x: 12, y: 43, class: 'architecture-node-type' })); const select = () => { selectedId = node.id; renderGraph(view); }; item.addEventListener('click', select); item.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(); } }); group.append(item); }
    svg.append(group); panel.append(svg);
  }
  function renderGraphError(error) {
    if (diagramRenderer) { diagramRenderer.unmount(); diagramRenderer = null; }
    const panel = root.querySelector('.architecture-graph');
    panel.textContent = '';
    const notice = element('article', undefined, { class: 'architecture-error', role: 'alert' });
    notice.append(element('h2', 'Architecture model unavailable'), element('p', error?.message || String(error)), element('p', 'Use the Architecture agent to inspect and repair the LikeC4 source, then reload this view.'));
    panel.append(notice);
  }
  function renderValidationResults(panel, results) { panel.textContent = ''; const list = element('ul'); for (const result of results) { const item = element('li', undefined, { class: `validation-${result.status}` }); item.append(element('strong', result.status.toUpperCase()), element('span', ` ${result.name}: ${result.message}`)); list.append(item); } panel.append(list); }
  function renderValidation() {
    if (diagramRenderer) { diagramRenderer.unmount(); diagramRenderer = null; }
    const panel = root.querySelector('.architecture-graph'); panel.textContent = '';
    const view = element('section', undefined, { class: 'architecture-validation-view' });
    const toolbar = element('div', undefined, { class: 'architecture-validation-toolbar' }); validationButton = element('button', 'Run validation', { type: 'button', class: 'architecture-validation-button' }); toolbar.append(validationButton); view.append(toolbar);
    const results = element('div', undefined, { class: 'architecture-validation', 'aria-live': 'polite' }); if (validationResults) renderValidationResults(results, validationResults); else results.append(element('p', 'Run validation to evaluate the current architecture model against its authored rules.')); view.append(results); panel.append(view);
    validationButton.addEventListener('click', () => { void runValidation(); });
  }
  async function runValidation() {
    const button = validationButton; button.disabled = true;
    try { validationResults = (await json('/api/architecture/validation')).results; if (activeView === 'validation') renderValidation(); }
    catch (error) { renderGraphError(error); }
    finally { button.disabled = false; if (validationButton !== button) validationButton.disabled = false; }
  }
  async function loadGraph() { if (activeView === 'validation') { renderValidation(); root.querySelectorAll('[data-view-id]').forEach((button) => button.classList.toggle('active', button.dataset.viewId === activeView)); return; } graph = await json(`/api/architecture/graph?view=${encodeURIComponent(activeView)}`); const view = views.find((candidate) => candidate.id === activeView) || graph.view; renderGraph(view); root.querySelectorAll('[data-view-id]').forEach((button) => button.classList.toggle('active', button.dataset.viewId === activeView)); }
  async function refreshWorkspace() { if (!active) return; const modelResponse = await json('/api/architecture/model'); if (modelResponse.likec4Error) throw new Error(modelResponse.likec4Error); likec4Dump = modelResponse.likec4; if (modelResponse.likec4Views) { views = [...modelResponse.likec4Views.filter((view) => view.id !== 'index'), { id: 'validation', name: 'Validation', type: 'validation', description: 'Run and inspect deterministic architecture validation.' }]; activeView = views.some((view) => view.id === activeView) ? activeView : views[0]?.id; } render(); await loadGraph(); }
  function renderTranscript() {
    if (!transcript) return;
    transcript.textContent = '';
    let assistantGroup;
    let assistantContent;
    for (const message of chatState.messages) {
      if (message.role === 'user') {
        assistantGroup = null;
        assistantContent = null;
        const item = element('p', undefined, { class: 'architecture-message architecture-message-user' });
        item.append(element('strong', 'You'), element('span', message.content));
        transcript.append(item);
        continue;
      }
      if (!assistantGroup) {
        assistantGroup = element('div', undefined, { class: 'architecture-message architecture-message-assistant' });
        assistantContent = element('div', undefined, { class: 'architecture-message-content' });
        assistantGroup.append(element('strong', 'Agent'), assistantContent);
        transcript.append(assistantGroup);
      }
      assistantContent.append(element('p', message.content));
    }
    if (!chatState.messages.length) transcript.append(element('p', 'Ask about this C4 view or selected entity.', { class: 'architecture-chat-empty' }));
    transcript.scrollTop = transcript.scrollHeight;
    const working = chatState.status === 'working'; statusLabel.textContent = working ? 'Working…' : chatState.error ? 'Needs attention' : 'Ready'; statusLabel.dataset.status = chatState.error ? 'error' : chatState.status; contextUsage.textContent = formatContextUsage(chatState.context); sendButton.disabled = working || !promptInput.value.trim();
    if (chatState.error) transcript.append(element('p', chatState.error, { class: 'architecture-chat-error' }));
  }
  function showCredential(show = true) { credentialPanel.hidden = !show; if (show) credentialInput.focus(); }
  function applySnapshot(snapshot, replaceMessages = false) { chatState = { messages: replaceMessages ? snapshot.messages || [] : (snapshot.messages?.length ? snapshot.messages : chatState.messages), status: snapshot.status || 'idle', error: snapshot.error || null, context: snapshot.context === undefined ? chatState.context : snapshot.context }; renderTranscript(); }
  function handleEvent(event) { let value; try { value = event?.data ? JSON.parse(event.data) : event; } catch { return; } if (!value || typeof value.type !== 'string') return; if (value.type === 'snapshot') applySnapshot(value.snapshot); else if (value.type === 'message') { const index = chatState.messages.findIndex((message) => message.id === value.message.id); if (index < 0) chatState.messages = [...chatState.messages, value.message]; else chatState.messages[index] = value.message; renderTranscript(); } else if (value.type === 'status') { chatState.status = value.status; renderTranscript(); } else if (value.type === 'context') { chatState.context = value.context; renderTranscript(); } else if (value.type === 'error') { chatState.error = value.message; retryButton.hidden = !lastPrompt; renderTranscript(); } else if (value.type === 'credential-required') showCredential(true); else if (value.type === 'done') { renderTranscript(); void refreshWorkspace().catch(renderGraphError); } }
  function connectEvents() { if (eventSource || !active) return; eventSource = eventSourceFactory('/api/architecture/agent/events'); if (!eventSource) return; eventSource.onmessage = handleEvent; eventSource.onerror = () => { if (active) statusLabel.textContent = 'Connection interrupted'; }; }
  function closeEvents() { if (eventSource) eventSource.close(); eventSource = null; }
  async function submitPrompt(prompt = promptInput.value) { const value = prompt.trim(); if (!value || chatState.status === 'working') return; lastPrompt = value; const response = await fetchFn('/api/architecture/agent/prompt', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: value, selectedId, selectedView: activeView }) }); if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Prompt could not be submitted.'); const result = await response.json(); if (result.credentialRequired) showCredential(true); else { promptInput.value = ''; retryButton.hidden = true; } renderTranscript(); }
  async function saveCredential(event) { event.preventDefault(); const response = await fetchFn('/api/architecture/agent/credential', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey: credentialInput.value }) }); if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Credential could not be saved.'); credentialInput.value = ''; showCredential(false); if (lastPrompt) retryButton.hidden = false; }
  async function reset() { await json('/api/architecture/agent/reset', { method: 'POST' }); lastPrompt = null; promptInput.value = ''; retryButton.hidden = true; showCredential(false); chatState = { messages: [], status: 'idle', error: null, context: null }; renderTranscript(); }
  function render() {
    const prompt = promptInput?.value || '';
    root.textContent = ''; root.classList.add('architecture-mount');
    root.innerHTML = '<section class="architecture-workspace" aria-label="Architecture"><aside class="architecture-navigator"><p class="architecture-eyebrow">RESONANCE</p><h1>Architecture</h1><nav aria-label="Architecture views"></nav></aside><main class="architecture-center"><header class="architecture-header"><span class="architecture-header-label">C4</span><span class="architecture-header-rule" aria-hidden="true"></span><span class="architecture-view-title"></span><div class="architecture-header-actions"><button type="button" class="architecture-agent-toggle" aria-controls="architecture-agent-panel" aria-expanded="true" aria-label="Hide agent panel" title="Hide agent panel"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5.5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z"></path></svg></button></div></header><div class="architecture-graph"></div></main><aside id="architecture-agent-panel" class="architecture-agent" aria-label="Architecture agent"><header class="architecture-agent-header"><span>AGENT / CHAT</span><p class="architecture-status" data-status="idle">Ready</p></header><div class="architecture-transcript" aria-live="polite"></div><div class="architecture-credential" hidden><p>Enter a local provider API key to start the agent.</p><form><input type="password" autocomplete="off" aria-label="Provider API key"><button type="submit">Save key</button></form></div><form class="architecture-composer"><textarea rows="3" aria-label="Message" placeholder="Ask about this C4 view…"></textarea><div><button type="button" class="architecture-reset">New Chat</button><span class="architecture-context-usage" aria-live="polite" title="Latest input context / maximum input context"></span><button type="submit">Send</button></div></form></aside></section>';
    workspace = root.querySelector('.architecture-workspace'); agentPanel = root.querySelector('.architecture-agent'); agentToggle = root.querySelector('.architecture-agent-toggle'); transcript = root.querySelector('.architecture-transcript'); statusLabel = root.querySelector('.architecture-status'); promptInput = root.querySelector('.architecture-composer textarea'); sendButton = root.querySelector('.architecture-composer button[type="submit"]'); contextUsage = root.querySelector('.architecture-context-usage'); credentialPanel = root.querySelector('.architecture-credential'); credentialInput = credentialPanel.querySelector('input'); retryButton = element('button', 'Retry', { type: 'button', class: 'architecture-retry', hidden: 'true' }); root.querySelector('.architecture-agent-header').append(retryButton);
    const nav = root.querySelector('.architecture-navigator nav'); renderNavigation(nav);
    const toggleAgent = () => setAgentVisible(!agentVisible); agentToggle.addEventListener('click', toggleAgent);
    root.querySelector('.architecture-composer').addEventListener('submit', (event) => { event.preventDefault(); void submitPrompt().catch(showError); }); credentialPanel.querySelector('form').addEventListener('submit', (event) => { void saveCredential(event).catch(showError); }); retryButton.addEventListener('click', () => { if (lastPrompt) void submitPrompt(lastPrompt).catch(showError); }); root.querySelector('.architecture-reset').addEventListener('click', () => { void reset().catch(showError); }); promptInput.addEventListener('input', renderTranscript);
    promptInput.value = prompt;
    updateViewHeader(activeView); setAgentVisible(agentVisible); renderTranscript();
  }
  return { mount(mountRoot) { root = mountRoot; root.hidden = true; root.classList.add('architecture-mount'); }, async activate() { active = true; root.hidden = false; connectEvents(); try { let modelResponse = null; let modelError = null; try { modelResponse = await json('/api/architecture/model'); } catch (error) { modelError = error; } const [viewsResponse, state] = await Promise.all([json('/api/architecture/views'), json('/api/architecture/agent/state')]); likec4Dump = modelResponse?.likec4; views = [...(modelResponse?.likec4Views || viewsResponse.views).filter((view) => view.id !== 'index'), { id: 'validation', name: 'Validation', type: 'validation', description: 'Run and inspect deterministic architecture validation.' }]; activeView = views.some((view) => view.id === activeView) ? activeView : views[0]?.id; applySnapshot(state, true); render(); const modelSourceError = modelError || (modelResponse?.likec4Error ? new Error(modelResponse.likec4Error) : null); if (modelSourceError) renderGraphError(modelSourceError); else { try { await loadGraph(); } catch (error) { renderGraphError(error); } } } catch (error) { showError(error); throw error; } }, deactivate() { active = false; closeEvents(); if (diagramRenderer) { diagramRenderer.unmount(); diagramRenderer = null; } root.hidden = true; } };
}
