//! Stable recipient-key material derived only inside the runtime custody boundary.

use crate::identity::InstallationIdentity;

use super::{
    crypto::{RecipientPrivateKey, RecipientPublicKey},
    wire::{
        ConversationRecordV1, ExactRecordV1, RecipientKeyRecordV1, WorkspaceId,
        EPOCH_ENVELOPE_SUITE_V1,
    },
    ConversationError,
};

#[derive(Clone)]
pub(crate) struct InstallationRecipientKey {
    private: RecipientPrivateKey,
    public: RecipientPublicKey,
}

impl InstallationRecipientKey {
    pub(crate) fn load(identity: &InstallationIdentity) -> Self {
        let (private, public) = RecipientPrivateKey::for_installation(identity);
        Self { private, public }
    }

    pub(crate) fn author_record(
        &self,
        identity: &InstallationIdentity,
        workspace_id: WorkspaceId,
    ) -> Result<ExactRecordV1, ConversationError> {
        ExactRecordV1::author(
            ConversationRecordV1::RecipientKey(RecipientKeyRecordV1 {
                workspace_id,
                installation: *identity.public_identity().as_bytes(),
                suite: EPOCH_ENVELOPE_SUITE_V1,
                generation: 0,
                recipient_public_key: *self.public.as_bytes(),
            }),
            identity,
        )
    }

    pub(crate) const fn private(&self) -> &RecipientPrivateKey {
        &self.private
    }
}
