import { createShell } from '/assets/shell/shell.js';

const THEME_STORAGE_KEY = 'resonance:theme';
const THEME_PREFERENCES = new Set(['light', 'dark', 'system']);

function getThemeStorage(windowRoot) {
  try { return windowRoot?.localStorage || null; }
  catch { return null; }
}

function getColorSchemeQuery(windowRoot) {
  try { return windowRoot?.matchMedia?.('(prefers-color-scheme: dark)') || null; }
  catch { return null; }
}

export function createThemeController({ documentRoot = document, windowRoot = globalThis.window } = {}) {
  const root = documentRoot.documentElement;
  const selector = documentRoot.querySelector('[data-shell-theme-selector]');
  const controls = [...documentRoot.querySelectorAll('[data-shell-theme]')];
  const storage = getThemeStorage(windowRoot);
  const colorSchemeQuery = getColorSchemeQuery(windowRoot);
  let preference = 'system';
  try {
    const stored = storage?.getItem(THEME_STORAGE_KEY);
    if (THEME_PREFERENCES.has(stored)) preference = stored;
  } catch { /* browser storage is optional */ }

  function render() {
    const resolved = preference === 'system' ? (colorSchemeQuery?.matches ? 'dark' : 'light') : preference;
    root.setAttribute('data-theme-preference', preference);
    root.setAttribute('data-theme', resolved);
    for (const control of controls) control.setAttribute('aria-pressed', String(control.dataset.shellTheme === preference));
  }

  function setPreference(nextPreference) {
    if (!THEME_PREFERENCES.has(nextPreference)) throw new Error(`Unsupported theme preference: ${nextPreference}`);
    preference = nextPreference;
    try { storage?.setItem(THEME_STORAGE_KEY, preference); }
    catch { /* the selected theme still applies for this page */ }
    render();
  }

  const handleClick = (event) => {
    const control = event.target.closest('[data-shell-theme]');
    if (control && selector?.contains(control)) setPreference(control.dataset.shellTheme);
  };
  const handleSystemChange = () => { if (preference === 'system') render(); };
  selector?.addEventListener('click', handleClick);
  if (colorSchemeQuery?.addEventListener) colorSchemeQuery.addEventListener('change', handleSystemChange);
  else colorSchemeQuery?.addListener?.(handleSystemChange);
  render();

  return {
    getPreference() { return preference; },
    setPreference,
    dispose() {
      selector?.removeEventListener('click', handleClick);
      if (colorSchemeQuery?.removeEventListener) colorSchemeQuery.removeEventListener('change', handleSystemChange);
      else colorSchemeQuery?.removeListener?.(handleSystemChange);
    },
  };
}

function loadStylesheet(documentRoot, packageId, stylesheet) {
  if (!stylesheet || documentRoot.querySelector(`link[data-package-style="${packageId}"]`)) return;
  const link = documentRoot.createElement('link');
  link.rel = 'stylesheet';
  link.href = stylesheet;
  link.dataset.packageStyle = packageId;
  documentRoot.head.append(link);
}

function renderRepositoryMetadata(documentRoot, manifest) {
  const repository = manifest.repository || {};
  const runtime = manifest.runtime || {};
  const name = documentRoot.querySelector('[data-shell-repository-name]');
  const version = documentRoot.querySelector('[data-shell-repository-version]');
  const tagline = documentRoot.querySelector('[data-shell-repository-tagline]');
  const runtimeVersion = documentRoot.querySelector('[data-shell-runtime-version]');
  if (name && repository.name) name.textContent = repository.name;
  if (version) {
    version.textContent = repository.version ? `v${repository.version}` : '';
    version.hidden = !repository.version;
  }
  if (tagline) {
    tagline.textContent = repository.tagline || '';
    tagline.hidden = !repository.tagline;
  }
  if (runtimeVersion && runtime.version) runtimeVersion.textContent = runtime.version;
}

export async function startApplication({ documentRoot = document, windowRoot = globalThis.window, fetchFn = fetch, eventSourceFactory = (url) => new EventSource(url) } = {}) {
  const theme = createThemeController({ documentRoot, windowRoot });
  const response = await fetchFn('/api/manifest');
  if (!response.ok) throw new Error('Package manifest could not be loaded.');
  const manifest = await response.json();
  renderRepositoryMetadata(documentRoot, manifest);
  const navigation = documentRoot.querySelector('#primary-navigation');
  const mount = documentRoot.querySelector('#package-mount');
  const home = documentRoot.querySelector('[data-shell-home]');
  if (!navigation || !mount) throw new Error('Shell mount is missing.');

  const shell = createShell({ documentRoot, navigation, mount, home });
  for (const diagnostic of manifest.diagnostics || []) {
    const notice = documentRoot.createElement('p');
    notice.className = 'shell-diagnostic'; notice.dataset.packageDiagnostic = diagnostic.id;
    notice.textContent = `${diagnostic.id}: ${diagnostic.message}`;
    navigation.append(notice);
    console.warn(`Member package ${diagnostic.id}: ${diagnostic.message}`);
  }
  const packages = new Map();
  for (const packageInfo of manifest.packages) {
    if (packageInfo.id === 'shell') continue;
    try {
      loadStylesheet(documentRoot, packageInfo.id, packageInfo.stylesheet);
      const loaded = await import(packageInfo.entry);
      const factory = loaded.default;
      if (typeof factory !== 'function') throw new Error(`Invalid browser module for package ${packageInfo.id}.`);
      const instance = factory({ fetchFn, eventSourceFactory });
      instance.mount(shell.createMount(packageInfo.id));
      shell.registerPackage(packageInfo.id, instance);
      packages.set(packageInfo.id, instance);
    } catch (error) {
      console.warn(`Skipping browser package ${packageInfo.id}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  const homeId = packages.has('home') ? 'home' : null;
  shell.setHomePackage(homeId);
  const workspaceNavigation = manifest.navigation.filter((item) => item.id !== homeId && packages.has(item.id));
  shell.renderNavigation(workspaceNavigation);
  const initialId = homeId || workspaceNavigation[0]?.id;
  if (initialId) {
    try { await shell.activate(initialId); }
    catch { /* keep Shell usable when a package activation fails */ }
  }
  return { manifest, packages, activate: shell.activate, theme };
}

if (!globalThis.__RESONANCE_TEST__) {
  startApplication().catch((error) => { document.body.innerHTML = `<p class="shell-error">${String(error.message || error)}</p>`; });
}
