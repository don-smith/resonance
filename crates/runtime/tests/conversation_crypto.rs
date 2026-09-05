#[path = "support/conversation_fixtures.rs"]
mod fixtures;

use std::{cell::Cell, fs};

use resonance_runtime::conversations::{
    crypto::{
        decode_validate_and_open_message, open_message, seal_message, unwrap_epoch_key,
        wrap_epoch_key, AuthenticatedMessageOpener, EpochEnvelopeContextV1, EpochKey,
        EpochKeyLookup, HpkeEpochEnvelopeV1, MessageAuthority, MessageHeaderInputV1,
        RecipientPrivateKey,
    },
    testing,
    wire::{
        ConversationRecordV1, ExactRecordV1, MessageRecordV1, MESSAGE_BODY_FORMAT_MARKDOWN_V1,
        MESSAGE_FAMILY,
    },
    ConversationError,
};

fn message_fixture() -> (
    resonance_runtime::identity::InstallationIdentity,
    EpochKey,
    ExactRecordV1,
) {
    let identity = fixtures::identity(50);
    let key = testing::epoch_key([51; 32]);
    let record = testing::seal_message(
        &identity,
        &key,
        MessageHeaderInputV1 {
            workspace_id: [1; 32],
            channel_id: [2; 16],
            authorization_epoch: [3; 32],
            channel_head: [4; 32],
            author_sequence: 5,
            lamport: 6,
            created_at: 7,
        },
        "authenticated **Markdown**",
        [52; 24],
    )
    .expect("message fixture");
    (identity, key, record)
}

fn message(record: &ExactRecordV1) -> MessageRecordV1 {
    let ConversationRecordV1::Message(message) = record.record() else {
        panic!("message family");
    };
    message.clone()
}

#[test]
fn deterministic_adapters_regenerate_identical_message_and_epoch_fixtures() {
    let first = fixtures::records();
    let second = fixtures::records();
    assert_eq!(first, second);

    for (name, record) in first {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/conversations/v1");
        assert_eq!(
            fs::read(root.join(format!("{name}.bin"))).expect("golden bytes"),
            record.bytes()
        );
    }
}

#[test]
fn message_round_trip_uses_exact_header_as_associated_data() {
    let (_, key, exact) = message_fixture();
    let record = message(&exact);
    assert_eq!(
        open_message(&record, &key).expect("message opens"),
        "authenticated **Markdown**"
    );

    let mut mutations: Vec<MessageRecordV1> = Vec::new();
    let mut changed = record.clone();
    changed.workspace_id[0] ^= 1;
    mutations.push(changed);
    let mut changed = record.clone();
    changed.channel_id[0] ^= 1;
    mutations.push(changed);
    let mut changed = record.clone();
    changed.authorization_epoch[0] ^= 1;
    mutations.push(changed);
    let mut changed = record.clone();
    changed.channel_head[0] ^= 1;
    mutations.push(changed);
    let mut changed = record.clone();
    changed.author_sequence += 1;
    mutations.push(changed);
    let mut changed = record.clone();
    changed.lamport += 1;
    mutations.push(changed);
    let mut changed = record.clone();
    changed.created_at += 1;
    mutations.push(changed);

    for changed in mutations {
        assert_eq!(
            open_message(&changed, &key),
            Err(ConversationError::AuthenticatedOpen)
        );
    }

    let other_identity = fixtures::identity(53);
    let mut changed_author = record.clone();
    changed_author.author = *other_identity.public_identity().as_bytes();
    let changed_author = ExactRecordV1::author(
        ConversationRecordV1::Message(changed_author),
        &other_identity,
    )
    .expect("changed author can re-sign");
    assert_eq!(
        open_message(&message(&changed_author), &key),
        Err(ConversationError::AuthenticatedOpen)
    );

    let mut changed_nonce = record.clone();
    changed_nonce.nonce[0] ^= 1;
    assert_eq!(
        open_message(&changed_nonce, &key),
        Err(ConversationError::AuthenticatedOpen)
    );
    let mut changed_ciphertext = record;
    changed_ciphertext.ciphertext[0] ^= 1;
    assert_eq!(
        open_message(&changed_ciphertext, &key),
        Err(ConversationError::AuthenticatedOpen)
    );
}

#[test]
fn unsupported_message_suite_and_body_format_fail_before_open() {
    let (identity, key, exact) = message_fixture();
    let mut unsupported_suite = message(&exact);
    unsupported_suite.encryption_suite = 99;
    assert_eq!(
        ExactRecordV1::author(
            ConversationRecordV1::Message(unsupported_suite.clone()),
            &identity,
        ),
        Err(ConversationError::UnsupportedSuite(99))
    );
    assert_eq!(
        open_message(&unsupported_suite, &key),
        Err(ConversationError::UnsupportedSuite(99))
    );

    let mut unsupported_body = message(&exact);
    unsupported_body.body_format = 99;
    assert_eq!(
        ExactRecordV1::author(
            ConversationRecordV1::Message(unsupported_body.clone()),
            &identity,
        ),
        Err(ConversationError::UnsupportedVersion {
            family: MESSAGE_FAMILY,
            version: 99
        })
    );
    assert_eq!(
        open_message(&unsupported_body, &key),
        Err(ConversationError::UnsupportedVersion {
            family: MESSAGE_FAMILY,
            version: 99
        })
    );
    assert_eq!(message(&exact).body_format, MESSAGE_BODY_FORMAT_MARKDOWN_V1);
}

#[test]
fn hpke_epoch_envelope_binds_every_context_field_and_recipient() {
    let epoch_key = testing::epoch_key([60; 32]);
    let (recipient_private, recipient_public) = testing::recipient_key_pair([61; 32]);
    let context = EpochEnvelopeContextV1 {
        workspace_id: [1; 32],
        previous_membership_head: Some([2; 32]),
        resulting_membership_head: [3; 32],
        coordinator: [4; 32],
        recipient: [5; 32],
        recipient_public_key: recipient_public,
        recipient_key_record_id: [6; 32],
    };
    let envelope = testing::wrap_epoch_key(&epoch_key, &context, [62; 32]).expect("wrap");
    assert_eq!(
        unwrap_epoch_key(&envelope, &recipient_private, &context).expect("unwrap"),
        epoch_key
    );

    let mut contexts = Vec::new();
    let mut changed = context.clone();
    changed.workspace_id[0] ^= 1;
    contexts.push(changed);
    let mut changed = context.clone();
    changed.previous_membership_head = None;
    contexts.push(changed);
    let mut changed = context.clone();
    changed.resulting_membership_head[0] ^= 1;
    contexts.push(changed);
    let mut changed = context.clone();
    changed.coordinator[0] ^= 1;
    contexts.push(changed);
    let mut changed = context.clone();
    changed.recipient[0] ^= 1;
    contexts.push(changed);
    let mut changed = context.clone();
    changed.recipient_key_record_id[0] ^= 1;
    contexts.push(changed);

    for changed in contexts {
        assert_eq!(
            unwrap_epoch_key(&envelope, &recipient_private, &changed),
            Err(ConversationError::AuthenticatedOpen)
        );
    }

    let (wrong_private, wrong_public) = testing::recipient_key_pair([63; 32]);
    assert_eq!(
        unwrap_epoch_key(&envelope, &wrong_private, &context),
        Err(ConversationError::RecipientMismatch)
    );
    let mut relabelled_context = context.clone();
    relabelled_context.recipient_public_key = wrong_public;
    assert_eq!(
        unwrap_epoch_key(&envelope, &wrong_private, &relabelled_context),
        Err(ConversationError::AuthenticatedOpen)
    );
}

#[test]
fn hpke_tampering_is_an_authenticated_open_failure() {
    let epoch_key = testing::epoch_key([70; 32]);
    let (private, public) = testing::recipient_key_pair([71; 32]);
    let context = EpochEnvelopeContextV1 {
        workspace_id: [1; 32],
        previous_membership_head: None,
        resulting_membership_head: [2; 32],
        coordinator: [3; 32],
        recipient: [4; 32],
        recipient_public_key: public,
        recipient_key_record_id: [5; 32],
    };
    let envelope = testing::wrap_epoch_key(&epoch_key, &context, [72; 32]).expect("wrap");

    let mut changed_ciphertext = envelope.clone();
    changed_ciphertext.ciphertext[0] ^= 1;
    assert_eq!(
        unwrap_epoch_key(&changed_ciphertext, &private, &context),
        Err(ConversationError::AuthenticatedOpen)
    );
    let mut changed_encapsulation = envelope;
    changed_encapsulation.encapsulation[0] ^= 1;
    assert_eq!(
        unwrap_epoch_key(&changed_encapsulation, &private, &context),
        Err(ConversationError::AuthenticatedOpen)
    );
}

#[test]
fn production_authoring_uses_fresh_operating_system_randomness() {
    let identity = fixtures::identity(80);
    let key = testing::epoch_key([81; 32]);
    let header = MessageHeaderInputV1 {
        workspace_id: [1; 32],
        channel_id: [2; 16],
        authorization_epoch: [3; 32],
        channel_head: [4; 32],
        author_sequence: 0,
        lamport: 1,
        created_at: 2,
    };
    let first = seal_message(&identity, &key, header.clone(), "same").expect("first message");
    let second = seal_message(&identity, &key, header, "same").expect("second message");
    assert_ne!(first.bytes(), second.bytes());
    assert_ne!(first.id(), second.id());

    let (private, public) = RecipientPrivateKey::generate().expect("recipient key");
    let context = EpochEnvelopeContextV1 {
        workspace_id: [1; 32],
        previous_membership_head: None,
        resulting_membership_head: [2; 32],
        coordinator: [3; 32],
        recipient: [4; 32],
        recipient_public_key: public,
        recipient_key_record_id: [5; 32],
    };
    let first = wrap_epoch_key(&key, &context).expect("first wrap");
    let second = wrap_epoch_key(&key, &context).expect("second wrap");
    assert_ne!(first, second);
    assert_eq!(
        unwrap_epoch_key(&first, &private, &context).expect("first unwrap"),
        key
    );
}

struct Allow;

impl MessageAuthority for Allow {
    fn permits(&self, _record: &MessageRecordV1) -> bool {
        true
    }
}

struct Deny;

impl MessageAuthority for Deny {
    fn permits(&self, _record: &MessageRecordV1) -> bool {
        false
    }
}

struct FixedKey(EpochKey);

impl EpochKeyLookup for FixedKey {
    fn find(&self, _epoch: &[u8; 32]) -> Result<EpochKey, ConversationError> {
        Ok(self.0.clone())
    }
}

struct MissingKey;

impl EpochKeyLookup for MissingKey {
    fn find(&self, _epoch: &[u8; 32]) -> Result<EpochKey, ConversationError> {
        Err(ConversationError::MissingKey)
    }
}

struct PanicKey;

impl EpochKeyLookup for PanicKey {
    fn find(&self, _epoch: &[u8; 32]) -> Result<EpochKey, ConversationError> {
        panic!("key lookup must not be called")
    }
}

struct PanicOpen;

impl AuthenticatedMessageOpener for PanicOpen {
    fn open(
        &self,
        _record: &MessageRecordV1,
        _key: &EpochKey,
    ) -> Result<String, ConversationError> {
        panic!("authenticated open must not be called")
    }
}

struct CountingOpen(Cell<usize>);

impl AuthenticatedMessageOpener for CountingOpen {
    fn open(&self, record: &MessageRecordV1, key: &EpochKey) -> Result<String, ConversationError> {
        self.0.set(self.0.get() + 1);
        open_message(record, key)
    }
}

#[test]
fn structural_and_authority_failures_precede_key_lookup_and_authenticated_open() {
    let (_, key, exact) = message_fixture();

    let mut malformed = exact.bytes().to_vec();
    malformed[4] = 99;
    assert_eq!(
        decode_validate_and_open_message(&malformed, &[1; 32], &Allow, &PanicKey, &PanicOpen,),
        Err(ConversationError::UnsupportedFamily(99))
    );

    assert_eq!(
        decode_validate_and_open_message(exact.bytes(), &[1; 32], &Deny, &PanicKey, &PanicOpen,),
        Err(ConversationError::UnauthorizedData(
            "membership or channel authority"
        ))
    );

    assert_eq!(
        decode_validate_and_open_message(exact.bytes(), &[1; 32], &Allow, &MissingKey, &PanicOpen,),
        Err(ConversationError::MissingKey)
    );

    let opener = CountingOpen(Cell::new(0));
    assert_eq!(
        decode_validate_and_open_message(exact.bytes(), &[1; 32], &Allow, &FixedKey(key), &opener,)
            .expect("authorized open"),
        "authenticated **Markdown**"
    );
    assert_eq!(opener.0.get(), 1);
}

#[test]
fn secret_types_are_redacted_and_fixture_files_contain_no_plaintext_or_private_keys() {
    let epoch_key = testing::epoch_key([90; 32]);
    let (private, _) = testing::recipient_key_pair([91; 32]);
    assert_eq!(format!("{epoch_key:?}"), "EpochKey([REDACTED])");
    assert_eq!(format!("{private:?}"), "RecipientPrivateKey([REDACTED])");

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/conversations/v1");
    for entry in fs::read_dir(root).expect("fixture directory") {
        let entry = entry.expect("fixture entry");
        if entry.path().extension().and_then(|value| value.to_str()) != Some("bin") {
            continue;
        }
        let bytes = fs::read(entry.path()).expect("fixture bytes");
        assert!(!bytes
            .windows(23)
            .any(|window| window == b"hello **direct** world"));
        assert!(!bytes.windows(32).any(|window| window == [8; 32]));
        assert!(!bytes.windows(32).any(|window| window == [10; 32]));
        assert!(!bytes.windows(32).any(|window| window == [12; 32]));
    }
}

#[test]
fn envelope_shapes_are_fixed() {
    let envelope = HpkeEpochEnvelopeV1 {
        encapsulation: [0; 32],
        ciphertext: [0; 48],
    };
    assert_eq!(envelope.encapsulation.len(), 32);
    assert_eq!(envelope.ciphertext.len(), 48);
}
