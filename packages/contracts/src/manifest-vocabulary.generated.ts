// Generated from schema/manifest.v2.json. Do not edit.
export const manifestSchemaSha256 =
  "e0cd21f49eb88151a51e3eb31d901b87eb4fc7bca65e321c4b34e1b70a586e89";
export const roles = ["viewer", "contributor", "developer"] as const;
export const semanticCapabilities = [
  "documents:read",
  "documents:write",
  "workspace:read",
  "repository:read",
  "telemetry:write",
  "workspace-files:v1",
  "conversations:v1",
] as const;
export const semanticCapabilityProperties = {
  "documents:read": "documentsRead",
  "documents:write": "documentsWrite",
  "workspace:read": "workspaceRead",
  "repository:read": "repositoryRead",
  "telemetry:write": "telemetryWrite",
  "workspace-files:v1": "workspaceFilesV1",
  "conversations:v1": "conversationsV1",
} as const;
export const semanticAgentPermissions = [
  "read",
  "suggest-edits",
  "apply-edits",
  "create-documents",
  "post-messages",
] as const;
