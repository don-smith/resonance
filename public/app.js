import { createShell } from '/assets/shell/shell.js';
import { createHomePackage } from '/assets/home/home.js';
import { createDocsPackage } from '/assets/docs/docs.js';

const packageFactories = new Map([
  ['home', createHomePackage],
  ['docs', createDocsPackage],
]);

export async function startApplication({ documentRoot = document, fetchFn = fetch } = {}) {
  const response = await fetchFn('/api/manifest');
  if (!response.ok) throw new Error('Package manifest could not be loaded.');
  const manifest = await response.json();
  const navigation = documentRoot.querySelector('#primary-navigation');
  const mount = documentRoot.querySelector('#package-mount');
  if (!navigation || !mount) throw new Error('Shell mount is missing.');

  const shell = createShell({ documentRoot, navigation, mount });
  shell.renderNavigation(manifest.navigation);
  const packages = new Map();

  for (const packageInfo of manifest.packages) {
    const factory = packageFactories.get(packageInfo.id);
    if (!factory || packageInfo.id === 'shell') continue;
    const instance = factory({ fetchFn });
    instance.mount(shell.createMount(packageInfo.id));
    shell.registerPackage(packageInfo.id, instance);
    packages.set(packageInfo.id, instance);
  }

  const first = manifest.navigation.find((item) => packages.has(item.id));
  if (first) {
    try {
      await shell.activate(first.id);
    } catch {
      // A transitional package may not have a server route yet; keep Shell usable.
    }
  }

  return { manifest, packages, activate: shell.activate };
}

if (!globalThis.__THEVIEW_TEST__) {
  startApplication().catch((error) => {
    document.body.innerHTML = `<p class="shell-error">${String(error.message || error)}</p>`;
  });
}
