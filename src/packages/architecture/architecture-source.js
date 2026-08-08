import createAgentPanel from '../../ui/agent-panel.js';
import createCollapsibleSection from '../../ui/collapsible-section.js';
import { createLikeC4Renderer } from './architecture-likec4.tsx';

function escapeId(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, '-'); }
function formatTokenCount(value) { if (value >= 1000000) { const rounded = Math.floor(value / 100000) / 10; return `${rounded % 1 === 0 ? rounded : rounded.toFixed(1)}M`; } if (value >= 1000) return `${Math.floor(value / 1000)}k`; return String(Math.floor(value)); }
function formatContextUsage(context) { return context ? `${formatTokenCount(context.inputTokens)} / ${formatTokenCount(context.maxInputTokens)}` : ''; }
function element(name, text, attributes = {}) { const node = document.createElement(name); for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value)); if (text !== undefined) node.textContent = text; return node; }
function getStorage() { try { return typeof window !== 'undefined' ? window.localStorage || null : null; } catch { return null; } }
function readStringSet(storage, key) { if (!storage) return new Set(); try { const value = JSON.parse(storage.getItem(key) || '[]'); return new Set(Array.isArray(value) ? value.filter((item) => typeof item === 'string') : []); } catch { return new Set(); } }
function writeStringSet(storage, key, values) { if (!storage) return; try { storage.setItem(key, JSON.stringify([...values].sort())); } catch { /* Browser storage may be unavailable or full. */ } }
function readBoolean(storage, key) { if (!storage) return false; try { return storage.getItem(key) === 'true'; } catch { return false; } }
function writeBoolean(storage, key, value) { if (!storage) return; try { storage.setItem(key, String(value)); } catch { /* Browser storage may be unavailable or full. */ } }
const COLLAPSED_NAVIGATION_STORAGE_KEY = 'resonance:architecture:collapsed-navigation-groups';
const RELATIONSHIPS_COLLAPSED_STORAGE_KEY = 'resonance:architecture:relationships-collapsed';

export default function createArchitecture({ fetchFn = fetch, eventSourceFactory = (url) => typeof EventSource === 'function' ? new EventSource(url) : null } = {}) {
  let root; let likec4Dump; let architectureModel = { entities: [], relationships: [] }; let views = []; let activeView = 'systemContext'; let selectedId = ''; let selectedNode = null; let graph; let diagramRenderer; let active = false; let eventSource = null;
  let workspace; let agentPanel; let agentUi; let agentToggle; let validationButton; let transcript; let statusLabel; let promptInput; let sendButton; let contextUsage; let credentialPanel; let credentialInput; let retryButton;
  let lastPrompt = null; let agentVisible = true; let validationResults = null; let stopPending = false; let credentialRequired = false; let retryVisible = false;
  let chatState = { messages: [], status: 'idle', error: null, context: null };
  let storage;
  let relationshipsCollapsed = false;
  agentUi = createAgentPanel({
    prefix: 'architecture',
    label: 'AGENT / CHAT',
    ariaLabel: 'Architecture agent',
    placeholder: 'Ask about this C4 view…',
    supportsStop: true,
    contextClass: 'architecture-context-usage',
    renderTranscript: renderAgentTranscript,
    onSend: (prompt) => submitPrompt(prompt),
    onStop: () => stop(),
    onReset: () => reset(),
    onRetry: () => { if (lastPrompt) return submitPrompt(lastPrompt); },
    onCredential: (key) => saveCredentialValue(key),
    onError: showError,
  });

  async function json(url, options) { const response = await fetchFn(url, options); if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Architecture data could not be loaded.'); return response.json(); }
  function showError(error) { root.textContent = ''; root.append(element('article', error?.message || String(error), { class: 'architecture-error', role: 'alert' })); }
  function setAgentVisible(show) { agentVisible = show; if (!agentPanel) return; agentUi.setVisible(show); workspace.classList.toggle('architecture-agent-hidden', !show); agentToggle.setAttribute('aria-expanded', String(show)); agentToggle.setAttribute('aria-label', show ? 'Hide agent panel' : 'Show agent panel'); agentToggle.title = show ? 'Hide agent panel' : 'Show agent panel'; }
  const navigationGroups = [{ id: 'containers', label: 'Containers' }, { id: 'components', label: 'Components' }, { id: 'code', label: 'Code' }, { id: 'dynamics', label: 'Dynamics' }, { id: 'deployment', label: 'Deployment' }];
  const collapsedNavigationGroups = new Set();
  function viewBreadcrumbs(viewId) {
    const chain = []; const visited = new Set(); let view = views.find((candidate) => candidate.id === viewId);
    while (view && !visited.has(view.id)) { chain.unshift(view); visited.add(view.id); view = views.find((candidate) => candidate.id === view.parentId); }
    return chain.filter((candidate, index) => index === chain.length - 1 || (!isSystemContextView(candidate) && candidate.id !== 'index'));
  }
  function updateViewHeader(viewId) {
    const breadcrumbs = root.querySelector('.architecture-breadcrumbs'); if (!breadcrumbs) return; breadcrumbs.textContent = '';
    const trail = viewBreadcrumbs(viewId); const currentView = views.find((candidate) => candidate.id === viewId); if (!trail.length && currentView) trail.push(currentView);
    trail.forEach((view, index) => {
      if (index) breadcrumbs.append(element('span', '/', { class: 'architecture-breadcrumb-separator', 'aria-hidden': 'true' }));
      if (view.id === viewId) breadcrumbs.append(element('span', view.name || view.id, { class: 'architecture-view-title', 'aria-current': 'page' }));
      else { const button = element('button', view.name || view.id, { type: 'button', class: 'architecture-breadcrumb', 'data-breadcrumb-view-id': view.id, 'aria-label': `Go to ${view.name || view.id}` }); button.addEventListener('click', () => { void navigateToView(view.id).catch(renderGraphError); }); breadcrumbs.append(button); }
    });
  }
  async function navigateToView(viewId) { activeView = viewId; updateViewHeader(viewId); await refreshWorkspace(); }
  function navigationGroupFor(view) { const type = String(view.type || '').toLowerCase(); const id = String(view.id).toLowerCase(); if (type === 'dynamic' || id.includes('dynamic')) return 'dynamics'; if (type === 'deployment' || id.includes('deployment')) return 'deployment'; if (id.includes('container')) return 'containers'; if (id.includes('component')) return 'components'; if (id.includes('code')) return 'code'; return 'components'; }
  function navigationLabelFor(view) { return (view.name || view.id).replace(/\s+(containers|components)$/i, ''); }
  function isLandscapeView(view) { return view.id === 'landscape'; }
  function isSystemContextView(view) { return view.id === 'systemContext' || view.id === 'system-context'; }
  function renderNavigation(nav) {
    nav.textContent = '';
    function appendViewTo(parent, view) { const button = element('button', navigationLabelFor(view), { type: 'button', class: 'architecture-nav-view', 'data-view-id': view.id }); if (view.id === activeView) { button.classList.add('active'); button.setAttribute('aria-current', 'page'); } button.addEventListener('click', () => { void navigateToView(view.id).catch(renderGraphError); }); parent.append(button); }
    const specialViews = [views.find((candidate) => candidate.id === 'validation'), views.find(isLandscapeView), views.find(isSystemContextView)];
    for (const view of specialViews.filter(Boolean)) appendViewTo(nav, view);
    const remainingViews = views.filter((candidate) => candidate.id !== 'validation' && !isLandscapeView(candidate) && !isSystemContextView(candidate));
    for (const group of navigationGroups) {
      const collapsed = collapsedNavigationGroups.has(group.id);
      const section = createCollapsibleSection({ documentRoot: nav.ownerDocument, id: group.id, label: group.label, collapsed, sectionClass: 'architecture-nav-group', toggleClass: 'architecture-nav-group-toggle', itemsClass: 'architecture-nav-group-items', indicatorClass: 'architecture-nav-group-indicator', itemsId: `architecture-nav-group-${group.id}` });
      section.element.dataset.navGroup = group.id;
      for (const view of remainingViews.filter((candidate) => navigationGroupFor(candidate) === group.id)) appendViewTo(section.items, view);
      section.toggle.addEventListener('click', () => { if (section.collapsed) collapsedNavigationGroups.add(group.id); else collapsedNavigationGroups.delete(group.id); writeStringSet(storage, COLLAPSED_NAVIGATION_STORAGE_KEY, collapsedNavigationGroups); renderNavigation(nav); });
      nav.append(section.element);
    }
  }

  function modelElement(id) { return likec4Dump?.elements?.[id] || null; }
  function descriptionText(value) { if (!value) return ''; if (typeof value === 'string') return value; return value.txt || value.md || ''; }
  function selectedMetadata(node) {
    const id = node?.modelRef || node?.id || selectedId;
    const exact = architectureModel.entities.find((entity) => entity.id === id);
    if (exact) return exact;
    const element = modelElement(id);
    const title = node?.title || element?.title;
    const leaf = String(id || '').split('.').pop();
    return architectureModel.entities.find((entity) => entity.id === leaf || entity.name === title) || null;
  }
  function selectedLinks(node, metadata, relationships = []) {
    const links = [];
    const add = (path, label, line) => { if (!path || links.some((link) => link.path === path)) return; links.push({ path, label, line }); };
    for (const evidence of metadata?.evidence || []) add(evidence.path, evidence.label, evidence.line);
    for (const relationship of relationships) for (const evidence of relationship.evidence || []) add(evidence.path, evidence.label, evidence.line);
    for (const link of [...(modelElement(node?.modelRef || node?.id)?.links || []), ...(node?.links || [])]) add(link.relative || link.url, link.title);
    return links;
  }
  function selectedRelationships(node, metadata) {
    const id = node?.modelRef || node?.id || selectedId;
    const relationships = [];
    const add = (source, target, title, type, evidence = []) => {
      if (!source || !target || relationships.some((item) => item.source === source && item.target === target && item.title === title)) return;
      relationships.push({ source, target, title: title || type || 'Relationship', type, evidence });
    };
    for (const relationship of Object.values(likec4Dump?.relations || {})) {
      const source = relationship.source?.model || relationship.source?.deployment || relationship.source;
      const target = relationship.target?.model || relationship.target?.deployment || relationship.target;
      if (source === id || target === id) add(source, target, relationship.title, relationship.kind);
    }
    const metadataId = metadata?.id;
    for (const relationship of architectureModel.relationships || []) {
      if (relationship.source === metadataId || relationship.target === metadataId) add(relationship.source, relationship.target, relationship.label, relationship.type, relationship.evidence || []);
    }
    return relationships;
  }
  function displayElementName(id) { return modelElement(id)?.title || architectureModel.entities.find((entity) => entity.id === id)?.name || String(id).split('.').pop() || id; }
  function renderEntityDetails() {
    const panel = root.querySelector('.architecture-details');
    if (!panel) return;
    panel.textContent = '';
    if (!selectedNode) { panel.hidden = true; return; }
    const id = selectedNode.modelRef || selectedNode.id || selectedId;
    const elementData = modelElement(id);
    const metadata = selectedMetadata(selectedNode);
    const name = selectedNode.title || elementData?.title || metadata?.name || id;
    const description = descriptionText(selectedNode.description) || descriptionText(elementData?.description) || metadata?.description || '';
    const technology = selectedNode.technology || elementData?.technology || metadata?.c4?.technology;
    const relationships = selectedRelationships(selectedNode, metadata);
    const links = selectedLinks(selectedNode, metadata, relationships);
    const header = element('header', undefined, { class: 'architecture-details-header' });
    header.append(element('div', 'SELECTED ENTITY', { class: 'architecture-details-eyebrow' }), element('h2', name));
    const close = element('button', '×', { type: 'button', class: 'architecture-details-close', 'aria-label': 'Close entity details' });
    close.addEventListener('click', () => { selectedNode = null; selectedId = ''; renderEntityDetails(); });
    header.append(close); panel.append(header);
    if (technology) panel.append(element('p', technology, { class: 'architecture-details-technology' }));
    if (description) panel.append(element('p', description, { class: 'architecture-details-description' }));
    const relationshipSection = element('section', undefined, { class: 'architecture-details-section' });
    const relationshipToggle = element('button', undefined, { type: 'button', class: 'architecture-details-section-toggle', 'aria-expanded': String(!relationshipsCollapsed), 'aria-controls': 'architecture-relationships' });
    relationshipToggle.append(element('span', `Relationships (${relationships.length})`), element('span', relationshipsCollapsed ? '▸' : '▾', { class: 'architecture-details-section-indicator', 'aria-hidden': 'true' }));
    const relationshipContent = element('div', undefined, { id: 'architecture-relationships', class: 'architecture-details-section-content' });
    relationshipContent.hidden = relationshipsCollapsed;
    if (!relationships.length) relationshipContent.append(element('p', 'No modeled relationships.', { class: 'architecture-details-empty' }));
    else { const list = element('ul'); for (const relationship of relationships) { const item = element('li'); const direction = relationship.source === id || relationship.source === metadata?.id ? '→' : '←'; item.append(element('strong', `${direction} ${relationship.title}`), element('span', `${displayElementName(relationship.source)} / ${displayElementName(relationship.target)}`)); list.append(item); } relationshipContent.append(list); }
    relationshipToggle.addEventListener('click', () => { relationshipsCollapsed = !relationshipsCollapsed; writeBoolean(storage, RELATIONSHIPS_COLLAPSED_STORAGE_KEY, relationshipsCollapsed); renderEntityDetails(); });
    relationshipSection.append(relationshipToggle, relationshipContent);
    panel.append(relationshipSection);
    const evidenceSection = element('section', undefined, { class: 'architecture-details-section' });
    evidenceSection.append(element('h3', `Linked evidence (${links.length})`));
    if (!links.length) evidenceSection.append(element('p', 'No linked evidence.', { class: 'architecture-details-empty' }));
    else { const list = element('ul'); for (const link of links) { const item = element('li'); item.append(element('span', link.label ? `${link.label}: ${link.path}` : link.path)); if (link.line) item.append(element('small', ` line ${link.line}`)); list.append(item); } evidenceSection.append(list); }
    panel.append(evidenceSection); panel.hidden = false;
  }
  function selectNode(node) { selectedNode = node; selectedId = node.modelRef || node.id; renderEntityDetails(); }
  function renderGraph(view) {
    const panel = root.querySelector('.architecture-graph');
    if (diagramRenderer) { diagramRenderer.unmount(); diagramRenderer = null; }
    panel.textContent = '';
    if (likec4Dump) {
      diagramRenderer = createLikeC4Renderer({ root: panel, dump: likec4Dump, viewId: activeView, onNodeClick: selectNode, onNavigate: navigateToView });
      return;
    }
    const svg = element('svg', undefined, { class: 'architecture-svg', viewBox: '0 0 900 620', role: 'img', 'aria-label': `${view.name} architecture graph` });
    const defs = element('defs'); const marker = element('marker', undefined, { id: 'architecture-arrow', markerWidth: 8, markerHeight: 8, refX: 7, refY: 4, orient: 'auto' }); marker.append(element('path', undefined, { d: 'M0,0 L8,4 L0,8 z' })); defs.append(marker); svg.append(defs);
    const layout = view.presentation?.layout || {}; const point = new Map(graph.nodes.map((node, index) => [node.id, layout[node.id] || { x: 80 + (index % 4) * 210, y: 70 + Math.floor(index / 4) * 140 }])); const group = element('g');
    for (const edge of graph.edges) { const from = point.get(edge.source); const to = point.get(edge.target); if (!from || !to) continue; group.append(element('line', undefined, { x1: from.x + 82, y1: from.y + 28, x2: to.x + 82, y2: to.y + 28, class: `architecture-edge edge-${escapeId(edge.type)}` })); }
    for (const node of graph.nodes) { const location = point.get(node.id); const item = element('g', undefined, { class: `architecture-node${selectedId === node.id ? ' selected' : ''}`, tabindex: '0', role: 'button', 'aria-label': `${node.name}, ${node.c4?.level || node.type}`, 'data-entity-id': node.id, transform: `translate(${location.x},${location.y})` }); item.append(element('rect', undefined, { width: 164, height: 58, rx: 8 }), element('text', node.name, { x: 12, y: 23 }), element('text', node.c4?.level || node.type, { x: 12, y: 43, class: 'architecture-node-type' })); const select = () => { selectNode(node); renderGraph(view); }; item.addEventListener('click', select); item.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(); } }); group.append(item); }
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
  async function refreshWorkspace() { if (!active) return; const modelResponse = await json('/api/architecture/model'); if (modelResponse.likec4Error) throw new Error(modelResponse.likec4Error); architectureModel = modelResponse.model || { entities: [], relationships: [] }; likec4Dump = modelResponse.likec4; if (modelResponse.likec4Views) { views = [...modelResponse.likec4Views.filter((view) => view.id !== 'index'), { id: 'validation', name: 'Validation', type: 'validation', description: 'Run and inspect deterministic architecture validation.' }]; activeView = views.some((view) => view.id === activeView) ? activeView : views[0]?.id; } render(); await loadGraph(); }
  function renderAgentTranscript(target, messages) {
    target.textContent = '';
    let assistantGroup;
    let assistantContent;
    for (const message of messages) {
      if (message.role === 'user') {
        assistantGroup = null;
        assistantContent = null;
        const item = element('p', undefined, { class: 'architecture-message architecture-message-user' });
        item.append(element('strong', 'You'), element('span', message.content));
        target.append(item);
        continue;
      }
      if (!assistantGroup) {
        assistantGroup = element('div', undefined, { class: 'architecture-message architecture-message-assistant' });
        assistantContent = element('div', undefined, { class: 'architecture-message-content' });
        assistantGroup.append(element('strong', 'Agent'), assistantContent);
        target.append(assistantGroup);
      }
      assistantContent.append(element('p', message.content));
    }
    if (!messages.length) target.append(element('p', 'Ask about this C4 view or selected entity.', { class: 'architecture-chat-empty' }));
  }
  function renderTranscript() {
    if (!agentUi) return;
    agentUi.update({ messages: chatState.messages, status: chatState.status, error: chatState.error, stopPending, credentialRequired, retryVisible, contextUsage: formatContextUsage(chatState.context), canSend: () => Boolean(agentUi.prompt.trim()) });
  }
  function showCredential(show = true) { credentialRequired = show; agentUi.update({ credentialRequired }); if (show) agentUi.focusCredential(); }
  function applySnapshot(snapshot, replaceMessages = false) { chatState = { messages: replaceMessages ? snapshot.messages || [] : (snapshot.messages?.length ? snapshot.messages : chatState.messages), status: snapshot.status || 'idle', error: snapshot.error || null, context: snapshot.context === undefined ? chatState.context : snapshot.context }; renderTranscript(); }
  function handleEvent(event) { let value; try { value = event?.data ? JSON.parse(event.data) : event; } catch { return; } if (!value || typeof value.type !== 'string') return; if (value.type === 'snapshot') applySnapshot(value.snapshot); else if (value.type === 'message') { const index = chatState.messages.findIndex((message) => message.id === value.message.id); if (index < 0) chatState.messages = [...chatState.messages, value.message]; else chatState.messages[index] = value.message; renderTranscript(); } else if (value.type === 'status') { chatState.status = value.status; renderTranscript(); } else if (value.type === 'context') { chatState.context = value.context; renderTranscript(); } else if (value.type === 'error') { chatState.error = value.message; retryVisible = Boolean(lastPrompt); renderTranscript(); } else if (value.type === 'credential-required') showCredential(true); else if (value.type === 'stopped' || value.type === 'done') { stopPending = false; renderTranscript(); void refreshWorkspace().catch(renderGraphError); } }
  function connectEvents() { if (eventSource || !active) return; eventSource = eventSourceFactory('/api/architecture/agent/events'); if (!eventSource) return; eventSource.onmessage = handleEvent; eventSource.onerror = () => { if (active) statusLabel.textContent = 'Connection interrupted'; }; }
  function closeEvents() { if (eventSource) eventSource.close(); eventSource = null; }
  async function submitPrompt(prompt = agentUi.prompt) { const value = prompt.trim(); if (!value || chatState.status === 'working') return; lastPrompt = value; const response = await fetchFn('/api/architecture/agent/prompt', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: value, selectedId, selectedView: activeView }) }); if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Prompt could not be submitted.'); const result = await response.json(); if (result.credentialRequired) showCredential(true); else { agentUi.clearPrompt(); retryVisible = false; } renderTranscript(); }
  async function stop() {
    if (chatState.status !== 'working' || stopPending) return;
    stopPending = true; renderTranscript();
    try {
      const result = await json('/api/architecture/agent/stop', { method: 'POST' });
      if (result.state) applySnapshot(result.state, true);
      await refreshWorkspace().catch(renderGraphError);
    } finally { stopPending = false; renderTranscript(); }
  }
  async function saveCredentialValue(apiKey) { const response = await fetchFn('/api/architecture/agent/credential', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey }) }); if (!response.ok) throw new Error((await response.json().catch(() => null))?.error || 'Credential could not be saved.'); showCredential(false); retryVisible = Boolean(lastPrompt); renderTranscript(); }
  async function reset() { await json('/api/architecture/agent/reset', { method: 'POST' }); lastPrompt = null; stopPending = false; retryVisible = false; agentUi.clearPrompt(); showCredential(false); chatState = { messages: [], status: 'idle', error: null, context: null }; renderTranscript(); }
  function render() {
    const prompt = promptInput?.value || '';
    root.textContent = ''; root.classList.add('architecture-mount');
    root.innerHTML = '<section class="architecture-workspace" aria-label="Architecture"><aside class="architecture-navigator"><p class="eyebrow">WORKSPACE</p><h1>Architecture</h1><nav aria-label="Architecture views"></nav></aside><main class="architecture-center"><header class="architecture-header"><span class="architecture-header-label">C4</span><span class="architecture-header-rule" aria-hidden="true"></span><nav class="architecture-breadcrumbs" aria-label="View hierarchy"></nav><div class="architecture-header-actions"><button type="button" class="architecture-agent-toggle" aria-controls="architecture-agent-panel" aria-expanded="true" aria-label="Hide agent panel" title="Hide agent panel"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5.5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z"></path></svg></button></div></header><div class="architecture-graph"></div><aside class="architecture-details" aria-label="Selected entity details" hidden></aside></main><div class="architecture-agent-slot"></div></section>';
    workspace = root.querySelector('.architecture-workspace'); agentUi.mount(root.querySelector('.architecture-agent-slot')); agentPanel = agentUi.root; agentToggle = root.querySelector('.architecture-agent-toggle'); transcript = root.querySelector('.architecture-transcript'); statusLabel = root.querySelector('.architecture-status'); promptInput = root.querySelector('.architecture-composer textarea'); sendButton = root.querySelector('.architecture-composer button[type="submit"]'); contextUsage = root.querySelector('.architecture-context-usage'); credentialPanel = root.querySelector('.architecture-credential'); credentialInput = credentialPanel.querySelector('input'); retryButton = root.querySelector('.architecture-retry');
    const nav = root.querySelector('.architecture-navigator nav'); renderNavigation(nav);
    const toggleAgent = () => setAgentVisible(!agentVisible); agentToggle.addEventListener('click', toggleAgent);
    promptInput.value = prompt;
    updateViewHeader(activeView); setAgentVisible(agentVisible); renderTranscript();
  }
  return { mount(mountRoot) { root = mountRoot; root.hidden = true; root.classList.add('architecture-mount'); }, async activate() { active = true; root.hidden = false; storage = getStorage(); collapsedNavigationGroups.clear(); for (const group of readStringSet(storage, COLLAPSED_NAVIGATION_STORAGE_KEY)) collapsedNavigationGroups.add(group); relationshipsCollapsed = readBoolean(storage, RELATIONSHIPS_COLLAPSED_STORAGE_KEY); connectEvents(); try { let modelResponse = null; let modelError = null; try { modelResponse = await json('/api/architecture/model'); } catch (error) { modelError = error; } architectureModel = modelResponse?.model || { entities: [], relationships: [] }; const [viewsResponse, state] = await Promise.all([json('/api/architecture/views'), json('/api/architecture/agent/state')]); likec4Dump = modelResponse?.likec4; views = [...(modelResponse?.likec4Views || viewsResponse.views).filter((view) => view.id !== 'index'), { id: 'validation', name: 'Validation', type: 'validation', description: 'Run and inspect deterministic architecture validation.' }]; activeView = views.some((view) => view.id === activeView) ? activeView : views[0]?.id; applySnapshot(state, true); render(); const modelSourceError = modelError || (modelResponse?.likec4Error ? new Error(modelResponse.likec4Error) : null); if (modelSourceError) renderGraphError(modelSourceError); else { try { await loadGraph(); } catch (error) { renderGraphError(error); } } } catch (error) { showError(error); throw error; } }, deactivate() { active = false; closeEvents(); if (diagramRenderer) { diagramRenderer.unmount(); diagramRenderer = null; } root.hidden = true; } };
}
