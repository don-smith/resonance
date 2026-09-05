#[path = "support/conversation_fixtures.rs"]
mod fixtures;

use std::{fs, path::PathBuf};

use commonware_codec::{varint::UInt, Encode as _};
use resonance_runtime::conversations::{
    testing,
    wire::{
        AcknowledgementV1, AddressNoticeV1, ChannelOperationV1, ChannelRecordV1,
        ConversationRecordV1, EpochRecipientV1, EpochRecordV1, ExactRecordV1, MessageRecordV1,
        RecoveryHeadV1, RecoveryRangeV1, RecoveryRequestV1, RecoveryResponseV1,
        EPOCH_ENVELOPE_SUITE_V1, FAMILY_MARKER, FORMAT_VERSION_V1, MAX_ACKNOWLEDGEMENT_ITEMS,
        MAX_ADDRESSES_PER_NOTICE, MAX_ADDRESS_BYTES, MAX_CHANNEL_NAME_BYTES, MAX_FRAME_BYTES,
        MAX_MARKDOWN_BYTES, MAX_MEMBERS_PER_EPOCH, MAX_MESSAGE_CIPHERTEXT_BYTES, MAX_RECORD_BYTES,
        MAX_RECOVERY_HEADS, MAX_RECOVERY_RANGES, MAX_RECOVERY_RECORD_BYTES,
        MAX_RECOVERY_RESPONSE_RECORDS, MESSAGE_BODY_FORMAT_MARKDOWN_V1,
        MESSAGE_ENCRYPTION_SUITE_V1,
    },
    ConversationError,
};

const _: () = assert!(MAX_FRAME_BYTES > MAX_RECORD_BYTES);

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conversations/v1")
}

fn fixture_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_or_assert_fixture(name: &str, record: &ExactRecordV1) {
    let root = fixture_root();
    let bytes_path = root.join(format!("{name}.bin"));
    let id_path = root.join(format!("{name}.id"));
    if std::env::var_os("UPDATE_CONVERSATION_FIXTURES").is_some() {
        fs::create_dir_all(&root).expect("fixture directory");
        fs::write(&bytes_path, record.bytes()).expect("write fixture bytes");
        fs::write(&id_path, format!("{}\n", fixture_hex(record.id()))).expect("write fixture id");
    }
    assert_eq!(
        fs::read(&bytes_path).expect("fixture bytes"),
        record.bytes()
    );
    assert_eq!(
        fs::read_to_string(&id_path).expect("fixture id"),
        format!("{}\n", fixture_hex(record.id()))
    );
}

fn write_or_assert_invalid(name: &str, bytes: &[u8]) {
    let path = fixture_root().join(format!("invalid-{name}.bin"));
    if std::env::var_os("UPDATE_CONVERSATION_FIXTURES").is_some() {
        fs::create_dir_all(fixture_root()).expect("fixture directory");
        fs::write(&path, bytes).expect("write invalid fixture");
    }
    let fixture = fs::read(path).expect("invalid fixture");
    assert_eq!(fixture, bytes);
    assert!(ExactRecordV1::decode(&fixture).is_err());
}

#[test]
fn every_v1_family_has_stable_exact_bytes_id_and_round_trip() {
    for (name, record) in fixtures::records() {
        write_or_assert_fixture(name, &record);
        let decoded = ExactRecordV1::decode(record.bytes()).expect("golden record decodes");
        assert_eq!(decoded, record);
        assert_eq!(decoded.bytes(), record.bytes());
        assert_eq!(decoded.id(), blake3::hash(record.bytes()).as_bytes());
    }
}

#[test]
fn checked_invalid_fixtures_cover_family_version_integer_extent_utf8_and_ordering() {
    let records = fixtures::records();
    let channel = records
        .iter()
        .find(|(name, _)| *name == "channel")
        .expect("channel")
        .1
        .bytes()
        .to_vec();

    let mut unknown_family = channel.clone();
    unknown_family[4] = 99;
    write_or_assert_invalid("unknown-family", &unknown_family);

    let mut unknown_version = channel.clone();
    unknown_version[5] = 99;
    write_or_assert_invalid("unknown-version", &unknown_version);

    let mut nonminimal_length = channel.clone();
    let last_length_byte = (6..)
        .find(|index| nonminimal_length[*index] & 0x80 == 0)
        .expect("payload length terminates");
    nonminimal_length[last_length_byte] |= 0x80;
    nonminimal_length.insert(last_length_byte + 1, 0);
    write_or_assert_invalid("nonminimal-integer", &nonminimal_length);

    write_or_assert_invalid("truncated", &channel[..channel.len() - 1]);
    let mut trailing = channel.clone();
    trailing.push(0);
    write_or_assert_invalid("trailing", &trailing);

    let mut invalid_utf8 = channel.clone();
    let name_offset = invalid_utf8
        .windows(b"#general".len())
        .position(|window| window == b"#general")
        .expect("channel name in fixture");
    invalid_utf8[name_offset] = 0xff;
    write_or_assert_invalid("utf8", &invalid_utf8);

    let (payload_start, payload_end) = outer_payload_bounds(&channel);
    let mut oversized_payload = channel[payload_start..payload_end].to_vec();
    let name_in_payload = name_offset - payload_start;
    oversized_payload[name_in_payload - 1] = (MAX_CHANNEL_NAME_BYTES + 1) as u8;
    oversized_payload.splice(
        name_in_payload..name_in_payload + b"#general".len(),
        vec![b'x'; MAX_CHANNEL_NAME_BYTES + 1],
    );
    let mut oversized_field = channel[..6].to_vec();
    oversized_field.extend_from_slice(UInt(oversized_payload.len() as u64).encode().as_ref());
    oversized_field.extend_from_slice(&oversized_payload);
    oversized_field.extend_from_slice(&channel[channel.len() - 64..]);
    write_or_assert_invalid("bound", &oversized_field);

    let epoch = records
        .iter()
        .find(|(name, _)| *name == "epoch")
        .expect("epoch")
        .1
        .clone();
    let ConversationRecordV1::Epoch(epoch_value) = epoch.record() else {
        panic!("epoch fixture family");
    };
    let first_needle = [
        epoch_value.recipients[0].member.as_slice(),
        epoch_value.recipients[0].recipient_key_record_id.as_slice(),
    ]
    .concat();
    let second_needle = [
        epoch_value.recipients[1].member.as_slice(),
        epoch_value.recipients[1].recipient_key_record_id.as_slice(),
    ]
    .concat();
    let first = epoch
        .bytes()
        .windows(first_needle.len())
        .position(|window| window == first_needle)
        .expect("first recipient offset");
    let second = epoch
        .bytes()
        .windows(second_needle.len())
        .position(|window| window == second_needle)
        .expect("second recipient offset");
    let mut invalid_order = epoch.bytes().to_vec();
    let first_entry = invalid_order[first..first + 144].to_vec();
    let second_entry = invalid_order[second..second + 144].to_vec();
    invalid_order[first..first + 144].copy_from_slice(&second_entry);
    invalid_order[second..second + 144].copy_from_slice(&first_entry);
    write_or_assert_invalid("ordering", &invalid_order);
}

#[test]
fn changing_any_signed_unsigned_byte_is_rejected() {
    for (_, record) in fixtures::records() {
        let signed_extent = record.bytes().len() - 64;
        for index in 0..signed_extent {
            let mut changed = record.bytes().to_vec();
            changed[index] ^= 1;
            assert!(
                ExactRecordV1::decode(&changed).is_err(),
                "authenticated byte {index} was accepted for family {}",
                record.record().family()
            );
        }
    }
}

#[test]
fn protocol_limits_accept_edges_and_reject_one_past_each_bound() {
    let signer = fixtures::identity(31);
    let author = *signer.public_identity().as_bytes();

    let channel = |name: String| {
        ExactRecordV1::author(
            ConversationRecordV1::Channel(ChannelRecordV1 {
                workspace_id: [1; 32],
                channel_id: [2; 16],
                authorization_epoch: [3; 32],
                creator: author,
                author,
                author_sequence: 0,
                created_at: 0,
                predecessor: None,
                operation: ChannelOperationV1::Create { name },
            }),
            &signer,
        )
    };
    assert!(channel("c".repeat(MAX_CHANNEL_NAME_BYTES)).is_ok());
    assert!(matches!(
        channel("c".repeat(MAX_CHANNEL_NAME_BYTES + 1)),
        Err(ConversationError::SizeLimit { .. })
    ));

    let key = testing::epoch_key([32; 32]);
    let header = resonance_runtime::conversations::crypto::MessageHeaderInputV1 {
        workspace_id: [1; 32],
        channel_id: [2; 16],
        authorization_epoch: [3; 32],
        channel_head: [4; 32],
        author_sequence: 0,
        lamport: 0,
        created_at: 0,
    };
    assert!(testing::seal_message(
        &signer,
        &key,
        header.clone(),
        &"m".repeat(MAX_MARKDOWN_BYTES),
        [0; 24],
    )
    .is_ok());
    assert!(matches!(
        testing::seal_message(
            &signer,
            &key,
            header,
            &"m".repeat(MAX_MARKDOWN_BYTES + 1),
            [0; 24],
        ),
        Err(ConversationError::SizeLimit { .. })
    ));

    let message_ciphertext = |length: usize| {
        ExactRecordV1::author(
            ConversationRecordV1::Message(MessageRecordV1 {
                workspace_id: [1; 32],
                channel_id: [2; 16],
                authorization_epoch: [3; 32],
                channel_head: [4; 32],
                author,
                author_sequence: 0,
                lamport: 0,
                created_at: 0,
                encryption_suite: MESSAGE_ENCRYPTION_SUITE_V1,
                body_format: MESSAGE_BODY_FORMAT_MARKDOWN_V1,
                nonce: [0; 24],
                ciphertext: vec![0; length],
            }),
            &signer,
        )
    };
    assert!(message_ciphertext(MAX_MESSAGE_CIPHERTEXT_BYTES).is_ok());
    assert!(matches!(
        message_ciphertext(MAX_MESSAGE_CIPHERTEXT_BYTES + 1),
        Err(ConversationError::SizeLimit { .. })
    ));

    let epoch_record = |count: usize| {
        let recipients = (0..count)
            .map(|index| EpochRecipientV1 {
                member: ordered_id(index),
                recipient_key_record_id: [5; 32],
                encapsulation: [6; 32],
                wrapped_epoch_key: [7; 48],
            })
            .collect();
        ExactRecordV1::author(
            ConversationRecordV1::Epoch(EpochRecordV1 {
                workspace_id: [1; 32],
                previous_membership_head: None,
                resulting_membership_head: [3; 32],
                coordinator: author,
                suite: EPOCH_ENVELOPE_SUITE_V1,
                recipients,
            }),
            &signer,
        )
    };
    assert!(epoch_record(MAX_MEMBERS_PER_EPOCH).is_ok());
    assert!(matches!(
        epoch_record(MAX_MEMBERS_PER_EPOCH + 1),
        Err(ConversationError::SizeLimit { .. })
    ));

    let address_record = |count: usize| {
        ExactRecordV1::author(
            ConversationRecordV1::AddressNotice(AddressNoticeV1 {
                workspace_id: [1; 32],
                sender: author,
                observed_membership_head: [3; 32],
                generation: 0,
                expires_at: 0,
                addresses: (0..count).map(|index| format!("addr-{index:03}")).collect(),
            }),
            &signer,
        )
    };
    assert!(address_record(MAX_ADDRESSES_PER_NOTICE).is_ok());
    assert!(matches!(
        address_record(MAX_ADDRESSES_PER_NOTICE + 1),
        Err(ConversationError::SizeLimit { .. })
    ));
    let address_bytes = |length: usize| {
        ExactRecordV1::author(
            ConversationRecordV1::AddressNotice(AddressNoticeV1 {
                workspace_id: [1; 32],
                sender: author,
                observed_membership_head: [3; 32],
                generation: 0,
                expires_at: 0,
                addresses: vec!["a".repeat(length)],
            }),
            &signer,
        )
    };
    assert!(address_bytes(MAX_ADDRESS_BYTES).is_ok());
    assert!(matches!(
        address_bytes(MAX_ADDRESS_BYTES + 1),
        Err(ConversationError::SizeLimit { .. })
    ));

    let acknowledgement = |count: usize| {
        ExactRecordV1::author(
            ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
                workspace_id: [1; 32],
                sender: author,
                record_ids: (0..count).map(ordered_id).collect(),
            }),
            &signer,
        )
    };
    assert!(acknowledgement(MAX_ACKNOWLEDGEMENT_ITEMS).is_ok());
    assert!(matches!(
        acknowledgement(MAX_ACKNOWLEDGEMENT_ITEMS + 1),
        Err(ConversationError::SizeLimit { .. })
    ));

    let recovery_request = |heads: usize, ranges: usize| {
        ExactRecordV1::author(
            ConversationRecordV1::RecoveryRequest(RecoveryRequestV1 {
                workspace_id: [1; 32],
                sender: author,
                heads: (0..heads)
                    .map(|index| RecoveryHeadV1 {
                        author: ordered_id(index),
                        highest_sequence: index as u64,
                    })
                    .collect(),
                missing_ranges: (0..ranges)
                    .map(|index| RecoveryRangeV1 {
                        author: ordered_id(index),
                        first_sequence: 1,
                        last_sequence: 2,
                    })
                    .collect(),
            }),
            &signer,
        )
    };
    assert!(recovery_request(MAX_RECOVERY_HEADS, MAX_RECOVERY_RANGES).is_ok());
    assert!(matches!(
        recovery_request(MAX_RECOVERY_HEADS + 1, 0),
        Err(ConversationError::SizeLimit { .. })
    ));
    assert!(matches!(
        recovery_request(0, MAX_RECOVERY_RANGES + 1),
        Err(ConversationError::SizeLimit { .. })
    ));

    let recovery_response = |count: usize| {
        let mut records: Vec<Vec<u8>> = (0..count)
            .map(|index| (index as u16).to_be_bytes().to_vec())
            .collect();
        records.sort_by_key(|record| *blake3::hash(record).as_bytes());
        ExactRecordV1::author(
            ConversationRecordV1::RecoveryResponse(RecoveryResponseV1 {
                workspace_id: [1; 32],
                sender: author,
                records,
            }),
            &signer,
        )
    };
    assert!(recovery_response(MAX_RECOVERY_RESPONSE_RECORDS).is_ok());
    assert!(matches!(
        recovery_response(MAX_RECOVERY_RESPONSE_RECORDS + 1),
        Err(ConversationError::SizeLimit { .. })
    ));
    let recovery_record_bytes = |length: usize| {
        ExactRecordV1::author(
            ConversationRecordV1::RecoveryResponse(RecoveryResponseV1 {
                workspace_id: [1; 32],
                sender: author,
                records: vec![vec![0; length]],
            }),
            &signer,
        )
    };
    assert!(recovery_record_bytes(MAX_RECOVERY_RECORD_BYTES).is_ok());
    assert!(matches!(
        recovery_record_bytes(MAX_RECOVERY_RECORD_BYTES + 1),
        Err(ConversationError::SizeLimit { .. })
    ));

    assert!(matches!(
        ExactRecordV1::decode(&vec![0; MAX_RECORD_BYTES + 1]),
        Err(ConversationError::SizeLimit { .. })
    ));
}

#[test]
fn authoring_rejects_ordering_reversed_ranges_and_wrong_signer() {
    let signer = fixtures::identity(40);
    let wrong = fixtures::identity(41);
    let author = *signer.public_identity().as_bytes();

    let unsorted = ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
        workspace_id: [1; 32],
        sender: author,
        record_ids: vec![[2; 32], [1; 32]],
    });
    assert!(matches!(
        ExactRecordV1::author(unsorted, &signer),
        Err(ConversationError::MalformedBytes(_))
    ));

    let reversed = ConversationRecordV1::RecoveryRequest(RecoveryRequestV1 {
        workspace_id: [1; 32],
        sender: author,
        heads: Vec::new(),
        missing_ranges: vec![RecoveryRangeV1 {
            author,
            first_sequence: 5,
            last_sequence: 4,
        }],
    });
    assert!(matches!(
        ExactRecordV1::author(reversed, &signer),
        Err(ConversationError::MalformedBytes(_))
    ));

    let wrong_signer = ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
        workspace_id: [1; 32],
        sender: author,
        record_ids: Vec::new(),
    });
    assert_eq!(
        ExactRecordV1::author(wrong_signer, &wrong),
        Err(ConversationError::UnauthorizedData(
            "record signer does not match installation identity"
        ))
    );
}

#[test]
fn test_local_message_v2_does_not_change_any_v1_fixture_or_id() {
    let before: Vec<(Vec<u8>, [u8; 32])> = fixtures::records()
        .into_iter()
        .map(|(_, record)| (record.bytes().to_vec(), *record.id()))
        .collect();
    let v2_without_parent = test_message_v2(b"future body", None);
    let v2_with_parent = test_message_v2(b"future body", Some([9; 32]));
    assert_ne!(v2_without_parent, v2_with_parent);
    assert_eq!(&v2_without_parent[..4], FAMILY_MARKER);
    assert_eq!(v2_without_parent[5], FORMAT_VERSION_V1 + 1);

    let after: Vec<(Vec<u8>, [u8; 32])> = fixtures::records()
        .into_iter()
        .map(|(_, record)| (record.bytes().to_vec(), *record.id()))
        .collect();
    assert_eq!(before, after);
}

fn outer_payload_bounds(bytes: &[u8]) -> (usize, usize) {
    let mut index = 6;
    let mut value = 0_u64;
    let mut shift = 0;
    loop {
        let byte = bytes[index];
        index += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (index, index + value as usize)
}

fn ordered_id(index: usize) -> [u8; 32] {
    let mut value = [0; 32];
    value[..8].copy_from_slice(&(index as u64).to_be_bytes());
    value
}

fn test_message_v2(body: &[u8], parent: Option<[u8; 32]>) -> Vec<u8> {
    let mut output = FAMILY_MARKER.to_vec();
    output.push(2);
    output.push(2);
    match parent {
        None => output.push(0),
        Some(parent) => {
            output.push(1);
            output.extend_from_slice(&parent);
        }
    }
    output.extend_from_slice(&(body.len() as u64).to_be_bytes());
    output.extend_from_slice(body);
    output
}
