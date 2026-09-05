use resonance_runtime::{
    conversations::{
        crypto::{EpochEnvelopeContextV1, MessageHeaderInputV1},
        testing,
        wire::{
            AcknowledgementV1, AddressNoticeV1, ChannelOperationV1, ChannelRecordV1,
            ConversationRecordV1, EpochRecipientV1, EpochRecordV1, ExactRecordV1,
            RecipientKeyRecordV1, RecoveryHeadV1, RecoveryRangeV1, RecoveryRequestV1,
            RecoveryResponseV1, EPOCH_ENVELOPE_SUITE_V1,
        },
    },
    identity::{InMemoryKeyCustody, InstallationIdentity},
};

pub fn identity(seed: u8) -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![seed; 32]))
        .expect("fixture identity")
}

pub fn records() -> Vec<(&'static str, ExactRecordV1)> {
    let signer = identity(7);
    let author = *signer.public_identity().as_bytes();
    let workspace = [1; 32];
    let epoch = [2; 32];
    let channel_id = [3; 16];
    let channel_head = [4; 32];

    let channel = ExactRecordV1::author(
        ConversationRecordV1::Channel(ChannelRecordV1 {
            workspace_id: workspace,
            channel_id,
            authorization_epoch: epoch,
            creator: author,
            author,
            author_sequence: 0,
            created_at: 1_788_555_000,
            predecessor: None,
            operation: ChannelOperationV1::Create {
                name: "#general".to_owned(),
            },
        }),
        &signer,
    )
    .expect("channel fixture");

    let epoch_key = testing::epoch_key([8; 32]);
    let message = testing::seal_message(
        &signer,
        &epoch_key,
        MessageHeaderInputV1 {
            workspace_id: workspace,
            channel_id,
            authorization_epoch: epoch,
            channel_head,
            author_sequence: 9,
            lamport: 17,
            created_at: 1_788_555_100,
        },
        "hello **direct** world",
        [9; 24],
    )
    .expect("message fixture");

    let (_private_a, public_a) = testing::recipient_key_pair([10; 32]);
    let recipient_key = ExactRecordV1::author(
        ConversationRecordV1::RecipientKey(RecipientKeyRecordV1 {
            workspace_id: workspace,
            installation: author,
            suite: EPOCH_ENVELOPE_SUITE_V1,
            generation: 1,
            recipient_public_key: *public_a.as_bytes(),
        }),
        &signer,
    )
    .expect("recipient key fixture");

    let second_identity = identity(11);
    let second_member = *second_identity.public_identity().as_bytes();
    let (_private_b, public_b) = testing::recipient_key_pair([12; 32]);
    let mut recipient_material = vec![
        (author, public_a, [13; 32], [14; 32]),
        (second_member, public_b, [15; 32], [16; 32]),
    ];
    recipient_material.sort_by_key(|entry| entry.0);
    let recipients = recipient_material
        .into_iter()
        .map(|(member, public, key_record_id, randomness)| {
            let context = EpochEnvelopeContextV1 {
                workspace_id: workspace,
                previous_membership_head: Some([17; 32]),
                resulting_membership_head: epoch,
                coordinator: author,
                recipient: member,
                recipient_public_key: public,
                recipient_key_record_id: key_record_id,
            };
            let envelope = testing::wrap_epoch_key(&epoch_key, &context, randomness)
                .expect("epoch envelope fixture");
            EpochRecipientV1 {
                member,
                recipient_key_record_id: key_record_id,
                encapsulation: envelope.encapsulation,
                wrapped_epoch_key: envelope.ciphertext,
            }
        })
        .collect();
    let epoch_record = ExactRecordV1::author(
        ConversationRecordV1::Epoch(EpochRecordV1 {
            workspace_id: workspace,
            previous_membership_head: Some([17; 32]),
            resulting_membership_head: epoch,
            coordinator: author,
            suite: EPOCH_ENVELOPE_SUITE_V1,
            recipients,
        }),
        &signer,
    )
    .expect("epoch fixture");

    let address = ExactRecordV1::author(
        ConversationRecordV1::AddressNotice(AddressNoticeV1 {
            workspace_id: workspace,
            sender: author,
            observed_membership_head: epoch,
            generation: 3,
            expires_at: 1_788_558_600,
            addresses: vec!["10.0.0.7:43000".to_owned(), "[fd00::7]:43000".to_owned()],
        }),
        &signer,
    )
    .expect("address fixture");

    let acknowledgement = ExactRecordV1::author(
        ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
            workspace_id: workspace,
            sender: author,
            record_ids: vec![[20; 32], [21; 32]],
        }),
        &signer,
    )
    .expect("acknowledgement fixture");

    let mut heads = vec![
        RecoveryHeadV1 {
            author,
            highest_sequence: 12,
        },
        RecoveryHeadV1 {
            author: second_member,
            highest_sequence: 7,
        },
    ];
    heads.sort_by_key(|head| head.author);
    let mut missing_ranges = vec![
        RecoveryRangeV1 {
            author,
            first_sequence: 3,
            last_sequence: 4,
        },
        RecoveryRangeV1 {
            author: second_member,
            first_sequence: 1,
            last_sequence: 2,
        },
    ];
    missing_ranges.sort();
    let recovery_request = ExactRecordV1::author(
        ConversationRecordV1::RecoveryRequest(RecoveryRequestV1 {
            workspace_id: workspace,
            sender: author,
            heads,
            missing_ranges,
        }),
        &signer,
    )
    .expect("recovery request fixture");

    let mut response_records = vec![channel.bytes().to_vec(), message.bytes().to_vec()];
    response_records.sort_by_key(|record| *blake3::hash(record).as_bytes());
    let recovery_response = ExactRecordV1::author(
        ConversationRecordV1::RecoveryResponse(RecoveryResponseV1 {
            workspace_id: workspace,
            sender: author,
            records: response_records,
        }),
        &signer,
    )
    .expect("recovery response fixture");

    vec![
        ("channel", channel),
        ("message", message),
        ("recipient-key", recipient_key),
        ("epoch", epoch_record),
        ("address-notice", address),
        ("acknowledgement", acknowledgement),
        ("recovery-request", recovery_request),
        ("recovery-response", recovery_response),
    ]
}
