// Generated from packages/contracts/schema/manifest.v2.json. Do not edit.

use serde::Deserialize;

pub const MANIFEST_SCHEMA_SHA256: &str =
    "e0cd21f49eb88151a51e3eb31d901b87eb4fc7bca65e321c4b34e1b70a586e89";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum ManifestRole {
    #[serde(rename = "viewer")]
    Viewer,
    #[serde(rename = "contributor")]
    Contributor,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum SemanticCapability {
    #[serde(rename = "documents:read")]
    DocumentsRead,
    #[serde(rename = "documents:write")]
    DocumentsWrite,
    #[serde(rename = "workspace:read")]
    WorkspaceRead,
    #[serde(rename = "repository:read")]
    RepositoryRead,
    #[serde(rename = "telemetry:write")]
    TelemetryWrite,
    #[serde(rename = "workspace-files:v1")]
    WorkspaceFilesV1,
    #[serde(rename = "conversations:v1")]
    ConversationsV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum AgentPermission {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "suggest-edits")]
    SuggestEdits,
    #[serde(rename = "apply-edits")]
    ApplyEdits,
    #[serde(rename = "create-documents")]
    CreateDocuments,
    #[serde(rename = "post-messages")]
    PostMessages,
}

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
