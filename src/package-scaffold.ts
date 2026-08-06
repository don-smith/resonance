import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PACKAGE_ID = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;

export type PackageScaffoldPlan = {
  id: string;
  label: string;
  directory: string;
  files: readonly string[];
  manifestSnippet: string;
  testFile: string;
};
export type VerifyPackageScaffold = (plan: PackageScaffoldPlan) => Promise<void>;

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }

export function validatePackageId(id: string): string {
  if (!PACKAGE_ID.test(id)) throw new Error(`Package id must be lowercase kebab-case: ${id}`);
  return id;
}

export function packageLabel(id: string): string {
  return validatePackageId(id).split('-').map((part) => `${part[0].toUpperCase()}${part.slice(1)}`).join(' ');
}

export function packageTemplates(id: string): Record<string, string> {
  const label = packageLabel(id);
  const functionName = `create${label.replaceAll(' ', '')}Package`;
  return {
    'index.ts': `import type { HostContext, PackageDefinition, PackageInput, PackageRegistration } from '../../package-contract.ts';

const metadata = { id: '${id}', version: '1.0.0', hostVersion: '1', label: '${label}', order: 40 } as const;
export function packageInput(input: PackageInput) { if (Object.keys(input).length > 0) throw new Error('${label} does not accept package inputs.'); return {}; }
function register(_context: HostContext, input: PackageInput): PackageRegistration {
  packageInput(input);
  return { metadata,
    routes: [{ method: 'GET', path: '/api/${id}', handler: async (_request, response) => response.json(200, { id: metadata.id, label: metadata.label }) }],
    assets: [{ path: '/assets/${id}/${id}.js', file: 'src/packages/${id}/${id}.js', contentType: 'text/javascript; charset=utf-8' }, { path: '/assets/${id}/${id}.css', file: 'src/packages/${id}/${id}.css', contentType: 'text/css; charset=utf-8' }],
    navigation: [{ id: metadata.id, label: metadata.label, order: metadata.order }],
    browser: { id: metadata.id, entry: '/assets/${id}/${id}.js', stylesheet: '/assets/${id}/${id}.css' },
  };
}
const packageDefinition: PackageDefinition = { metadata, register }; export default packageDefinition;
`,
    [`${id}.js`]: `export default function ${functionName}({ fetchFn = fetch } = {}) {
  let root;
  const render = (html) => { root.innerHTML = html; };
  return { mount(mountRoot) { root = mountRoot; render('<article class="${id}-workspace"><p>Loading ${label}…</p></article>'); },
    async activate() { root.hidden = false; render('<article class="${id}-workspace"><p>Loading ${label}…</p></article>'); const response = await fetchFn('/api/${id}'); if (!response.ok) { const error = new Error('${label} could not be loaded.'); render('<p class="${id}-error"></p>'); root.querySelector('.${id}-error').textContent = error.message; throw error; } const value = await response.json(); render('<article class="${id}-workspace"><h1></h1><p>Starter package is ready.</p></article>'); root.querySelector('h1').textContent = value.label; },
    deactivate() { root.hidden = true; },
  };
}
`,
    [`${id}.css`]: `.${id}-workspace { padding: 2rem; }\n.${id}-error { color: #a11; }\n`,
    'README.md': `# ${label} package

## Responsibilities
- Provide a read-only starter route at \`/api/${id}\`.
- Render only inside Shell's supplied private mount.
- Serve its registered browser entrypoint and stylesheet.

## Configuration
Add this explicit entry to the viewed repository's \`.resonance/config.json\` package allowlist:
\`\`\`json
"${id}": { "module": "src/packages/${id}/index.ts" }
\`\`\`
The module path is application-root-relative; this scaffold does not change repository configuration.

## Ownership boundary
Keep reusable code here. Validate repository paths with \`HostContext.resolveRepositoryPath()\` before reading under \`HostContext.repositoryRoot\`.
`,
    [`${id}.test.ts`]: `import test from 'node:test'; import assert from 'node:assert/strict'; import { mkdtemp, rm } from 'node:fs/promises'; import { tmpdir } from 'node:os'; import { fileURLToPath } from 'node:url'; import { parseHTML } from 'linkedom'; import { createApp } from '../../server.ts'; import createPackage from './${id}.js';
async function withServer(run, options) { const server = await createApp(options); await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve)); const { port } = server.address(); try { await run(\`http://127.0.0.1:\${port}\`); } finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); } }
test('${id} composes through server and browser contracts', async () => { const root = await mkdtemp(\`${'${tmpdir()}'}/resonance-${id}-\`); try { const appRoot = fileURLToPath(new URL('../../../', import.meta.url)); const config = { version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, '${id}': { module: 'src/packages/${id}/index.ts' } } }; await withServer(async (baseUrl) => { const manifest = await fetch(\`${'${baseUrl}'}/api/manifest\`).then((response) => response.json()); assert.ok(manifest.packages.some((item) => item.id === '${id}')); assert.deepEqual(await fetch(\`${'${baseUrl}'}/api/${id}\`).then((response) => response.json()), { id: '${id}', label: '${label}' }); assert.equal((await fetch(\`${'${baseUrl}'}/assets/${id}/${id}.js\`)).status, 200); assert.equal((await fetch(\`${'${baseUrl}'}/assets/${id}/${id}.css\`)).status, 200); }, { root, appRoot, config }); const { document } = parseHTML('<!doctype html><body></body>'); const mount = document.createElement('section'); const instance = createPackage({ fetchFn: async () => ({ ok: true, async json() { return { label: '${label}' }; } }) }); instance.mount(mount); await instance.activate(); assert.equal(mount.querySelector('h1').textContent, '${label}'); instance.deactivate(); assert.equal(mount.hidden, true); } finally { await rm(root, { recursive: true, force: true }); } });
`,
  };
}

export function planPackageScaffold({ appRoot, id }: { appRoot: string | URL; id: string }): PackageScaffoldPlan {
  const packageId = validatePackageId(id);
  const root = path.resolve(toPath(appRoot));
  const directory = path.resolve(root, 'src', 'packages', packageId);
  const relative = path.relative(root, directory);
  if (!relative || relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error(`Package destination must stay inside the application root: ${packageId}`);
  }
  const files = Object.keys(packageTemplates(packageId)).sort().map((filename) => path.join(directory, filename));
  return Object.freeze({
    id: packageId,
    label: packageLabel(packageId),
    directory,
    files: Object.freeze(files),
    manifestSnippet: `"${packageId}": { "module": "src/packages/${packageId}/index.ts" }`,
    testFile: path.join(directory, `${packageId}.test.ts`),
  });
}

export async function createPackageScaffold({ appRoot, id, verify }: { appRoot: string | URL; id: string; verify: VerifyPackageScaffold }): Promise<PackageScaffoldPlan> {
  if (typeof verify !== 'function') throw new Error('Package scaffolding requires a focused verifier.');
  const plan = planPackageScaffold({ appRoot, id });
  let reserved = false;
  try {
    await mkdir(plan.directory);
    reserved = true;
    for (const [filename, content] of Object.entries(packageTemplates(plan.id))) {
      await writeFile(path.join(plan.directory, filename), content, { encoding: 'utf8', flag: 'wx' });
    }
    await verify(plan);
    return plan;
  } catch (error) {
    if (reserved) await rm(plan.directory, { recursive: true, force: true });
    throw error;
  }
}
