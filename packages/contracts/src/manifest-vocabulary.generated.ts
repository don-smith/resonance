// Generated from schema/manifest.v2.json. Do not edit.
export const manifestSchemaSha256 =
  "4c41c465507f4870f37f4534c2ef89a9e6229c43bb39c30a35fd27ad000b8591";
export const roles = ["viewer", "contributor", "developer"] as const;
export const semanticCapabilities = [
  "documents:read",
  "documents:write",
  "workspace:read",
  "repository:read",
  "telemetry:write",
  "workspace-files:v1",
] as const;
export const semanticAgentPermissions = [
  "read",
  "suggest-edits",
  "apply-edits",
  "create-documents",
  "post-messages",
] as const;
