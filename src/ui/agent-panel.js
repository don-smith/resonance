function element(documentRoot, name, text, attributes = {}) {
  const node = documentRoot.createElement(name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
  if (text !== undefined) node.textContent = text;
  return node;
}

function defaultMessage(documentRoot, message, prefix) {
  const item = element(documentRoot, 'p', undefined, { class: `${prefix}-message ${prefix}-message-${message.role}` });
  item.append(element(documentRoot, 'strong', message.role === 'user' ? 'You' : 'Agent'), element(documentRoot, 'span', message.content));
  return item;
}

function defaultTranscript(transcript, messages, prefix, renderMessage) {
  transcript.replaceChildren();
  for (const message of messages || []) transcript.append(renderMessage(transcript.ownerDocument, message));
  if (!messages?.length) transcript.append(element(transcript.ownerDocument, 'p', 'Ask about the selected item.', { class: `${prefix}-chat-empty` }));
}

export default function createAgentPanel({
  prefix = 'agent',
  label = 'Agent / Chat',
  ariaLabel = 'Agent',
  placeholder = 'Message…',
  supportsStop = false,
  contextClass = `${prefix}-context`,
  renderMessage = (documentRoot, message) => defaultMessage(documentRoot, message, prefix),
  renderTranscript = (transcriptRoot, messages) => defaultTranscript(transcriptRoot, messages, prefix, renderMessage),
  onSend = () => undefined,
  onStop = () => undefined,
  onReset = () => undefined,
  onRetry = () => undefined,
  onCredential = () => undefined,
  onError = (error) => console.error(error),
} = {}) {
  let panel;
  let transcript;
  let statusLabel;
  let sendButton;
  let promptInput;
  let contextUsage;
  let credentialPanel;
  let credentialInput;
  let retryButton;
  let state = { messages: [], status: 'idle', error: null, canSend: false, stopPending: false, credentialRequired: false, retryVisible: false, contextUsage: '' };

  function run(action, ...args) {
    try { return Promise.resolve(action(...args)).catch(onError); }
    catch (error) { onError(error); return Promise.resolve(); }
  }
  function renderMessages() {
    renderTranscript(transcript, state.messages || [], state);
    if (state.error) transcript.append(element(transcript.ownerDocument, 'p', state.error, { class: `${prefix}-chat-error` }));
    transcript.scrollTop = transcript.scrollHeight;
  }
  function renderState() {
    const working = state.status === 'working';
    const stopping = working && state.stopPending;
    statusLabel.textContent = working ? (stopping ? 'Stopping…' : 'Working…') : state.error ? 'Needs attention' : 'Ready';
    statusLabel.dataset.status = state.error ? 'error' : state.status;
    contextUsage.textContent = state.contextUsage || '';
    credentialPanel.hidden = !state.credentialRequired;
    retryButton.hidden = !state.retryVisible;
    if (working && supportsStop) {
      sendButton.type = 'button';
      sendButton.textContent = stopping ? 'Stopping…' : 'Stop';
      sendButton.classList.toggle(`${prefix}-stop`, true);
      sendButton.disabled = stopping;
    } else {
      sendButton.type = 'submit';
      sendButton.textContent = 'Send';
      sendButton.classList.toggle(`${prefix}-stop`, false);
      const canSend = typeof state.canSend === 'function' ? state.canSend(promptInput.value, state) : state.canSend;
      sendButton.disabled = working || !canSend;
    }
    renderMessages();
  }
  function mount(mountRoot) {
    const documentRoot = mountRoot.ownerDocument || document;
    mountRoot.classList.add(`${prefix}-host`, 'resonance-agent-host');
    mountRoot.innerHTML = `<aside class="${prefix}-agent ${prefix}-panel resonance-agent-panel" aria-label="${ariaLabel}"><header class="${prefix}-agent-header ${prefix}-header resonance-agent-header"><span class="eyebrow">${label}</span><p class="${prefix}-status resonance-agent-status" data-status="idle">Ready</p></header><div class="${prefix}-transcript resonance-agent-transcript" aria-live="polite"></div><div class="${prefix}-agent-state ${prefix}-state resonance-agent-state"><button type="button" class="${prefix}-retry resonance-agent-retry" hidden>Retry</button></div><div class="${prefix}-credential resonance-agent-credential" hidden><p>Enter a local provider API key to start the agent.</p><form><input type="password" autocomplete="off" aria-label="Provider API key"><button type="submit">Save key</button></form></div><div class="${prefix}-auxiliary resonance-agent-auxiliary"></div><form class="${prefix}-composer resonance-agent-composer"><textarea rows="3" aria-label="Message" placeholder="${placeholder}"></textarea><div class="${prefix}-composer-actions resonance-agent-composer-actions"><button type="submit" class="${prefix}-send resonance-agent-send">Send</button><span class="${contextClass} resonance-agent-context" aria-live="polite"></span><button type="button" class="${prefix}-reset resonance-agent-reset">New Chat</button></div></form></aside>`;
    panel = mountRoot.querySelector(`.${prefix}-panel`);
    transcript = mountRoot.querySelector(`.${prefix}-transcript`);
    statusLabel = mountRoot.querySelector(`.${prefix}-status`);
    sendButton = mountRoot.querySelector(`.${prefix}-send`);
    promptInput = mountRoot.querySelector(`.${prefix}-composer textarea`);
    contextUsage = mountRoot.querySelector(`.${contextClass}`);
    credentialPanel = mountRoot.querySelector(`.${prefix}-credential`);
    credentialInput = credentialPanel.querySelector('input');
    retryButton = mountRoot.querySelector(`.${prefix}-retry`);
    mountRoot.querySelector(`.${prefix}-composer`).addEventListener('submit', (event) => {
      event.preventDefault();
      if (state.status === 'working') return;
      const prompt = promptInput.value.trim();
      if (prompt && state.canSend) void run(onSend, prompt);
    });
    sendButton.addEventListener('click', (event) => {
      if (state.status !== 'working' || !supportsStop) return;
      event.preventDefault();
      void run(onStop);
    });
    mountRoot.querySelector(`.${prefix}-reset`).addEventListener('click', () => { void run(onReset); });
    retryButton.addEventListener('click', () => { void run(onRetry); });
    credentialPanel.querySelector('form').addEventListener('submit', (event) => {
      event.preventDefault();
      void run(onCredential, credentialInput.value).then(() => { credentialInput.value = ''; });
    });
    promptInput.addEventListener('input', () => renderState());
    renderState();
    return panel;
  }

  return {
    mount,
    update(nextState = {}) { state = { ...state, ...nextState }; if (panel) renderState(); },
    setVisible(show) { if (panel) panel.hidden = !show; },
    clearPrompt() { if (promptInput) promptInput.value = ''; },
    focusCredential() { credentialInput?.focus(); },
    get prompt() { return promptInput?.value || ''; },
    get auxiliary() { return panel?.querySelector(`.${prefix}-auxiliary`); },
    get root() { return panel; },
  };
}
