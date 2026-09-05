use std::{
    net::SocketAddr,
    thread,
    time::{Duration, Instant},
};

use resonance_runtime::{
    conversations::testing::{
        self,
        lookup::ConversationLookup,
        mesh::{
            ConversationMeshError, ConversationMeshEvent, InMemoryConversationMesh,
            ProductionConversationMesh,
        },
        wire::{
            AcknowledgementV1, AddressNoticeV1, ConversationRecordV1, ExactRecordV1,
            RecoveryRequestV1, MAX_RECORD_BYTES,
        },
    },
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{MembershipLog, SignedMembershipOperation},
    protocol::{Envelope, EnvelopeBody},
    workspace_store::WorkspaceStore,
};
use serde::Serialize;

fn identity(seed: u8) -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![seed; 32]))
        .expect("identity loads")
}

fn workspace_bytes(workspace: &str) -> [u8; 32] {
    let mut bytes = [0; 32];
    for (index, pair) in workspace.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        bytes[index] = hex_nibble(pair[0]) << 4 | hex_nibble(pair[1]);
    }
    bytes
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("hex fixture"),
    }
}

fn two_member_log(
    workspace: &str,
    creator: &InstallationIdentity,
    member: &InstallationIdentity,
) -> MembershipLog {
    let mut log = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(creator, workspace, "Ada", 1).expect("genesis signs");
    log.insert(genesis).expect("genesis inserts");
    let addition = SignedMembershipOperation::add_member(
        creator,
        workspace,
        log.projection(workspace)
            .canonical_head
            .expect("genesis head")
            .to_string(),
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("addition signs");
    log.insert(addition).expect("addition inserts");
    log
}

fn wait_for_started(mesh: &ProductionConversationMesh) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match mesh.recv_event_timeout(Duration::from_millis(100)) {
            Some(ConversationMeshEvent::Started { .. }) => return,
            Some(ConversationMeshEvent::Fatal(error)) => panic!("mesh failed: {error}"),
            _ => {}
        }
    }
    panic!("mesh did not start");
}

#[test]
fn sources_have_no_iroh_conversation_payload_or_relay_discovery_branch() {
    let protocol = include_str!("../src/protocol/mod.rs");
    for forbidden in [
        "ConversationMessage(",
        "ChannelRecord(",
        "EpochRecord(",
        "Acknowledgement(",
        "RecoveryRequest(",
        "RecoveryResponse(",
    ] {
        assert!(
            !protocol.contains(forbidden),
            "Iroh payload branch: {forbidden}"
        );
    }
    let mesh = concat!(
        include_str!("../src/conversations/mesh.rs"),
        include_str!("../src/conversations/lookup.rs"),
        include_str!("../src/conversations/address_directory.rs"),
    );
    for forbidden in [
        "chat relay",
        "rendezvous",
        "bootstrapper",
        "hole punching",
        "STUN",
        "TURN",
        "UPnP",
        "NAT traversal",
    ] {
        assert!(
            !mesh.contains(forbidden),
            "unsupported network path: {forbidden}"
        );
    }
}

#[test]
fn bounded_adapters_report_backpressure_and_monotonic_peer_sets() {
    let first = identity(1).public_identity();
    let second = identity(2).public_identity();
    let address: SocketAddr = "127.0.0.1:41001".parse().expect("address");
    let mut mesh = InMemoryConversationMesh::with_capacity(1);
    mesh.track_members(7, vec![(first, vec![address]), (second, vec![address])])
        .expect("peer set tracks");
    mesh.overwrite_addresses(vec![(
        second,
        vec!["127.0.0.1:41002".parse().expect("replacement")],
    )]);
    assert_eq!(mesh.peer_set_version(), Some(7));
    assert_eq!(mesh.peers()[&second][0].port(), 41002);
    mesh.send(Some(second), b"first").expect("first queues");
    assert_eq!(
        mesh.send(Some(second), b"second"),
        Err(ConversationMeshError::Backpressure)
    );
    assert!(mesh.track_members(7, vec![(first, vec![address])]).is_err());
    mesh.take_sent();
    mesh.track_members(8, vec![(first, vec![address])])
        .expect("removal advances peer set");
    assert!(!mesh.peers().contains_key(&second));
    assert_eq!(
        mesh.send(Some(second), b"removed peer"),
        Err(ConversationMeshError::UnauthorizedPeer)
    );
    mesh.stop();
    assert_eq!(
        mesh.send(None, b"after stop"),
        Err(ConversationMeshError::Stopped)
    );
}

#[test]
fn runtime_bridge_propagates_startup_failure_panic_and_joins_repeatedly() {
    let identity = identity(3);
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("port reserves");
    let occupied_address = occupied.local_addr().expect("address");
    assert!(
        ProductionConversationMesh::start(identity.clone(), [3; 32], occupied_address).is_err()
    );
    drop(occupied);

    for iteration in 0..3 {
        let mut mesh = ProductionConversationMesh::start(
            identity.clone(),
            [3; 32],
            "127.0.0.1:0".parse().expect("listen"),
        )
        .expect("mesh starts");
        wait_for_started(&mesh);
        if iteration == 0 {
            testing::force_mesh_thread_panic(&mesh).expect("panic injects");
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut reported = false;
            while Instant::now() < deadline {
                if matches!(
                    mesh.recv_event_timeout(Duration::from_millis(100)),
                    Some(ConversationMeshEvent::Fatal(_))
                ) {
                    reported = true;
                    break;
                }
            }
            assert!(reported, "thread panic is reported");
        }
        mesh.stop().expect("thread joins");
        assert_eq!(
            mesh.send(None, b"stopped"),
            Err(ConversationMeshError::Stopped)
        );
    }
}

#[derive(Serialize)]
struct LegacyEnvelope<'a> {
    version: u8,
    workspace_id: &'a str,
    sender: [u8; 32],
    body: LegacyEnvelopeBody,
    signature: &'a [u8],
}

#[derive(Serialize)]
enum LegacyEnvelopeBody {
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

#[test]
fn appended_iroh_controls_preserve_legacy_bytes_and_reject_bad_outer_inputs() {
    let signer = identity(5);
    let workspace = "45".repeat(32);
    let current = Envelope::sign(&signer, &workspace, EnvelopeBody::Heartbeat { sent_at: 7 })
        .expect("legacy envelope signs");
    let legacy = postcard::to_stdvec(&LegacyEnvelope {
        version: current.version,
        workspace_id: &current.workspace_id,
        sender: current.sender,
        body: LegacyEnvelopeBody::Heartbeat { sent_at: 7 },
        signature: &current.signature,
    })
    .expect("legacy envelope encodes");
    assert_eq!(current.encode().expect("current encodes"), legacy);
    let mut bad_signature = current.clone();
    bad_signature.signature[0] ^= 1;
    assert!(bad_signature.verify().is_err());
    assert!(Envelope::decode(b"malformed").is_err());
    let oversized = Envelope {
        version: 1,
        workspace_id: workspace,
        sender: *signer.public_identity().as_bytes(),
        body: EnvelopeBody::AddressNotice(vec![0; MAX_RECORD_BYTES + 1]),
        signature: vec![0; 64],
    };
    assert!(oversized.verify().is_err());

    // Compile every old variant through the legacy serializer so appending controls cannot
    // accidentally reorder the established postcard discriminants.
    let legacy_variants = [
        LegacyEnvelopeBody::MembershipOperation(vec![1]),
        LegacyEnvelopeBody::JoinRequest {
            inviter: [1; 32],
            display_name: "member".to_owned(),
            recipient_key: vec![2],
        },
        LegacyEnvelopeBody::MembershipSyncRequest,
        LegacyEnvelopeBody::MembershipSyncResponse(vec![vec![3]]),
        LegacyEnvelopeBody::FileHistoryNotice {
            operation_ids: vec!["00".repeat(16)],
        },
    ];
    for body in legacy_variants {
        assert!(!postcard::to_stdvec(&body).expect("legacy body").is_empty());
    }
}

#[test]
fn address_controls_reject_wrong_sender_workspace_member_expiry_and_stale_generation() {
    let workspace = "46".repeat(32);
    let creator = identity(6);
    let member = identity(7);
    let outsider = identity(8);
    let membership = two_member_log(&workspace, &creator, &member);
    let mut creator_lookup = ConversationLookup::start(
        creator.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
    )
    .expect("creator starts");
    let mut member_lookup = ConversationLookup::start(
        member.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
    )
    .expect("member starts");
    wait_for_started(creator_lookup.mesh());
    wait_for_started(member_lookup.mesh());

    let valid = member_lookup.local_address_notice(2, 100).expect("notice");
    assert!(creator_lookup
        .accept_address_notice(creator.public_identity(), valid.bytes(), 10)
        .is_err());
    let expired = member_lookup
        .local_address_notice(3, 9)
        .expect("expired signs");
    assert!(creator_lookup
        .accept_address_notice(member.public_identity(), expired.bytes(), 10)
        .is_err());
    let too_far = member_lookup
        .local_address_notice(3, 311)
        .expect("long expiry signs");
    assert!(creator_lookup
        .accept_address_notice(member.public_identity(), too_far.bytes(), 10)
        .is_err());
    assert!(creator_lookup
        .accept_address_notice(member.public_identity(), valid.bytes(), 10)
        .expect("valid applies"));
    let stale = member_lookup
        .local_address_notice(1, 100)
        .expect("stale signs");
    assert!(creator_lookup
        .accept_address_notice(member.public_identity(), stale.bytes(), 10)
        .is_err());

    let wrong_workspace = ExactRecordV1::author(
        ConversationRecordV1::AddressNotice(AddressNoticeV1 {
            workspace_id: [99; 32],
            sender: *member.public_identity().as_bytes(),
            observed_membership_head: valid_head(&membership, &workspace),
            generation: 4,
            expires_at: 100,
            addresses: vec![member_lookup.mesh().listen_addr().to_string()],
        }),
        &member,
    )
    .expect("wrong workspace signs");
    assert!(creator_lookup
        .accept_address_notice(member.public_identity(), wrong_workspace.bytes(), 10)
        .is_err());
    let outsider_notice = ExactRecordV1::author(
        ConversationRecordV1::AddressNotice(AddressNoticeV1 {
            workspace_id: workspace_bytes(&workspace),
            sender: *outsider.public_identity().as_bytes(),
            observed_membership_head: valid_head(&membership, &workspace),
            generation: 1,
            expires_at: 100,
            addresses: vec!["127.0.0.1:40001".to_owned()],
        }),
        &outsider,
    )
    .expect("outsider notice signs");
    assert!(creator_lookup
        .accept_address_notice(outsider.public_identity(), outsider_notice.bytes(), 10)
        .is_err());
    let mut bad_signature = valid.bytes().to_vec();
    let last = bad_signature.len() - 1;
    bad_signature[last] ^= 1;
    assert!(creator_lookup
        .accept_address_notice(member.public_identity(), &bad_signature, 10)
        .is_err());
    creator_lookup.stop().expect("creator stops");
    member_lookup.stop().expect("member stops");
}

fn valid_head(membership: &MembershipLog, workspace: &str) -> [u8; 32] {
    workspace_bytes(
        membership
            .projection(workspace)
            .canonical_head
            .expect("head")
            .as_str(),
    )
}

#[test]
fn address_generations_and_notices_survive_restart_without_advancing_membership_version() {
    let root = tempfile::tempdir().expect("root");
    let workspace = "47".repeat(32);
    let creator = identity(9);
    let member = identity(10);
    let membership = two_member_log(&workspace, &creator, &member);
    let store = WorkspaceStore::open(root.path(), &workspace).expect("store");
    assert_eq!(
        store.next_conversation_address_generation().expect("one"),
        1
    );
    assert_eq!(
        store.next_conversation_address_generation().expect("two"),
        2
    );
    let version = store.lookup_peer_set_version().expect("version");
    let mut member_lookup = ConversationLookup::start(
        member.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
    )
    .expect("member lookup");
    wait_for_started(member_lookup.mesh());
    let notice = member_lookup.local_address_notice(2, 100).expect("notice");
    let mut creator_lookup = ConversationLookup::start_with_store(
        creator.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
        store.clone(),
        10,
    )
    .expect("creator lookup");
    wait_for_started(creator_lookup.mesh());
    creator_lookup
        .accept_address_notice(member.public_identity(), notice.bytes(), 10)
        .expect("notice persists");
    creator_lookup.stop().expect("creator stops");
    let mut restarted = ConversationLookup::start_with_store(
        creator,
        &workspace,
        membership,
        "127.0.0.1:0".parse().expect("listen"),
        store.clone(),
        11,
    )
    .expect("persisted notice reloads");
    wait_for_started(restarted.mesh());
    let stale = member_lookup
        .local_address_notice(1, 100)
        .expect("stale notice");
    assert!(restarted
        .accept_address_notice(member.public_identity(), stale.bytes(), 11)
        .is_err());
    assert_eq!(store.lookup_peer_set_version().expect("unchanged"), version);
    restarted.stop().expect("restart stops");
    member_lookup.stop().expect("member stops");
}

#[test]
fn commonware_lookup_authenticates_two_installations_and_reconnects_after_listener_replacement() {
    let workspace = "44".repeat(32);
    let creator = identity(11);
    let member = identity(12);
    let membership = two_member_log(&workspace, &creator, &member);
    let mut creator_lookup = ConversationLookup::start(
        creator.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
    )
    .expect("creator lookup starts");
    let mut member_lookup = ConversationLookup::start(
        member.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
    )
    .expect("member lookup starts");
    wait_for_started(creator_lookup.mesh());
    wait_for_started(member_lookup.mesh());

    let creator_notice = creator_lookup
        .local_address_notice(1, 100)
        .expect("creator notice");
    let member_notice = ExactRecordV1::author(
        ConversationRecordV1::AddressNotice(AddressNoticeV1 {
            workspace_id: workspace_bytes(&workspace),
            sender: *member.public_identity().as_bytes(),
            observed_membership_head: valid_head(&membership, &workspace),
            generation: 1,
            expires_at: 100,
            addresses: vec![
                "127.0.0.1:1".to_owned(),
                member_lookup.mesh().listen_addr().to_string(),
            ],
        }),
        &member,
    )
    .expect("member notice");
    creator_lookup
        .replace_membership(membership.clone(), 1, 10)
        .expect("creator tracks the full canonical set before addresses arrive");
    member_lookup
        .replace_membership(membership.clone(), 1, 10)
        .expect("member tracks the full canonical set before addresses arrive");
    creator_lookup
        .accept_address_notice(member.public_identity(), member_notice.bytes(), 10)
        .expect("member notice overwrites its closed candidate");
    member_lookup
        .accept_address_notice(creator.public_identity(), creator_notice.bytes(), 10)
        .expect("creator notice overwrites its closed candidate");

    let record = ExactRecordV1::author(
        ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
            workspace_id: workspace_bytes(&workspace),
            sender: *creator.public_identity().as_bytes(),
            record_ids: Vec::new(),
        }),
        &creator,
    )
    .expect("record signs");
    thread::sleep(Duration::from_millis(500));
    creator_lookup
        .rotate_direct_candidates(10)
        .expect("failed candidate rotates");
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        creator_lookup
            .mesh()
            .send(Some(member.public_identity()), record.bytes())
            .ok();
        while creator_lookup.mesh().try_event().is_some() {}
        if Instant::now() >= deadline {
            panic!("lookup did not connect");
        }
        if let Some(ConversationMeshEvent::Received {
            authenticated_peer,
            exact_bytes,
        }) = member_lookup
            .mesh()
            .recv_event_timeout(Duration::from_millis(300))
        {
            if exact_bytes == record.bytes() {
                assert_eq!(authenticated_peer, creator.public_identity());
                break;
            }
        }
    }

    member_lookup.stop().expect("old member listener stops");
    thread::sleep(Duration::from_millis(500));
    let mut replacement = ConversationLookup::start(
        member.clone(),
        &workspace,
        membership.clone(),
        "127.0.0.1:0".parse().expect("listen"),
    )
    .expect("replacement starts");
    wait_for_started(replacement.mesh());
    let replacement_notice = replacement
        .local_address_notice(2, 100)
        .expect("replacement notice");
    creator_lookup
        .accept_address_notice(member.public_identity(), replacement_notice.bytes(), 11)
        .expect("replacement overwrites without a peer-set increment");
    assert!(creator_lookup.another_member_has_candidate(11));
    replacement
        .accept_address_notice(creator.public_identity(), creator_notice.bytes(), 11)
        .expect("creator notice validates");
    replacement
        .replace_membership(membership.clone(), 1, 11)
        .expect("same durable peer version restores");

    let record_two = ExactRecordV1::author(
        ConversationRecordV1::RecoveryRequest(RecoveryRequestV1 {
            workspace_id: workspace_bytes(&workspace),
            sender: *creator.public_identity().as_bytes(),
            heads: Vec::new(),
            missing_ranges: Vec::new(),
        }),
        &creator,
    )
    .expect("second record signs");
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut reconnected = false;
    while Instant::now() < deadline {
        creator_lookup
            .mesh()
            .send(Some(member.public_identity()), record_two.bytes())
            .ok();
        if let Some(ConversationMeshEvent::Received { exact_bytes, .. }) = replacement
            .mesh()
            .recv_event_timeout(Duration::from_millis(300))
        {
            if exact_bytes == record_two.bytes() {
                reconnected = true;
                break;
            }
        }
    }
    assert!(reconnected, "listener replacement did not reconnect");

    let interval = membership
        .projection(&workspace)
        .interval_id(&member.public_identity())
        .expect("member interval")
        .clone();
    let removal = SignedMembershipOperation::expel_member(
        &creator,
        &workspace,
        membership
            .projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        2,
        *member.public_identity().as_bytes(),
        interval.as_str(),
        20,
    )
    .expect("removal signs");
    let mut removed_membership = membership;
    removed_membership.insert(removal).expect("removal applies");
    creator_lookup
        .replace_membership(removed_membership, 2, 20)
        .expect("removal immediately replaces the sole tracked peer set");
    while creator_lookup.mesh().try_event().is_some() {}
    thread::sleep(Duration::from_millis(500));
    let removed_record = ExactRecordV1::author(
        ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
            workspace_id: workspace_bytes(&workspace),
            sender: *member.public_identity().as_bytes(),
            record_ids: vec![[9; 32]],
        }),
        &member,
    )
    .expect("removed record signs");
    for _ in 0..5 {
        replacement
            .mesh()
            .send(Some(creator.public_identity()), removed_record.bytes())
            .ok();
        thread::sleep(Duration::from_millis(100));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(ConversationMeshEvent::Received { exact_bytes, .. }) = creator_lookup
            .mesh()
            .recv_event_timeout(Duration::from_millis(100))
        {
            assert_ne!(exact_bytes, removed_record.bytes());
        }
    }
    creator_lookup.stop().expect("creator stops");
    replacement.stop().expect("replacement stops");
}
