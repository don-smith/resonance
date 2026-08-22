//! Package-manifest and declared-event runtime seams.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

const ROLES: [&str; 3] = ["viewer", "contributor", "developer"];
const CAPABILITIES: [&str; 5] = [
    "documents:read",
    "documents:write",
    "workspace:read",
    "repository:read",
    "telemetry:write",
];
const AGENT_PERMISSIONS: [&str; 5] = [
    "read",
    "suggest-edits",
    "apply-edits",
    "create-documents",
    "post-messages",
];
const STANDARD_EVENTS: [&str; 9] = [
    "repo:changed",
    "doc:updated",
    "doc:opened",
    "message:received",
    "peer:joined",
    "peer:left",
    "peer:connection",
    "workspace:member-added",
    "workspace:member-removed",
];

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageManifest {
    pub manifest_version: u8,
    pub source: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub nav: Navigation,
    pub events: EventDeclarations,
    pub min_role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub agent: Option<AgentConfiguration>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Navigation {
    pub label: String,
    pub icon: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct EventDeclarations {
    pub emits: Vec<String>,
    pub consumes: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConfiguration {
    pub system_prompt: String,
    pub permissions: Vec<String>,
    pub context_providers: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PackageDiagnostic {
    pub package_id: String,
    pub message: String,
}

impl PackageDiagnostic {
    fn new(package_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            package_id: package_id.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PackageSource {
    BundledTeam,
    MemberLocal,
    Repository,
}

#[derive(Debug)]
pub struct PackageRegistry {
    manifests: BTreeMap<String, PackageManifest>,
}

impl PackageRegistry {
    /// Parses and validates only bundled team manifests. Diagnostics are sorted
    /// so callers can report reproducible remediation to package authors.
    pub fn load(
        source: PackageSource,
        raw_manifests: &[&str],
    ) -> Result<Self, Vec<PackageDiagnostic>> {
        if source != PackageSource::BundledTeam {
            return Err(vec![PackageDiagnostic::new(
                "<source>",
                "only bundled-team packages may load in Phase 1",
            )]);
        }

        let mut diagnostics = Vec::new();
        let mut manifests = BTreeMap::new();
        for raw in raw_manifests {
            match Self::parse(raw) {
                Ok(manifest) if manifests.contains_key(&manifest.id) => {
                    diagnostics.push(PackageDiagnostic::new(
                        manifest.id,
                        "namespace collision: package id is already registered",
                    ))
                }
                Ok(manifest) => {
                    manifests.insert(manifest.id.clone(), manifest);
                }
                Err(mut errors) => diagnostics.append(&mut errors),
            }
        }

        diagnostics.sort_by(|left, right| {
            (&left.package_id, &left.message).cmp(&(&right.package_id, &right.message))
        });
        if diagnostics.is_empty() {
            Ok(Self { manifests })
        } else {
            Err(diagnostics)
        }
    }

    pub fn get(&self, id: &str) -> Option<&PackageManifest> {
        self.manifests.get(id)
    }

    fn parse(raw: &str) -> Result<PackageManifest, Vec<PackageDiagnostic>> {
        let manifest: PackageManifest = serde_json::from_str(raw).map_err(|error| {
            vec![PackageDiagnostic::new(
                "<unknown>",
                format!("malformed manifest: {error}"),
            )]
        })?;
        let mut diagnostics = Vec::new();
        let id = manifest.id.clone();

        if manifest.manifest_version != 1 {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "manifestVersion must be 1",
            ));
        }
        if manifest.source != "bundled-team" {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "source must be bundled-team",
            ));
        }
        if !is_namespaced_id(&manifest.id) {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "id must use a lowercase namespace.name form",
            ));
        }
        if manifest.name.is_empty()
            || manifest.description.is_empty()
            || manifest.nav.label.is_empty()
            || manifest.nav.icon.is_empty()
        {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "name, description, and nav fields must be non-empty",
            ));
        }
        if !ROLES.contains(&manifest.min_role.as_str()) {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "minRole is not a supported role",
            ));
        }
        validate_set(
            &manifest.capabilities,
            &CAPABILITIES,
            "capability",
            &id,
            &mut diagnostics,
        );
        validate_events(&manifest.events.emits, "emitted", &id, &mut diagnostics);
        validate_events(&manifest.events.consumes, "consumed", &id, &mut diagnostics);
        if let Some(agent) = &manifest.agent {
            if agent.system_prompt.is_empty() {
                diagnostics.push(PackageDiagnostic::new(
                    id.clone(),
                    "agent systemPrompt must be non-empty",
                ));
            }
            validate_set(
                &agent.permissions,
                &AGENT_PERMISSIONS,
                "agent permission",
                &id,
                &mut diagnostics,
            );
            if has_duplicate(&agent.context_providers)
                || agent.context_providers.iter().any(String::is_empty)
            {
                diagnostics.push(PackageDiagnostic::new(
                    id.clone(),
                    "agent contextProviders must be unique non-empty values",
                ));
            }
        }

        if diagnostics.is_empty() {
            Ok(manifest)
        } else {
            Err(diagnostics)
        }
    }
}

fn validate_set(
    values: &[String],
    allowed: &[&str],
    label: &str,
    id: &str,
    diagnostics: &mut Vec<PackageDiagnostic>,
) {
    if has_duplicate(values) {
        diagnostics.push(PackageDiagnostic::new(
            id,
            format!("{label} values must be unique"),
        ));
    }
    for value in values {
        if !allowed.contains(&value.as_str()) {
            diagnostics.push(PackageDiagnostic::new(
                id,
                format!("unsupported {label}: {value}"),
            ));
        }
    }
}

fn validate_events(
    events: &[String],
    direction: &str,
    id: &str,
    diagnostics: &mut Vec<PackageDiagnostic>,
) {
    if has_duplicate(events) {
        diagnostics.push(PackageDiagnostic::new(
            id,
            format!("{direction} events must be unique"),
        ));
    }
    for event in events {
        if !is_event_name(event) {
            diagnostics.push(PackageDiagnostic::new(
                id,
                format!("invalid {direction} event: {event}"),
            ));
        }
    }
}

fn has_duplicate(values: &[String]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() != values.len()
}

fn is_namespaced_id(id: &str) -> bool {
    let Some((namespace, name)) = id.split_once('.') else {
        return false;
    };
    !namespace.is_empty()
        && !name.is_empty()
        && namespace.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn is_event_name(event: &str) -> bool {
    STANDARD_EVENTS.contains(&event)
        || event
            .strip_prefix("agent-context:")
            .is_some_and(is_kebab_token)
        || event
            .split_once(':')
            .is_some_and(|(namespace, name)| is_kebab_token(namespace) && is_kebab_token(name))
}

fn is_kebab_token(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

#[derive(Debug, PartialEq, Eq)]
pub enum BusOutcome {
    Routed,
    Rejected { warning: String },
    Dropped,
}

#[derive(Debug)]
pub struct PackageBus {
    development: bool,
    declarations: BTreeMap<String, BTreeSet<String>>,
}

impl PackageBus {
    pub fn new(development: bool, manifests: &[&PackageManifest]) -> Self {
        let declarations = manifests
            .iter()
            .map(|manifest| {
                (
                    manifest.id.clone(),
                    manifest.events.emits.iter().cloned().collect(),
                )
            })
            .collect();
        Self {
            development,
            declarations,
        }
    }

    /// Routes a declared event without inspecting its payload.
    pub fn emit(&self, package_id: &str, event: &str, _payload: &[u8]) -> BusOutcome {
        if self
            .declarations
            .get(package_id)
            .is_some_and(|events| events.contains(event))
        {
            BusOutcome::Routed
        } else if self.development {
            BusOutcome::Rejected {
                warning: format!("{package_id} attempted undeclared emit: {event}"),
            }
        } else {
            BusOutcome::Dropped
        }
    }
}
