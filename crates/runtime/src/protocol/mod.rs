//! Signed workspace-delivery envelopes. Transport adapters carry only these opaque bytes.

use std::fmt;

use iroh::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

use crate::{identity::InstallationIdentity, workspace_domain::WorkspaceId};

const ENVELOPE_DOMAIN: &[u8] = b"resonance.workspace-envelope.v1\0";
pub const ENVELOPE_PROTOCOL_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub version: u8,
    pub workspace_id: String,
    pub sender: [u8; 32],
    pub body: EnvelopeBody,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeBody {
    MembershipOperation(Vec<u8>),
    JoinRequest {
        inviter: [u8; 32],
        display_name: String,
        recipient_key: Vec<u8>,
    },
    MembershipSyncRequest,
    MembershipSyncResponse(Vec<Vec<u8>>),
    FileHistoryNotice {
        operation_ids: Vec<String>,
    },
    Heartbeat {
        sent_at: i64,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProtocolError {
    Decode,
    Encode,
    InvalidVersion,
    InvalidWorkspace,
    InvalidSignature,
    InvalidJoinRequest,
    InvalidFileHistoryNotice,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode => formatter.write_str("workspace envelope could not be decoded"),
            Self::Encode => formatter.write_str("workspace envelope could not be encoded"),
            Self::InvalidVersion => {
                formatter.write_str("workspace envelope version is not supported")
            }
            Self::InvalidWorkspace => {
                formatter.write_str("workspace envelope workspace ID is invalid")
            }
            Self::InvalidSignature => {
                formatter.write_str("workspace envelope signature is invalid")
            }
            Self::InvalidJoinRequest => formatter.write_str("workspace join request is invalid"),
            Self::InvalidFileHistoryNotice => {
                formatter.write_str("workspace file-history notice is invalid")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

impl Envelope {
    pub fn sign(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        body: EnvelopeBody,
    ) -> Result<Self, ProtocolError> {
        let unsigned = UnsignedEnvelope {
            version: ENVELOPE_PROTOCOL_VERSION,
            workspace_id: workspace_id.into(),
            sender: *identity.public_identity().as_bytes(),
            body,
        };
        validate_unsigned(&unsigned)?;
        Ok(Self {
            version: unsigned.version,
            workspace_id: unsigned.workspace_id.clone(),
            sender: unsigned.sender,
            body: unsigned.body.clone(),
            signature: identity.sign(&signing_bytes(&unsigned)?).to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        postcard::to_stdvec(self).map_err(|_| ProtocolError::Encode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        postcard::from_bytes(bytes).map_err(|_| ProtocolError::Decode)
    }

    pub fn workspace_id_value(&self) -> Result<WorkspaceId, ProtocolError> {
        WorkspaceId::parse(&self.workspace_id).map_err(|_| ProtocolError::InvalidWorkspace)
    }

    pub fn verify(&self) -> Result<(), ProtocolError> {
        let unsigned = UnsignedEnvelope {
            version: self.version,
            workspace_id: self.workspace_id.clone(),
            sender: self.sender,
            body: self.body.clone(),
        };
        validate_unsigned(&unsigned)?;
        let signer =
            PublicKey::from_bytes(&self.sender).map_err(|_| ProtocolError::InvalidSignature)?;
        let signature: [u8; Signature::LENGTH] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| ProtocolError::InvalidSignature)?;
        signer
            .verify(
                &signing_bytes(&unsigned)?,
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| ProtocolError::InvalidSignature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct UnsignedEnvelope {
    version: u8,
    workspace_id: String,
    sender: [u8; 32],
    body: EnvelopeBody,
}

fn signing_bytes(envelope: &UnsignedEnvelope) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = ENVELOPE_DOMAIN.to_vec();
    bytes.extend(postcard::to_stdvec(envelope).map_err(|_| ProtocolError::Encode)?);
    Ok(bytes)
}

fn validate_unsigned(envelope: &UnsignedEnvelope) -> Result<(), ProtocolError> {
    if envelope.version != ENVELOPE_PROTOCOL_VERSION {
        return Err(ProtocolError::InvalidVersion);
    }
    WorkspaceId::parse(&envelope.workspace_id).map_err(|_| ProtocolError::InvalidWorkspace)?;
    match &envelope.body {
        EnvelopeBody::JoinRequest {
            display_name,
            recipient_key,
            ..
        } if display_name.trim().is_empty()
            || display_name.len() > 256
            || recipient_key.len() > crate::conversations::wire::MAX_RECORD_BYTES =>
        {
            Err(ProtocolError::InvalidJoinRequest)
        }
        EnvelopeBody::FileHistoryNotice { operation_ids }
            if operation_ids.len() > crate::workspace_file_transport::MAX_OPERATION_RECORDS
                || operation_ids.iter().any(|id| {
                    id.len() != 32
                        || !id
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }) =>
        {
            Err(ProtocolError::InvalidFileHistoryNotice)
        }
        _ => Ok(()),
    }
}
