import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import prettier from "prettier";

type JsonSchema = Readonly<{
  properties: {
    minRole: { enum: string[] };
    capabilities: { items: { enum: string[] } };
    agent: { properties: { permissions: { items: { enum: string[] } } } };
  };
}>;

type BindingOptions = Readonly<{
  root?: string;
  mode?: "write" | "check";
}>;

const TYPESCRIPT_OUTPUT =
  "packages/contracts/src/manifest-vocabulary.generated.ts";
const RUST_OUTPUT = "crates/runtime/src/packages/manifest_generated.rs";

function rustVariant(value: string): string {
  return value
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() ?? ""}${part.slice(1)}`)
    .join("")
    .replace(/V(\d+)$/, "V$1");
}

function rustEnum(name: string, values: string[]): string {
  return `#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum ${name} {
${values
  .map(
    (value) =>
      `    #[serde(rename = ${JSON.stringify(value)})]\n    ${rustVariant(value)},`,
  )
  .join("\n")}
}`;
}

async function renderBindings(root: string): Promise<{
  typescript: string;
  rust: string;
}> {
  const schemaSource = await readFile(
    resolve(root, "packages/contracts/schema/manifest.v2.json"),
    "utf8",
  );
  const schema = JSON.parse(schemaSource) as JsonSchema;
  const schemaSha256 = createHash("sha256").update(schemaSource).digest("hex");
  const roles = schema.properties.minRole.enum;
  const capabilities = schema.properties.capabilities.items.enum;
  const permissions = schema.properties.agent.properties.permissions.items.enum;

  const typescript = await prettier.format(
    `// Generated from schema/manifest.v2.json. Do not edit.
export const manifestSchemaSha256 = ${JSON.stringify(schemaSha256)};
export const roles = ${JSON.stringify(roles)} as const;
export const semanticCapabilities = ${JSON.stringify(capabilities)} as const;
export const semanticAgentPermissions = ${JSON.stringify(permissions)} as const;
`,
    { parser: "typescript" },
  );
  const rust = `// Generated from packages/contracts/schema/manifest.v2.json. Do not edit.

use serde::Deserialize;

pub const MANIFEST_SCHEMA_SHA256: &str =
    ${JSON.stringify(schemaSha256)};

${rustEnum("ManifestRole", roles)}

${rustEnum("SemanticCapability", capabilities)}

${rustEnum("AgentPermission", permissions)}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageManifest {
    pub manifest_version: u8,
    pub source: PackageSourceValue,
    pub id: String,
    pub name: String,
    pub description: String,
    pub nav: Navigation,
    pub content: ContentEntry,
    pub events: EventDeclarations,
    pub min_role: ManifestRole,
    #[serde(default)]
    pub capabilities: Vec<SemanticCapability>,
    pub agent: Option<AgentConfiguration>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum PackageSourceValue {
    #[serde(rename = "bundled")]
    Bundled,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Navigation {
    pub label: String,
    pub icon: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContentEntry {
    pub entry: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventDeclarations {
    pub emits: Vec<String>,
    pub consumes: Vec<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConfiguration {
    pub system_prompt: String,
    pub permissions: Vec<AgentPermission>,
    pub context_providers: Vec<String>,
}
`;
  return { typescript, rust };
}

async function publish(
  path: string,
  expected: string,
  root: string,
  mode: "write" | "check",
): Promise<void> {
  if (mode === "check") {
    let current: string;
    try {
      current = await readFile(path, "utf8");
    } catch {
      throw new Error(
        `${relative(root, path)} is missing; run pnpm packages:generate`,
      );
    }
    if (current !== expected) {
      throw new Error(
        `${relative(root, path)} is stale; run pnpm packages:generate`,
      );
    }
    return;
  }
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, expected);
}

export async function generateManifestBindings({
  root = process.cwd(),
  mode = "write",
}: BindingOptions = {}): Promise<void> {
  const absoluteRoot = resolve(root);
  const { typescript, rust } = await renderBindings(absoluteRoot);
  await Promise.all([
    publish(
      resolve(absoluteRoot, TYPESCRIPT_OUTPUT),
      typescript,
      absoluteRoot,
      mode,
    ),
    publish(resolve(absoluteRoot, RUST_OUTPUT), rust, absoluteRoot, mode),
  ]);
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  await generateManifestBindings();
}
