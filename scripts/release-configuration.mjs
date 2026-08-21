import { readFileSync } from "node:fs";

export const REQUIRED_TARGETS = [
  "darwin-aarch64",
  "darwin-x86_64",
  "windows-x86_64",
];

const PLACEHOLDER_MARKERS = [
  "placeholder",
  "replace",
  "example",
  "your-",
  "changeme",
  "<",
];

export function readReleaseConfiguration(path) {
  let source;
  try {
    source = readFileSync(path, "utf8");
  } catch (error) {
    throw new Error(
      `Could not read release configuration ${path}: ${error.message}`,
    );
  }
  try {
    return validateReleaseConfiguration(JSON.parse(source));
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`Release configuration is malformed: ${error.message}`);
    }
    throw error;
  }
}

export function validateReleaseConfiguration(configuration) {
  if (
    !configuration ||
    typeof configuration !== "object" ||
    Array.isArray(configuration)
  ) {
    throw new Error("Release configuration must be a JSON object.");
  }

  validateHttpsUrl(configuration.manifestEndpoint, "manifestEndpoint");
  validateHttpsUrl(configuration.artifactBaseUrl, "artifactBaseUrl");

  if (isPlaceholder(configuration.publicKey)) {
    throw new Error(
      "publicKey must be a provisioned, non-placeholder updater key.",
    );
  }
  if (!configuration.targets || typeof configuration.targets !== "object") {
    throw new Error("targets must contain metadata for every release target.");
  }

  for (const target of REQUIRED_TARGETS) {
    const artifact = configuration.targets[target]?.artifact;
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

  return configuration;
}

export function validateSigningSecret(value) {
  if (typeof value !== "string" || value.trim().length < 32) {
    throw new Error(
      "TAURI_SIGNING_PRIVATE_KEY must be supplied by CI for releases.",
    );
  }
}

function validateHttpsUrl(value, name) {
  let url;
  try {
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

function isPlaceholder(value) {
  if (typeof value !== "string") return true;
  const normalized = value.trim().toLowerCase();
  return (
    normalized.length < 32 ||
    PLACEHOLDER_MARKERS.some((marker) => normalized.includes(marker))
  );
}
