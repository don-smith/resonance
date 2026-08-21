//! Secret-free workspace domain values and runtime-private workspace tokens.

use std::fmt;

const WORKSPACE_ID_DOMAIN: &[u8] = b"resonance.workspace-id.v1\0";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn parse(value: &str) -> Result<Self, WorkspaceDomainError> {
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(WorkspaceDomainError::InvalidWorkspaceId)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceLifecycle {
    Ready,
    Joining,
}

impl WorkspaceLifecycle {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Joining => "joining",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, WorkspaceDomainError> {
        match value {
            "ready" => Ok(Self::Ready),
            "joining" => Ok(Self::Joining),
            _ => Err(WorkspaceDomainError::InvalidLifecycle),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub id: WorkspaceId,
    pub display_name: String,
    pub lifecycle: WorkspaceLifecycle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub public_identity: String,
    pub display_name: String,
    pub role: String,
    pub added_by: String,
    pub added_at: i64,
}

impl Member {
    #[must_use]
    pub fn new(
        public_identity: impl Into<String>,
        display_name: impl Into<String>,
        role: impl Into<String>,
        added_by: impl Into<String>,
        added_at: i64,
    ) -> Self {
        Self {
            public_identity: public_identity.into(),
            display_name: display_name.into(),
            role: role.into(),
            added_by: added_by.into(),
            added_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSettings {
    pub display_name: String,
    pub relay_override: Option<String>,
    pub lifecycle: WorkspaceLifecycle,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorkspaceDomainError {
    InvalidWorkspaceId,
    InvalidLifecycle,
    RandomnessUnavailable,
}

impl fmt::Display for WorkspaceDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorkspaceId => formatter.write_str("invalid workspace ID"),
            Self::InvalidLifecycle => formatter.write_str("invalid workspace lifecycle"),
            Self::RandomnessUnavailable => formatter.write_str("secure randomness is unavailable"),
        }
    }
}

impl std::error::Error for WorkspaceDomainError {}

#[derive(Clone)]
pub(crate) struct WorkspaceToken([u8; 32]);

impl WorkspaceToken {
    pub(crate) fn generate() -> Result<Self, WorkspaceDomainError> {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| WorkspaceDomainError::RandomnessUnavailable)?;
        Ok(Self(bytes))
    }

    pub(crate) fn workspace_id(&self) -> WorkspaceId {
        let mut digest = blake3::Hasher::new();
        digest.update(WORKSPACE_ID_DOMAIN);
        digest.update(&self.0);
        WorkspaceId(digest.finalize().to_hex().to_string())
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
