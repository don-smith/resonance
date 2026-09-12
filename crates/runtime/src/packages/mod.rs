//! Package-manifest and declared-event runtime seams.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

mod manifest_generated;

pub use manifest_generated::{
    AgentConfiguration, AgentPermission, ContentEntry, EventDeclarations, ManifestRole, Navigation,
    PackageManifest, PackageSourceValue, SemanticCapability, MANIFEST_SCHEMA_SHA256,
};
const STANDARD_EVENTS: [&str; 10] = [
    "repo:changed",
    "doc:updated",
    "doc:opened",
    "message:received",
    "peer:joined",
    "peer:left",
    "peer:connection",
    "workspace:member-added",
    "workspace:member-removed",
    "conversations:changed",
];

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
    Bundled,
    MemberLocal,
    Repository,
}

#[derive(Debug)]
pub struct PackageRegistry {
    manifests: BTreeMap<String, PackageManifest>,
}

impl PackageRegistry {
    /// Parses and validates bundled manifests. Diagnostics are sorted so
    /// callers can report reproducible remediation to package authors.
    pub fn load(
        source: PackageSource,
        raw_manifests: &[&str],
    ) -> Result<Self, Vec<PackageDiagnostic>> {
        if source != PackageSource::Bundled {
            return Err(vec![PackageDiagnostic::new(
                "<source>",
                "only bundled packages may load from the bundled catalog",
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

    pub fn load_catalog(
        source: PackageSource,
        raw_catalog: &str,
    ) -> Result<Self, Vec<PackageDiagnostic>> {
        let values: Vec<serde_json::Value> =
            serde_json::from_str(raw_catalog).map_err(|error| {
                vec![PackageDiagnostic::new(
                    "<catalog>",
                    format!("malformed package catalog: {error}"),
                )]
            })?;
        let manifests = values
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let raw_manifests = manifests.iter().map(String::as_str).collect::<Vec<_>>();
        Self::load(source, &raw_manifests)
    }

    pub fn get(&self, id: &str) -> Option<&PackageManifest> {
        self.manifests.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.manifests.keys().map(String::as_str)
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

        if manifest.manifest_version != 2 {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "manifestVersion must be 2",
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
        if !is_relative_resource(&manifest.content.entry, ".ts") {
            diagnostics.push(PackageDiagnostic::new(
                id.clone(),
                "content entry must be a package-relative TypeScript path without traversal",
            ));
        }
        validate_events(&manifest.events.emits, "emitted", &id, &mut diagnostics);
        validate_events(&manifest.events.consumes, "consumed", &id, &mut diagnostics);
        if let Some(agent) = &manifest.agent {
            if !is_relative_resource(&agent.system_prompt, ".md") {
                diagnostics.push(PackageDiagnostic::new(
                    id.clone(),
                    "agent systemPrompt must be a package-relative Markdown path without traversal",
                ));
            }
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

fn is_relative_resource(path: &str, extension: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path.ends_with(extension)
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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
