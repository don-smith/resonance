import test from 'node:test';
import assert from 'node:assert/strict';
import { getBrowserCommand, openBrowser } from '../src/browser.ts';

test('uses the platform default browser command', () => {
  assert.deepEqual(getBrowserCommand('darwin', 'http://127.0.0.1:4317'), {
    command: 'open',
    args: ['http://127.0.0.1:4317'],
  });
  assert.deepEqual(getBrowserCommand('linux', 'http://127.0.0.1:4317'), {
    command: 'xdg-open',
    args: ['http://127.0.0.1:4317'],
  });
  assert.deepEqual(getBrowserCommand('win32', 'http://127.0.0.1:4317'), {
    command: 'cmd',
    args: ['/c', 'start', '', 'http://127.0.0.1:4317'],
  });
});

test('launches the default browser without coupling server startup to the child process', () => {
  const calls = [];
  const child = { unref() { calls.push(['unref']); } };
  openBrowser('http://127.0.0.1:4317', {
    platform: 'linux',
    spawnProcess(command, args, options) {
      calls.push([command, args, options]);
      return child;
    },
  });

  assert.deepEqual(calls, [
    ['xdg-open', ['http://127.0.0.1:4317'], { detached: true, stdio: 'ignore' }],
    ['unref'],
  ]);
});
