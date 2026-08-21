//! Signed, deterministic workspace invitations.

use std::fmt;

use iroh::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    identity::InstallationIdentity,
    workspace_domain::{WorkspaceId, WorkspaceToken},
};

const INVITE_DOMAIN: &[u8] = b"resonance.invite.v1\0";
pub const INVITE_PROTOCOL_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct UnsignedInvite {
    version: u8,
    workspace_id: String,
    workspace_token: [u8; 32],
    workspace_name: String,
    relay_override: Option<String>,
    inviter: [u8; 32],
    bootstrap: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SignedInvite {
    unsigned: UnsignedInvite,
    signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    unsigned: UnsignedInvite,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InviteError {
    Decode,
    Encode,
    InvalidVersion,
    InvalidWorkspace,
    InvalidName,
    InvalidRelay,
    InvalidBootstrap,
    InvalidSignature,
}

impl fmt::Display for InviteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode => formatter.write_str("invite is not valid base58 data"),
            Self::Encode => formatter.write_str("invite could not be encoded"),
            Self::InvalidVersion => formatter.write_str("invite protocol version is not supported"),
            Self::InvalidWorkspace => formatter.write_str("invite workspace data is invalid"),
            Self::InvalidName => formatter.write_str("invite workspace name is invalid"),
            Self::InvalidRelay => formatter.write_str("invite relay URL is invalid"),
            Self::InvalidBootstrap => formatter.write_str("invite bootstrap hint is invalid"),
            Self::InvalidSignature => formatter.write_str("invite signature is invalid"),
        }
    }
}

impl std::error::Error for InviteError {}

impl Invite {
    pub(crate) fn create(
        identity: &InstallationIdentity,
        token: &WorkspaceToken,
        workspace_name: impl Into<String>,
        relay_override: Option<String>,
        bootstrap: impl Into<String>,
    ) -> Result<String, InviteError> {
        let unsigned = UnsignedInvite {
            version: INVITE_PROTOCOL_VERSION,
            workspace_id: token.workspace_id().as_str().to_owned(),
            workspace_token: *token.as_bytes(),
            workspace_name: workspace_name.into(),
            relay_override,
            inviter: *identity.public_identity().as_bytes(),
            bootstrap: bootstrap.into(),
        };
        validate(&unsigned)?;
        let signature = identity.sign(&signing_bytes(&unsigned)?).to_vec();
        let encoded = postcard::to_stdvec(&SignedInvite {
            unsigned,
            signature,
        })
        .map_err(|_| InviteError::Encode)?;
        Ok(bs58::encode(encoded).into_string())
    }

    pub fn decode(encoded: &str) -> Result<Self, InviteError> {
        let bytes = bs58::decode(encoded)
            .into_vec()
            .map_err(|_| InviteError::Decode)?;
        let signed: SignedInvite = postcard::from_bytes(&bytes).map_err(|_| InviteError::Decode)?;
        validate(&signed.unsigned)?;
        let signer = PublicKey::from_bytes(&signed.unsigned.inviter)
            .map_err(|_| InviteError::InvalidSignature)?;
        let signature: [u8; Signature::LENGTH] = signed
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| InviteError::InvalidSignature)?;
        signer
            .verify(
                &signing_bytes(&signed.unsigned)?,
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| InviteError::InvalidSignature)?;
        Ok(Self {
            unsigned: signed.unsigned,
        })
    }

    #[must_use]
    pub fn workspace_id(&self) -> &str {
        &self.unsigned.workspace_id
    }

    #[must_use]
    pub fn inviter(&self) -> [u8; 32] {
        self.unsigned.inviter
    }

    #[must_use]
    pub fn bootstrap(&self) -> &str {
        &self.unsigned.bootstrap
    }

    pub(crate) fn workspace_token(&self) -> WorkspaceToken {
        WorkspaceToken::from_bytes(self.unsigned.workspace_token)
    }

    #[must_use]
    pub(crate) fn workspace_name(&self) -> &str {
        &self.unsigned.workspace_name
    }

    #[must_use]
    pub(crate) fn relay_override(&self) -> Option<&str> {
        self.unsigned.relay_override.as_deref()
    }
}

fn signing_bytes(invite: &UnsignedInvite) -> Result<Vec<u8>, InviteError> {
    let mut bytes = INVITE_DOMAIN.to_vec();
    bytes.extend(postcard::to_stdvec(invite).map_err(|_| InviteError::Encode)?);
    Ok(bytes)
}

fn validate(invite: &UnsignedInvite) -> Result<(), InviteError> {
    if invite.version != INVITE_PROTOCOL_VERSION {
        return Err(InviteError::InvalidVersion);
    }
    let workspace_id =
        WorkspaceId::parse(&invite.workspace_id).map_err(|_| InviteError::InvalidWorkspace)?;
    let token = WorkspaceToken::from_bytes(invite.workspace_token);
    if token.workspace_id() != workspace_id {
        return Err(InviteError::InvalidWorkspace);
    }
    if invite.workspace_name.trim().is_empty() || invite.workspace_name.len() > 256 {
        return Err(InviteError::InvalidName);
    }
    if let Some(relay) = &invite.relay_override {
        validate_relay_override(relay)?;
    }
    if invite.bootstrap.trim().is_empty() || invite.bootstrap.len() > 1024 {
        return Err(InviteError::InvalidBootstrap);
    }
    Ok(())
}

pub(crate) fn validate_relay_override(relay: &str) -> Result<(), InviteError> {
    let relay = Url::parse(relay).map_err(|_| InviteError::InvalidRelay)?;
    if !matches!(relay.scheme(), "https" | "http") || relay.host_str().is_none() {
        return Err(InviteError::InvalidRelay);
    }
    Ok(())
}
