import { spawn } from 'node:child_process';
import process from 'node:process';

export function getBrowserCommand(platform, url) {
  if (platform === 'darwin') return { command: 'open', args: [url] };
  if (platform === 'win32') return { command: 'cmd', args: ['/c', 'start', '', url] };
  return { command: 'xdg-open', args: [url] };
}

export function openBrowser(url, { platform = process.platform, spawnProcess = spawn } = {}) {
  const { command, args } = getBrowserCommand(platform, url);
  const child = spawnProcess(command, args, { detached: true, stdio: 'ignore' });
  child.once?.('error', (error) => {
    console.error(`theview: unable to open browser: ${error.message}`);
  });
  child.unref?.();
  return child;
}
