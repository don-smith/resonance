import { readFileSync } from "node:fs";

export interface ReleaseTarget {
  artifact: string;
}

export interface ReleaseConfiguration {
  manifestEndpoint: string;
  artifactBaseUrl: string;
  publicKey: string;
  targets: Record<string, ReleaseTarget>;
}

export const REQUIRED_TARGETS = [
  "darwin-aarch64",
  "darwin-x86_64",
  "windows-x86_64",
] as const;

const PLACEHOLDER_MARKERS = [
  "placeholder",
  "replace",
  "example",
  "your-",
  "changeme",
  "<",
];

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function readReleaseConfiguration(path: string): ReleaseConfiguration {
  let source: string;
  try {
    source = readFileSync(path, "utf8");
  } catch (error) {
    throw new Error(
      `Could not read release configuration ${path}: ${errorMessage(error)}`,
    );
  }
  try {
    return validateReleaseConfiguration(JSON.parse(source) as unknown);
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`Release configuration is malformed: ${error.message}`);
    }
    throw error;
  }
}

export function validateReleaseConfiguration(
  configuration: unknown,
): ReleaseConfiguration {
  if (
    !configuration ||
    typeof configuration !== "object" ||
    Array.isArray(configuration)
  ) {
    throw new Error("Release configuration must be a JSON object.");
  }
  const candidate = configuration as Record<string, unknown>;

  validateHttpsUrl(candidate.manifestEndpoint, "manifestEndpoint");
  validateHttpsUrl(candidate.artifactBaseUrl, "artifactBaseUrl");

  if (isPlaceholder(candidate.publicKey)) {
    throw new Error(
      "publicKey must be a provisioned, non-placeholder updater key.",
    );
  }
  if (
    !candidate.targets ||
    typeof candidate.targets !== "object" ||
    Array.isArray(candidate.targets)
  ) {
    throw new Error("targets must contain metadata for every release target.");
  }
  const targets = candidate.targets as Record<
    string,
    Record<string, unknown> | undefined
  >;

  for (const target of REQUIRED_TARGETS) {
    const artifact = targets[target]?.artifact;
    if (
      typeof artifact !== "string" ||
      artifact.trim() === "" ||
      artifact.includes("..") ||
      artifact.includes("/") ||
      artifact.includes("\\")
    ) {
      throw new Error(
        `targets.${target}.artifact must be a safe, non-empty filename.`,
      );
    }
  }

  return candidate as unknown as ReleaseConfiguration;
}

export function validateSigningSecret(value: string | undefined): void {
  if (typeof value !== "string" || value.trim().length < 32) {
    throw new Error(
      "TAURI_SIGNING_PRIVATE_KEY must be supplied by CI for releases.",
    );
  }
}

function validateHttpsUrl(value: unknown, name: string): void {
  let url: URL;
  try {
    if (typeof value !== "string") throw new TypeError();
    url = new URL(value);
  } catch {
    throw new Error(`${name} must be an absolute HTTPS URL.`);
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.username ||
    url.password
  ) {
    throw new Error(`${name} must be an absolute HTTPS URL.`);
  }
}

function isPlaceholder(value: unknown): boolean {
  if (typeof value !== "string") return true;
  const normalized = value.trim().toLowerCase();
  return (
    normalized.length < 32 ||
    PLACEHOLDER_MARKERS.some((marker) => normalized.includes(marker))
  );
}
