//! Signed workspace-file operations and the authority that projects them.
//!
//! The authority is transport-independent: callers persist and replay these
//! authenticated records before projecting them to a local root.

pub mod authority;
pub mod blobs;
pub mod merge;
pub mod paths;
pub mod projection;

use std::fmt;

use iroh::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

use crate::identity::InstallationIdentity;

const FILE_OPERATION_DOMAIN: &[u8] = b"resonance.workspace-file-op.v1\0";
pub const FILE_OPERATION_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOperation {
    pub version: u8,
    pub workspace_id: String,
    pub operation_id: String,
    pub node_id: String,
    pub causal_parents: Vec<String>,
    pub signer: [u8; 32],
    pub body: FileOperationBody,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOperationBody {
    CreateDirectory {
        name: String,
    },
    CreateFile {
        parent_node_id: String,
        name: String,
        content_hash: String,
        mime_type: String,
        byte_length: u64,
    },
    ReplaceFileRevision {
        node_id: String,
        base_revision_id: String,
        content_hash: String,
        mime_type: String,
        byte_length: u64,
    },
    MoveNode {
        node_id: String,
        new_parent_node_id: String,
        new_name: String,
    },
    TombstoneNode {
        node_id: String,
    },
    ResolveConflict {
        conflict_record_id: String,
        chosen_revision_id: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedFileOperation {
    pub operation: FileOperation,
    pub signature: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FileOperationError {
    Encode,
    Decode,
    InvalidSignature,
    RandomnessUnavailable,
}

impl fmt::Display for FileOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => formatter.write_str("workspace file operation could not be encoded"),
            Self::Decode => formatter.write_str("workspace file operation could not be decoded"),
            Self::InvalidSignature => {
                formatter.write_str("workspace file operation signature is invalid")
            }
            Self::RandomnessUnavailable => formatter.write_str("secure randomness is unavailable"),
        }
    }
}

impl std::error::Error for FileOperationError {}

impl SignedFileOperation {
    pub fn create_directory(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, FileOperationError> {
        let operation = FileOperation {
            version: FILE_OPERATION_VERSION,
            workspace_id: workspace_id.into(),
            operation_id: random_id()?,
            node_id: random_id()?,
            causal_parents: Vec::new(),
            signer: *identity.public_identity().as_bytes(),
            body: FileOperationBody::CreateDirectory { name: name.into() },
        };
        let signature = identity.sign(&signing_bytes(&operation)?).to_vec();
        Ok(Self {
            operation,
            signature,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, FileOperationError> {
        postcard::to_stdvec(self).map_err(|_| FileOperationError::Encode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FileOperationError> {
        postcard::from_bytes(bytes).map_err(|_| FileOperationError::Decode)
    }

    pub fn verify(&self) -> Result<(), FileOperationError> {
        let signer = PublicKey::from_bytes(&self.operation.signer)
            .map_err(|_| FileOperationError::InvalidSignature)?;
        let signature: [u8; Signature::LENGTH] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| FileOperationError::InvalidSignature)?;
        signer
            .verify(
                &signing_bytes(&self.operation)?,
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| FileOperationError::InvalidSignature)
    }
}

fn signing_bytes(operation: &FileOperation) -> Result<Vec<u8>, FileOperationError> {
    let mut bytes = FILE_OPERATION_DOMAIN.to_vec();
    bytes.extend(postcard::to_stdvec(operation).map_err(|_| FileOperationError::Encode)?);
    Ok(bytes)
}

pub(crate) fn random_id() -> Result<String, FileOperationError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| FileOperationError::RandomnessUnavailable)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
