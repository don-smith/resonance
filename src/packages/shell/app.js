import { createShell } from '/assets/shell/shell.js';

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

export async function startApplication({ documentRoot = document, fetchFn = fetch, eventSourceFactory = (url) => new EventSource(url) } = {}) {
  const response = await fetchFn('/api/manifest');
  if (!response.ok) throw new Error('Package manifest could not be loaded.');
  const manifest = await response.json();
  renderRepositoryMetadata(documentRoot, manifest);
  const navigation = documentRoot.querySelector('#primary-navigation');
  const mount = documentRoot.querySelector('#package-mount');
  if (!navigation || !mount) throw new Error('Shell mount is missing.');

  const shell = createShell({ documentRoot, navigation, mount });
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
  shell.renderNavigation(manifest.navigation.filter((item) => packages.has(item.id)));
  const first = manifest.navigation.find((item) => packages.has(item.id));
  if (first) {
    try { await shell.activate(first.id); }
    catch { /* keep Shell usable when a package activation fails */ }
  }
  return { manifest, packages, activate: shell.activate };
}

if (!globalThis.__RESONANCE_TEST__) {
  startApplication().catch((error) => { document.body.innerHTML = `<p class="shell-error">${String(error.message || error)}</p>`; });
}
