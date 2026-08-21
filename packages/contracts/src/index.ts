import Ajv2020, { type ErrorObject } from "ajv/dist/2020.js";

import manifestSchema from "../schema/manifest.v1.json";

export const roles = ["viewer", "contributor", "developer"] as const;
export const semanticCapabilities = [
  "documents:read",
  "documents:write",
  "workspace:read",
  "repository:read",
  "telemetry:write",
] as const;
export const semanticAgentPermissions = [
  "read",
  "suggest-edits",
  "apply-edits",
  "create-documents",
  "post-messages",
] as const;

export type ManifestRole = (typeof roles)[number];
export type PackageManifest = {
  manifestVersion: 1;
  source: "bundled-team";
  id: string;
  name: string;
  description: string;
  nav: { label: string; icon: string };
  events: { emits: string[]; consumes: string[] };
  minRole: ManifestRole;
  capabilities?: (typeof semanticCapabilities)[number][];
  agent?: {
    systemPrompt: string;
    permissions: (typeof semanticAgentPermissions)[number][];
    contextProviders: string[];
  };
};

export type ManifestDiagnostic = {
  path: string;
  message: string;
};

const validator = new Ajv2020({ allErrors: true, strict: true }).compile(
  manifestSchema,
);

function diagnostics(
  errors: ErrorObject[] | null | undefined,
): ManifestDiagnostic[] {
  return (errors ?? [])
    .map((error) => ({
      path: error.instancePath || "/",
      message: error.message ?? "is invalid",
    }))
    .sort((left, right) =>
      `${left.path}:${left.message}`.localeCompare(
        `${right.path}:${right.message}`,
      ),
    );
}

export function validateManifest(
  candidate: unknown,
):
  | { manifest: PackageManifest; diagnostics: [] }
  | { diagnostics: ManifestDiagnostic[] } {
  if (!validator(candidate)) {
    return { diagnostics: diagnostics(validator.errors) };
  }

  return { manifest: candidate as PackageManifest, diagnostics: [] };
}
