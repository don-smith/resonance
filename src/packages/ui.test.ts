import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const shellRoot = new URL('./shell/', import.meta.url);
const retiredTerm = new RegExp(['cock', 'pit'].join(''), 'i');

test('the Shell document contains only fixed Shell browser wiring', async () => {
  const html = await readFile(new URL('index.html', shellRoot), 'utf8');
  assert.match(html, /id="primary-navigation"/);
  assert.match(html, /aria-label="Workspaces"/);
  assert.match(html, /<p class="eyebrow">RESONANCE<\/p>/);
  assert.match(html, /<button class="repository-title" type="button" data-shell-repository-name data-shell-home disabled>resonance<\/button>/);
  assert.doesNotMatch(html, /<button class="repository-home"/);
  assert.doesNotMatch(html, /RESONANCE \/ WORKSPACE/);
  assert.doesNotMatch(html, /WORKSPACE 0\.1/);
  assert.match(html, /<span>v<span data-shell-runtime-version><\/span><\/span>/);
  assert.match(html, /data-shell-theme="light"[^>]*aria-label="Use light theme"/);
  assert.match(html, /data-shell-theme="dark"[^>]*aria-label="Use dark theme"/);
  assert.match(html, /data-shell-theme="system"[^>]*aria-label="Use system theme"/);
  assert.match(html, /assets\/shell\/theme-bootstrap\.js[\s\S]*assets\/shell\/shell\.css[\s\S]*assets\/shell\/ui\.css/);
  assert.match(html, /id="package-mount"/);
  assert.match(html, /assets\/shell\/shell\.css/);
  assert.match(html, /assets\/app\.js/);
  assert.doesNotMatch(html, /assets\/home\/home\.css/);
  assert.doesNotMatch(html, /assets\/docs\/docs\.css/);
  assert.doesNotMatch(html, retiredTerm);
});

test('the Shell keeps primary navigation fixed while the package area scrolls', async () => {
  const css = await readFile(new URL('./shell/styles.css', import.meta.url), 'utf8');
  assert.match(css, /\.repository-version \{[^}]*color: var\(--sidebar-ink\);/);
  assert.doesNotMatch(css, /\.repository-title \{[^}]*all: unset;/);
  assert.match(css, /\.repository-title \{[^}]*appearance: none;[^}]*margin: 0;[^}]*padding: 0;[^}]*border: 0;[^}]*background: transparent;[^}]*font: inherit;[^}]*line-height: inherit;/s);
  assert.match(css, /\.repository-title:not\(:disabled\) \{[^}]*cursor: pointer;/);
  assert.match(css, /\.nav-section-label \{[^}]*font: 600 11px/);
  assert.match(css, /:root\[data-theme="dark"\] \{[^}]*color-scheme: dark;[^}]*--ink: #ece8e1;[^}]*--paper: #181a1c;/s);
  assert.match(css, /--diagram-edge: #aaa59d;[^}]*--diagram-edge-contains: #bd5f37;/s);
  assert.match(css, /:root\[data-theme="dark"\] \{[^}]*--diagram-edge: #5a5e61;[^}]*--diagram-edge-contains: #e18a62;/s);
  assert.match(css, /\.primary-footer \{[^}]*justify-content: space-between;/);
  assert.match(css, /\.theme-selector button\[aria-pressed="true"\] \{[^}]*color: var\(--accent\);/);
  assert.doesNotMatch(css, /\.theme-selector button\[aria-pressed="true"\] \{[^}]*border/);
  assert.doesNotMatch(css, /\.primary-footer \{[^}]*border-top:/);
  assert.match(css, /\.app-shell \{[^}]*height: 100vh;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.primary-sidebar \{[^}]*height: 100vh;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.package-mount-region \{[^}]*min-height: 0;[^}]*overflow-y: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.app-shell \{[^}]*height: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.primary-sidebar \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.package-mount-region \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.package-mount \{[^}]*height: auto;/s);
  assert.doesNotMatch(css, /@media \(max-width: 720px\)[\s\S]*\.primary-footer \{[^}]*display: none;/s);
});

test('the Docs keeps its tree fixed while both panes scroll when needed', async () => {
  const css = await readFile(new URL('./docs/docs.css', import.meta.url), 'utf8');
  assert.match(css, /\.docs-layout \{[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.document-sidebar \{[^}]*display: flex;[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow: hidden;[^}]*padding: 32px 10px 20px 20px;/s);
  assert.match(css, /\.document-sidebar h2 \{[^}]*font: 400 28px\/1 var\(--display\);/s);
  assert.doesNotMatch(css, /\.document-sidebar \.eyebrow \{/);
  assert.match(css, /\.document-tree \{[^}]*min-height: 0;[^}]*flex: 1;[^}]*overflow-y: auto;[^}]*scrollbar-width: thin;[^}]*scrollbar-color: var\(--line\) transparent;/s);
  assert.match(css, /\.document-tree::-webkit-scrollbar \{[^}]*width: 4px;/s);
  assert.match(css, /\.document-tree::-webkit-scrollbar-thumb \{[^}]*background: var\(--line\);/s);
  assert.match(css, /\.document-header \{[^}]*margin: 0;[^}]*padding: 28px 52px 20px;[^}]*border-bottom: 1px solid var\(--line\);/s);
  assert.doesNotMatch(css, /\.document-header \{[^}]*max-width:/s);
  assert.match(css, /\.document-pane \{[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow-y: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.docs-layout \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.document-sidebar \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.document-pane \{[^}]*height: auto;[^}]*overflow: visible;/s);
});

test('the Home package does not expose retired product terminology', async () => {
  const homeModule = await readFile(new URL('./home/home.js', import.meta.url), 'utf8');
  assert.doesNotMatch(homeModule, retiredTerm);
});

test('shared agent panels fill their mount and share panel controls', async () => {
  const sharedCss = await readFile(new URL('../ui/ui.css', import.meta.url), 'utf8');
  const architectureCss = await readFile(new URL('./architecture/architecture.css', import.meta.url), 'utf8');
  assert.match(sharedCss, /\.resonance-agent-panel \{[^}]*height: 100%;[^}]*min-height: 0;/s);
  assert.match(sharedCss, /\.resonance-agent-transcript \{[^}]*flex: 1 1 auto;[^}]*min-height: 0;/s);
  assert.match(sharedCss, /\.resonance-agent-header \{[^}]*padding: 28px 20px 20px;/s);
  assert.match(sharedCss, /\.resonance-agent-panel button \{[^}]*background: var\(--ink\);[^}]*color: var\(--paper\);/s);
  assert.doesNotMatch(architectureCss, /\.architecture-agent button \{/);
});

test('the repository Home presents the Resonance manifesto', async () => {
  const html = await readFile(new URL('../../.resonance/home.html', import.meta.url), 'utf8');
  assert.match(html, /Integrated Application Environment/);
  assert.match(html, /The repository defines the team(?:'|’)s shared understanding/);
  assert.match(html, /Resonate is the action/);
  assert.equal((html.match(/class="home-section"/g) || []).length, 7);
  assert.match(html, /<pre><code>resonate init/);
  assert.match(html, /\.home-callout \{[^}]*background: var\(--sidebar\); color: var\(--code-ink\);/);
  assert.match(html, /\.home-commands pre \{[^}]*background: var\(--sidebar\); color: var\(--code-ink\);/);
  assert.match(html, /aria-labelledby="home-commands-title"/);
});
