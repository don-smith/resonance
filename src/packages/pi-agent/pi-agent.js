function emptyState() {
  return { messages: [], status: 'idle', hasSession: false, error: null };
}

function visibleMessages(messages) {
  return Array.isArray(messages) ? messages.filter((message) => message.role !== 'activity') : [];
}

function createMessageElement(documentRoot, message) {
  const item = documentRoot.createElement('article');
  item.className = `pi-agent-message pi-agent-message-${message.role}`;
  item.dataset.messageId = message.id;
  const role = documentRoot.createElement('span');
  role.className = 'pi-agent-message-role';
  role.textContent = message.role === 'activity' ? 'Activity' : message.role === 'user' ? 'You' : 'Pi';
  const content = documentRoot.createElement('p');
  content.className = 'pi-agent-message-content';
  content.textContent = message.content;
  item.append(role, content);
  return item;
}

export default function createPiAgentPackage({ fetchFn = fetch, eventSourceFactory = (url) => new EventSource(url) } = {}) {
  let root;
  let eventSource = null;
  let active = false;
  let pending = false;
  let state = emptyState();
  let history;
  let statusElement;
  let waitElement;
  let errorElement;
  let retryButton;
  let form;
  let input;
  let submitButton;

  function renderMessages() {
    history.replaceChildren(...state.messages.map((message) => createMessageElement(root.ownerDocument, message)));
    history.scrollTop = history.scrollHeight;
  }

  function render() {
    if (!root) return;
    renderMessages();
    const statusText = state.status === 'working' ? 'Pi is working…' : state.status === 'error' ? 'Pi Agent needs attention.' : 'Ready';
    statusElement.textContent = statusText;
    waitElement.hidden = state.status !== 'working';
    root.dataset.status = state.status;
    root.setAttribute('aria-busy', state.status === 'working' ? 'true' : 'false');
    errorElement.textContent = state.error || '';
    errorElement.hidden = !state.error;
    retryButton.hidden = !state.error;
    submitButton.disabled = pending;
  }

  function closeEventSource() {
    if (!eventSource) return;
    eventSource.onmessage = null;
    eventSource.onerror = null;
    eventSource.close();
    eventSource = null;
  }

  function showError(error) {
    state = { ...state, status: 'error', error: error?.message || String(error) };
    render();
  }

  function upsertMessage(message) {
    if (message.role === 'activity') return;
    const index = state.messages.findIndex((item) => item.id === message.id);
    const messages = [...state.messages];
    if (index === -1) messages.push({ ...message });
    else messages[index] = { ...messages[index], ...message };
    state = { ...state, messages };
  }

  function handleEvent(event) {
    if (!event || typeof event !== 'object') return;
    if (event.type === 'snapshot') {
      state = { ...emptyState(), ...(event.snapshot || {}), messages: visibleMessages(event.snapshot?.messages) };
    } else if (event.type === 'message') {
      upsertMessage(event.message);
    } else if (event.type === 'activity') {
      return;
    } else if (event.type === 'status') {
      state = { ...state, status: event.status, error: event.status === 'error' ? state.error : null };
    } else if (event.type === 'error') {
      state = { ...state, status: 'error', error: event.message };
    } else if (event.type === 'done') {
      state = { ...state, status: 'idle' };
    } else return;
    render();
  }

  function connectEventSource() {
    closeEventSource();
    eventSource = eventSourceFactory('/api/pi-agent/events');
    eventSource.onmessage = (messageEvent) => {
      try { handleEvent(JSON.parse(messageEvent.data)); }
      catch (error) { showError(new Error(`Pi Agent stream returned invalid data: ${error.message || error}`)); }
    };
    eventSource.onerror = () => {
      closeEventSource();
      if (active) showError(new Error('Pi Agent connection was lost. Retry to reconnect.'));
    };
  }

  function handleInputKeydown(event) {
    if (event.key === 'Enter' && event.shiftKey) submitPrompt(event);
  }

  async function submitPrompt(event) {
    event.preventDefault();
    const prompt = input.value.trim();
    if (!prompt || pending) return;
    pending = true;
    state = { ...state, status: 'working', error: null };
    render();
    try {
      const response = await fetchFn('/api/pi-agent/prompt', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ prompt }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => ({}));
        throw new Error(body.error || 'Prompt could not be submitted.');
      }
      input.value = '';
    } catch (error) {
      showError(error);
    } finally {
      pending = false;
      render();
    }
  }

  async function resetSession(event) {
    event.preventDefault();
    if (pending) return;
    pending = true;
    render();
    try {
      const response = await fetchFn('/api/pi-agent/reset', { method: 'POST' });
      if (!response.ok) {
        const body = await response.json().catch(() => ({}));
        throw new Error(body.error || 'New Pi Agent session could not be started.');
      }
      const body = await response.json();
      state = { ...emptyState(), ...(body.state || {}), messages: visibleMessages(body.state?.messages) };
      input.value = '';
      render();
      input.focus();
    } catch (error) {
      showError(error);
    } finally {
      pending = false;
      render();
    }
  }

  function retry(event) {
    event.preventDefault();
    // Keep the error state until the reconnected stream supplies an authoritative snapshot.
    state = { ...state, error: null };
    render();
    if (active) connectEventSource();
  }

  return {
    mount(mountRoot) {
      root = mountRoot;
      root.innerHTML = `<section class="pi-agent-workspace" aria-label="Pi Agent"><header class="pi-agent-header"><div><p class="eyebrow">PI AGENT / PI ACP</p><h2>Work with Pi</h2></div><button class="pi-agent-new" type="button">New Session</button></header><div class="pi-agent-status" role="status" aria-live="polite"><span class="pi-agent-status-text">Ready</span><span class="pi-agent-wait" aria-hidden="true" hidden></span></div><div class="pi-agent-error" role="alert" hidden></div><button class="pi-agent-retry" type="button" hidden>Retry connection</button><div class="pi-agent-history" role="log" aria-live="polite" aria-relevant="additions text" tabindex="0"></div><form class="pi-agent-composer"><label for="pi-agent-prompt">Prompt Pi</label><textarea id="pi-agent-prompt" rows="3" placeholder="Ask Pi about this repository…"></textarea><div class="pi-agent-composer-actions"><span class="pi-agent-hint">Pi runs locally from this repository.</span><button type="submit">Send prompt</button></div></form></section>`;
      history = root.querySelector('.pi-agent-history');
      statusElement = root.querySelector('.pi-agent-status-text');
      waitElement = root.querySelector('.pi-agent-wait');
      errorElement = root.querySelector('.pi-agent-error');
      retryButton = root.querySelector('.pi-agent-retry');
      form = root.querySelector('.pi-agent-composer');
      input = root.querySelector('#pi-agent-prompt');
      submitButton = form.querySelector('button[type="submit"]');
      input.addEventListener('keydown', handleInputKeydown);
      form.addEventListener('submit', submitPrompt);
      root.querySelector('.pi-agent-new').addEventListener('click', resetSession);
      retryButton.addEventListener('click', retry);
      render();
    },
    async activate() {
      active = true;
      root.hidden = false;
      if (!eventSource) connectEventSource();
      render();
    },
    deactivate() {
      active = false;
      closeEventSource();
      root.hidden = true;
    },
  };
}
