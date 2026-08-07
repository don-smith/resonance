import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { packageLabel, validatePackageId } from './package-scaffold.ts';

const MEMBER_MANIFEST = 'member-packages.json';
const PACKAGE_JSON = 'package.json';
const PACKAGE_CONTRACT = 'src/package-contract.ts';
const AUTHORING_SKILL = '.agents/skills/package-authoring/SKILL.md';

export type MemberRepositoryPlan = { directory: string; files: readonly string[] };
export type MemberPackagePlan = { id: string; label: string; directory: string; files: readonly string[]; manifestSnippet: string; testFile: string };
export type VerifyMemberPackage = (plan: MemberPackagePlan) => Promise<void>;

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function memberPackageTemplates(id: string): Record<string, string> {
  const label = packageLabel(id);
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
    [`${id}.js`]: `export default function create${label.replaceAll(' ', '')}Package() { let root; return { mount(mountRoot) { root = mountRoot; root.innerHTML = '<article><h2>${label}</h2><p>Member package is ready.</p></article>'; }, activate() { root.hidden = false; }, deactivate() { root.hidden = true; } }; }
`,
    [`${id}.css`]: `.${id}-workspace { padding: 2rem; }\n`,
    'README.md': `# ${label} member package

This package is owned by the member-package repository and is selected through a viewed repository's ignored \`.resonance/member-config.json\`.

Keep routes under \/api\/${id} and assets under \/assets\/${id}. Resolve viewed-repository files with HostContext.resolveRepositoryPath().
`,
    [`${id}.test.ts`]: `import test from 'node:test'; import assert from 'node:assert/strict'; import packageDefinition from './index.ts';

test('${id} registers through the member package contract', () => {
  const registration = packageDefinition.register({ repositoryRoot: process.cwd(), appRoot: process.cwd(), resolveRepositoryPath: (relativePath) => relativePath }, {});
  assert.equal(registration.metadata.id, '${id}');
  assert.equal(registration.browser.entry, '/assets/${id}/${id}.js');
  assert.equal(registration.navigation[0].id, '${id}');
});
`,
  };
}

export function planMemberPackageScaffold({ memberRoot, id }: { memberRoot: string | URL; id: string }): MemberPackagePlan {
  const packageId = validatePackageId(id);
  const root = path.resolve(toPath(memberRoot));
  const directory = path.resolve(root, 'src', 'packages', packageId);
  const relative = path.relative(root, directory);
  if (!relative || relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error(`Member package destination must stay inside the member repository: ${packageId}`);
  const files = Object.keys(memberPackageTemplates(packageId)).sort().map((filename) => path.join(directory, filename));
  return Object.freeze({ id: packageId, label: packageLabel(packageId), directory, files: Object.freeze(files), manifestSnippet: `"${packageId}": { "module": "src/packages/${packageId}/index.ts" }`, testFile: path.join(directory, `${packageId}.test.ts`) });
}

export async function createMemberPackageScaffold({ memberRoot, id, verify }: { memberRoot: string | URL; id: string; verify: VerifyMemberPackage }): Promise<MemberPackagePlan> {
  if (typeof verify !== 'function') throw new Error('Member package scaffolding requires a focused verifier.');
  const plan = planMemberPackageScaffold({ memberRoot, id });
  let reserved = false;
  try {
    await mkdir(path.dirname(plan.directory), { recursive: true });
    await mkdir(plan.directory);
    reserved = true;
    for (const [filename, content] of Object.entries(memberPackageTemplates(plan.id))) await writeFile(path.join(plan.directory, filename), content, { encoding: 'utf8', flag: 'wx' });
    await verify(plan);
    return plan;
  } catch (error) {
    if (reserved) await rm(plan.directory, { recursive: true, force: true });
    throw error;
  }
}

function repositoryFiles(directory: string): Record<string, string> {
  const name = path.basename(directory).toLowerCase().replace(/[^a-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '') || 'member-packages';
  return {
    [MEMBER_MANIFEST]: '{\n  "version": 1,\n  "packages": {}\n}\n',
    [PACKAGE_JSON]: `${JSON.stringify({ name, private: true, type: 'module', packageManager: 'bun@1.3.13', scripts: { test: 'bun test' } }, null, 2)}\n`,
    '.gitignore': 'node_modules/\n.DS_Store\n',
    'README.md': `# ${name}\n\nThis repository contains developer-specific Resonance packages. Select packages from a viewed repository with:\n\n\`resonate member install ${directory}\`\n\nCreate a package with \`resonate member package create <id>\`. The checked-in \`${MEMBER_MANIFEST}\` is the explicit package allowlist; it is not discovered by directory scanning.\n`,
  };
}

export function planMemberRepository({ directory }: { directory: string | URL }): MemberRepositoryPlan {
  const root = path.resolve(toPath(directory));
  const files = [...Object.keys(repositoryFiles(root)), PACKAGE_CONTRACT, AUTHORING_SKILL].map((filename) => path.join(root, filename));
  return Object.freeze({ directory: root, files: Object.freeze(files) });
}

async function defaultGitInit(directory: string): Promise<void> {
  const child = Bun.spawn(['git', 'init', directory], { stdout: 'ignore', stderr: 'inherit' });
  if (await child.exited !== 0) throw new Error(`Unable to initialize Git in ${directory}.`);
}

export async function createMemberRepository({ directory, appRoot = fileURLToPath(new URL('../', import.meta.url)), gitInitFn = defaultGitInit }: { directory: string | URL; appRoot?: string | URL; gitInitFn?: (directory: string) => Promise<void> }): Promise<MemberRepositoryPlan> {
  const plan = planMemberRepository({ directory });
  let directoryExists = false; let nonEmpty = false;
  try { directoryExists = true; nonEmpty = (await readdir(plan.directory)).length > 0; } catch (error) { if (error?.code !== 'ENOENT') throw error; }
  if (nonEmpty) throw new Error(`Member repository directory is not empty: ${plan.directory}`);
  const files = repositoryFiles(plan.directory);
  files[PACKAGE_CONTRACT] = await readFile(path.join(toPath(appRoot), 'src', 'package-contract.ts'), 'utf8');
  files[AUTHORING_SKILL] = await readFile(path.join(toPath(appRoot), '.agents', 'skills', 'package-authoring', 'SKILL.md'), 'utf8');
  const createdDirectory = !directoryExists;
  await mkdir(plan.directory, { recursive: true });
  await mkdir(path.join(plan.directory, 'src', 'packages'), { recursive: true });
  const written: string[] = [];
  try {
    for (const [relative, content] of Object.entries(files)) { const filename = path.join(plan.directory, relative); await mkdir(path.dirname(filename), { recursive: true }); await writeFile(filename, content, { encoding: 'utf8', flag: 'wx' }); written.push(filename); }
    await gitInitFn(plan.directory);
    return plan;
  } catch (error) {
    await Promise.all(written.map((filename) => rm(filename, { force: true })));
    if (createdDirectory) await rm(plan.directory, { recursive: true, force: true });
    throw error;
  }
}
