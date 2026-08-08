import test from 'node:test';
import assert from 'node:assert/strict';
import { parseHTML } from 'linkedom';
import createAgentPanel from './agent-panel.js';

test('renders agent state and delegates composer actions', () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const calls: Array<{ type: string; value?: string }> = [];
  const panel = createAgentPanel({
    prefix: 'test-agent',
    label: 'Backlog agent',
    placeholder: 'Ask about this decision…',
    supportsStop: true,
    onSend: (value) => calls.push({ type: 'send', value }),
    onStop: () => calls.push({ type: 'stop' }),
    onReset: () => calls.push({ type: 'reset' }),
  });
  const mount = document.createElement('section');
  document.body.append(mount);
  panel.mount(mount);
  panel.update({
    messages: [{ id: '1', role: 'user', content: 'Review this' }, { id: '2', role: 'assistant', content: 'Looks good.' }],
    status: 'idle',
    canSend: true,
  });

  assert.equal(mount.querySelector('.test-agent-transcript')?.textContent, 'YouReview thisAgentLooks good.');
  assert.equal(mount.querySelector('.test-agent-status')?.textContent, 'Ready');
  assert.equal((mount.querySelector('textarea') as HTMLTextAreaElement).placeholder, 'Ask about this decision…');
  assert.equal((mount.querySelector('.test-agent-send') as HTMLButtonElement).textContent, 'Send');
  assert.equal((mount.querySelector('.test-agent-send') as HTMLButtonElement).disabled, false);

  const input = mount.querySelector('textarea') as HTMLTextAreaElement;
  input.value = '  Tell me more  ';
  mount.querySelector('.test-agent-composer')?.dispatchEvent(new document.defaultView.Event('submit', { bubbles: true, cancelable: true }));
  (mount.querySelector('.test-agent-reset') as HTMLButtonElement).click();
  assert.deepEqual(calls, [{ type: 'send', value: 'Tell me more' }, { type: 'reset' }]);

  panel.update({ status: 'working', canSend: false, stopPending: false });
  assert.equal((mount.querySelector('.test-agent-send') as HTMLButtonElement).textContent, 'Stop');
  (mount.querySelector('.test-agent-send') as HTMLButtonElement).click();
  assert.deepEqual(calls.at(-1), { type: 'stop' });
});

test('supports credential, retry, context, auxiliary, and visibility slots', () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const calls: string[] = [];
  const panel = createAgentPanel({
    prefix: 'test-agent',
    onCredential: (value) => calls.push(`credential:${value}`),
    onRetry: () => calls.push('retry'),
  });
  const mount = document.createElement('section');
  document.body.append(mount);
  panel.mount(mount);
  const extra = document.createElement('p'); extra.textContent = 'Confirmation'; panel.auxiliary.append(extra);
  panel.update({ status: 'error', error: 'Needs attention', credentialRequired: true, retryVisible: true, contextUsage: '2k / 8k' });

  assert.equal((mount.querySelector('.test-agent-credential') as HTMLElement).hidden, false);
  assert.equal((mount.querySelector('.test-agent-retry') as HTMLButtonElement).hidden, false);
  assert.equal(mount.querySelector('.test-agent-context')?.textContent, '2k / 8k');
  assert.match(mount.querySelector('.test-agent-auxiliary')?.textContent || '', /Confirmation/);
  assert.match(mount.querySelector('.test-agent-transcript')?.textContent || '', /Needs attention/);
  (mount.querySelector('.test-agent-retry') as HTMLButtonElement).click();
  const credentialInput = mount.querySelector('.test-agent-credential input') as HTMLInputElement;
  credentialInput.value = 'local-key';
  mount.querySelector('.test-agent-credential form')?.dispatchEvent(new document.defaultView.Event('submit', { bubbles: true, cancelable: true }));
  assert.deepEqual(calls, ['retry', 'credential:local-key']);

  panel.setVisible(false);
  assert.equal((mount.querySelector('.test-agent-panel') as HTMLElement).hidden, true);
  panel.setVisible(true);
  assert.equal((mount.querySelector('.test-agent-panel') as HTMLElement).hidden, false);
});
