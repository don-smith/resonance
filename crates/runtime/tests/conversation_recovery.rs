use std::path::Path;

use resonance_runtime::{
    conversations::testing::{
        authority::ConversationAuthority,
        recovery::RecoveryRequestTracker,
        runtime::{ConversationRuntime, ConversationSyncState},
        wire::{
            ConversationRecordV1, ExactRecordV1, RecoveryHeadV1, RecoveryRequestV1,
            RecoveryResponseV1,
        },
    },
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{MembershipLog, SignedMembershipOperation},
    workspace_store::WorkspaceStore,
};

fn identity(signing: u8, recipient: u8) -> InstallationIdentity {
    let custody = InMemoryKeyCustody::with_secret(vec![signing; 32]);
    custody.replace_recipient_secret(vec![recipient; 32]);
    InstallationIdentity::load_or_create(&custody).expect("identity loads")
}

fn store(root: &Path, workspace: &str) -> WorkspaceStore {
    WorkspaceStore::open(root, workspace).expect("store opens")
}

struct RecoveryFixture {
    _creator_root: tempfile::TempDir,
    _member_root: tempfile::TempDir,
    workspace: String,
    creator: InstallationIdentity,
    member: InstallationIdentity,
    creator_store: WorkspaceStore,
    member_store: WorkspaceStore,
    membership: MembershipLog,
    current_epoch: ExactRecordV1,
    general: ExactRecordV1,
}

impl RecoveryFixture {
    fn new() -> Self {
        let creator_root = tempfile::tempdir().expect("creator root");
        let member_root = tempfile::tempdir().expect("member root");
        let workspace = "55".repeat(32);
        let creator = identity(31, 91);
        let member = identity(32, 92);
        let creator_store = store(creator_root.path(), &workspace);
        let member_store = store(member_root.path(), &workspace);
        let mut creator_authority =
            ConversationAuthority::open(creator.clone(), &workspace, creator_store.clone())
                .expect("creator authority");
        let member_authority =
            ConversationAuthority::open(member.clone(), &workspace, member_store.clone())
                .expect("member authority");
        let creator_recipient = creator_authority.local_recipient_record().clone();
        let member_recipient = member_authority.local_recipient_record().clone();
        creator_authority
            .stage_recipient_record(member_recipient.bytes(), member.public_identity())
            .expect("member key stages");

        let mut membership = MembershipLog::new();
        let genesis =
            SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
        let prepared = membership
            .prepare(&workspace, genesis.clone())
            .expect("prepare");
        let genesis_commit = creator_authority
            .commit_transition(&prepared)
            .expect("genesis commits");
        membership.insert(genesis).expect("genesis finalizes");
        let addition = SignedMembershipOperation::add_member(
            &creator,
            &workspace,
            membership
                .projection(&workspace)
                .canonical_head
                .expect("head")
                .to_string(),
            1,
            *member.public_identity().as_bytes(),
            "Lin",
            2,
        )
        .expect("addition");
        let prepared = membership
            .prepare(&workspace, addition.clone())
            .expect("prepare");
        let addition_commit = creator_authority
            .commit_transition(&prepared)
            .expect("addition commits");
        membership.insert(addition).expect("addition finalizes");
        member_store
            .record_recipient_key(&creator_recipient, true)
            .expect("creator key copies");
        member_store
            .record_recipient_key(&member_recipient, true)
            .expect("member key copies");
        Self {
            _creator_root: creator_root,
            _member_root: member_root,
            workspace,
            creator,
            member,
            creator_store,
            member_store,
            membership,
            current_epoch: addition_commit.epoch,
            general: genesis_commit.genesis_channel.expect("general"),
        }
    }

    fn creator_runtime(&self) -> ConversationRuntime {
        ConversationRuntime::open(
            self.creator.clone(),
            &self.workspace,
            self.creator_store.clone(),
            self.membership.clone(),
        )
        .expect("creator runtime")
    }

    fn empty_member_runtime(&self) -> ConversationRuntime {
        ConversationRuntime::open(
            self.member.clone(),
            &self.workspace,
            self.member_store.clone(),
            self.membership.clone(),
        )
        .expect("member runtime")
    }
}

#[test]
fn exact_duties_survive_restart_filter_old_envelopes_and_clear_only_after_acknowledgement() {
    let fixture = RecoveryFixture::new();
    let mut creator = fixture.creator_runtime();
    let message = creator
        .post_message(fixture.general_id(), "direct exact ciphertext", 10)
        .expect("message posts");
    let pending = creator
        .pending_commonware_records_for(fixture.member.public_identity())
        .expect("pending records");
    assert!(pending
        .iter()
        .any(|bytes| bytes == fixture.current_epoch.bytes()));
    assert!(pending.iter().any(|bytes| {
        ExactRecordV1::decode(bytes).is_ok_and(|record| record.id() == &message.message_id)
    }));
    assert!(!pending.iter().any(|bytes| {
        ExactRecordV1::decode(bytes).is_ok_and(|record| {
            matches!(record.record(), ConversationRecordV1::Epoch(epoch) if !epoch.recipients.iter().any(|recipient| recipient.member == *fixture.member.public_identity().as_bytes()))
        })
    }));
    drop(creator);

    let mut restarted = fixture.creator_runtime();
    assert_eq!(
        restarted
            .pending_commonware_records_for(fixture.member.public_identity())
            .expect("restart duties"),
        pending
    );
    assert_eq!(
        restarted
            .network_synchronization_state(true, true)
            .expect("pending state"),
        ConversationSyncState::WaitingToSync
    );
    let member = fixture.empty_member_runtime();
    let ids = pending
        .iter()
        .map(|bytes| *ExactRecordV1::decode(bytes).expect("exact").id())
        .collect();
    let acknowledgement = member.acknowledgement(ids).expect("ack signs");
    restarted
        .accept_acknowledgement(
            fixture.member.public_identity(),
            acknowledgement.bytes(),
            20,
        )
        .expect("ack applies");
    assert!(restarted
        .pending_commonware_records_for(fixture.member.public_identity())
        .expect("duties clear")
        .is_empty());
    assert_eq!(
        restarted
            .network_synchronization_state(true, true)
            .expect("current state"),
        ConversationSyncState::Current
    );
}

#[test]
fn epoch_arrives_before_open_and_sparse_disconnect_recovery_commits_exact_records() {
    let fixture = RecoveryFixture::new();
    let mut creator = fixture.creator_runtime();
    let first = creator
        .post_message(fixture.general_id(), "zero", 10)
        .expect("first");
    let missing = creator
        .post_message(fixture.general_id(), "one", 11)
        .expect("missing");
    let third = creator
        .post_message(fixture.general_id(), "two", 12)
        .expect("third");
    let pending = creator
        .pending_commonware_records_for(fixture.member.public_identity())
        .expect("pending");
    let exact_message = |id| {
        pending
            .iter()
            .find(|bytes| ExactRecordV1::decode(bytes).is_ok_and(|record| record.id() == &id))
            .expect("message exact")
            .clone()
    };
    let first_exact = exact_message(first.message_id);
    let missing_exact = exact_message(missing.message_id);
    let third_exact = exact_message(third.message_id);

    let mut member = fixture.empty_member_runtime();
    assert!(member.accept_message_record(&first_exact).is_err());
    member
        .accept_epoch_record(fixture.current_epoch.bytes())
        .expect("HPKE epoch opens");
    member
        .accept_channel_record(fixture.general.bytes())
        .expect("catalog applies");
    member
        .accept_message_record(&first_exact)
        .expect("first applies");
    member
        .accept_message_record(&third_exact)
        .expect("third applies");
    let gaps = member.recovery_gaps().expect("gaps");
    assert_eq!(
        gaps[fixture.creator.public_identity().as_bytes()][0].first_sequence,
        1
    );

    let request = member.recovery_request().expect("request signs");
    let ConversationRecordV1::RecoveryRequest(request_record) = request.record() else {
        panic!("request family");
    };
    assert_eq!(request_record.heads[0].highest_sequence, 0);
    let response = creator
        .answer_recovery_request(fixture.member.public_identity(), request.bytes())
        .expect("response filters from canonical history");
    let ConversationRecordV1::RecoveryResponse(response_record) = response.record() else {
        panic!("response family");
    };
    assert!(response_record.records.contains(&missing_exact));
    let committed = member
        .accept_recovery_response(fixture.creator.public_identity(), response.bytes())
        .expect("response commits");
    assert!(committed.contains(&missing.message_id));
    let filled = member.recovery_gaps().expect("filled");
    assert!(
        filled.values().all(Vec::is_empty),
        "remaining gaps: {filled:?}"
    );
    assert_eq!(
        member
            .messages(&fixture.general_id())
            .expect("messages")
            .len(),
        3
    );
}

#[test]
fn recovery_response_applies_catalog_before_messages_regardless_of_record_order() {
    let fixture = RecoveryFixture::new();
    let mut creator = fixture.creator_runtime();
    let channel_hash = blake3::hash(fixture.general.bytes()).as_bytes().to_owned();
    let message_exact = (0..32)
        .find_map(|sequence| {
            let message = creator
                .post_message(
                    fixture.general_id(),
                    &format!("catalog follows message {sequence}"),
                    10 + sequence,
                )
                .ok()?;
            let exact = creator
                .pending_commonware_records_for(fixture.member.public_identity())
                .ok()?
                .into_iter()
                .find(|bytes| {
                    ExactRecordV1::decode(bytes)
                        .is_ok_and(|exact| exact.id() == &message.message_id)
                })?;
            (blake3::hash(&exact).as_bytes() < &channel_hash).then_some(exact)
        })
        .expect("fixture obtains a message that canonically precedes its channel");

    let mut member = fixture.empty_member_runtime();
    member
        .accept_epoch_record(fixture.current_epoch.bytes())
        .expect("epoch opens");
    let response = ExactRecordV1::author(
        ConversationRecordV1::RecoveryResponse(RecoveryResponseV1 {
            workspace_id: [0x55; 32],
            sender: *fixture.creator.public_identity().as_bytes(),
            records: vec![message_exact, fixture.general.bytes().to_vec()],
        }),
        &fixture.creator,
    )
    .expect("response signs");
    member
        .accept_recovery_response(fixture.creator.public_identity(), response.bytes())
        .expect("catalog precedes message application");
    assert_eq!(
        member
            .messages(&fixture.general_id())
            .expect("messages")
            .len(),
        1
    );
}

#[test]
fn recovery_response_capacity_prioritizes_catalog_without_reshuffling_groups() {
    let fixture = RecoveryFixture::new();
    let mut creator = fixture.creator_runtime();
    for sequence in 0..127 {
        creator
            .post_message(
                fixture.general_id(),
                &format!("message {sequence}"),
                10 + sequence,
            )
            .expect("message posts");
    }
    let request = fixture
        .empty_member_runtime()
        .recovery_request()
        .expect("request signs");
    let response = creator
        .answer_recovery_request(fixture.member.public_identity(), request.bytes())
        .expect("response signs");
    let ConversationRecordV1::RecoveryResponse(response) = response.record() else {
        panic!("response family");
    };
    assert_eq!(response.records.len(), 128);
    assert!(response.records.contains(&fixture.general.bytes().to_vec()));
    assert_eq!(
        response.records,
        {
            let mut canonical = response.records.clone();
            canonical.sort_by_key(|bytes| blake3::hash(bytes).as_bytes().to_owned());
            canonical
        },
        "the selected channel-first page keeps v1's global canonical order"
    );
    assert_eq!(
        response
            .records
            .iter()
            .filter(|bytes| {
                matches!(
                    ExactRecordV1::decode(bytes).expect("exact").record(),
                    ConversationRecordV1::Message(_)
                )
            })
            .count(),
        127
    );
    let first_page = response.records.clone();
    let changed_request = ExactRecordV1::author(
        ConversationRecordV1::RecoveryRequest(RecoveryRequestV1 {
            workspace_id: [0x55; 32],
            sender: *fixture.member.public_identity().as_bytes(),
            heads: vec![RecoveryHeadV1 {
                author: *fixture.creator.public_identity().as_bytes(),
                highest_sequence: 126,
            }],
            missing_ranges: Vec::new(),
        }),
        &fixture.member,
    )
    .expect("changed request signs");
    let retransmission = creator
        .answer_recovery_request(fixture.member.public_identity(), changed_request.bytes())
        .expect("unacknowledged page retransmits");
    let ConversationRecordV1::RecoveryResponse(retransmission) = retransmission.record() else {
        panic!("retransmission family");
    };
    assert_eq!(
        retransmission.records, first_page,
        "changed requester heads cannot replace an awaiting page"
    );

    let received = first_page
        .iter()
        .map(|bytes| *ExactRecordV1::decode(bytes).expect("exact").id())
        .collect();
    let acknowledgement = fixture
        .empty_member_runtime()
        .acknowledgement(received)
        .expect("acknowledgement signs");
    creator
        .accept_acknowledgement(
            fixture.member.public_identity(),
            acknowledgement.bytes(),
            20,
        )
        .expect("page acknowledgement applies");
    let completion = creator
        .answer_recovery_request(fixture.member.public_identity(), request.bytes())
        .expect("completion response signs");
    let ConversationRecordV1::RecoveryResponse(completion) = completion.record() else {
        panic!("completion family");
    };
    assert!(
        completion.records.is_empty(),
        "an exactly-full final page is followed by a completion response"
    );
    let restarted = creator
        .answer_recovery_request(fixture.member.public_identity(), request.bytes())
        .expect("later identical request recomputes");
    let ConversationRecordV1::RecoveryResponse(restarted) = restarted.record() else {
        panic!("restarted response family");
    };
    assert_eq!(
        restarted.records.len(),
        128,
        "the empty completion response is sent exactly once"
    );
}

#[test]
fn acknowledged_short_recovery_page_expires_for_later_identical_requests() {
    let fixture = RecoveryFixture::new();
    let mut creator = fixture.creator_runtime();
    let first = creator
        .post_message(fixture.general_id(), "first", 10)
        .expect("first message posts");
    let request = fixture
        .empty_member_runtime()
        .recovery_request()
        .expect("request signs");
    let response = creator
        .answer_recovery_request(fixture.member.public_identity(), request.bytes())
        .expect("short response signs");
    let ConversationRecordV1::RecoveryResponse(response) = response.record() else {
        panic!("short response family");
    };
    assert!(response.records.len() < 128);
    let acknowledgement = fixture
        .empty_member_runtime()
        .acknowledgement(
            response
                .records
                .iter()
                .map(|bytes| *ExactRecordV1::decode(bytes).expect("exact").id())
                .collect(),
        )
        .expect("acknowledgement signs");
    creator
        .accept_acknowledgement(
            fixture.member.public_identity(),
            acknowledgement.bytes(),
            20,
        )
        .expect("short page acknowledgement applies");
    let later = creator
        .post_message(fixture.general_id(), "later", 21)
        .expect("later message posts");
    let recomputed = creator
        .answer_recovery_request(fixture.member.public_identity(), request.bytes())
        .expect("later identical request recomputes");
    let ConversationRecordV1::RecoveryResponse(recomputed) = recomputed.record() else {
        panic!("recomputed response family");
    };
    let ids = recomputed
        .records
        .iter()
        .map(|bytes| *ExactRecordV1::decode(bytes).expect("exact").id())
        .collect::<Vec<_>>();
    assert!(ids.contains(&first.message_id));
    assert!(
        ids.contains(&later.message_id),
        "the expired short page cannot hide newly authored data"
    );
}

#[test]
fn recovery_request_tracker_suppresses_successful_request_loops_and_retries_gaps() {
    let peer = identity(81, 82).public_identity();
    let mut requests = RecoveryRequestTracker::default();
    requests.reset_for_membership(Some([9; 32]));
    assert!(
        requests.should_request(peer, false, 10),
        "initial sync requests once"
    );
    assert!(
        !requests.should_request(peer, false, 10),
        "request is in flight"
    );
    requests.response_completed(peer, 1);
    assert!(
        !requests.should_request(peer, false, 10),
        "a successful complete response does not immediately loop"
    );
    assert!(
        requests.should_request(peer, true, 10),
        "a known gap continues recovery"
    );
    assert!(
        !requests.should_request(peer, true, 11),
        "in-flight gap requests obey backoff"
    );
    assert!(
        requests.should_request(peer, true, 12),
        "timed-out requests retry"
    );
}

#[test]
fn removed_peer_cannot_request_or_receive_next_epoch_traffic_and_offline_state_is_truthful() {
    let fixture = RecoveryFixture::new();
    let old_request = fixture
        .empty_member_runtime()
        .recovery_request()
        .expect("old request");
    let interval = fixture
        .membership
        .projection(&fixture.workspace)
        .interval_id(&fixture.member.public_identity())
        .expect("interval")
        .clone();
    let removal = SignedMembershipOperation::expel_member(
        &fixture.creator,
        &fixture.workspace,
        fixture
            .membership
            .projection(&fixture.workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        2,
        *fixture.member.public_identity().as_bytes(),
        interval.as_str(),
        30,
    )
    .expect("removal signs");
    let prepared = fixture
        .membership
        .prepare(&fixture.workspace, removal.clone())
        .expect("removal prepares");
    let mut creator = fixture.creator_runtime();
    let mut authority = ConversationAuthority::open(
        fixture.creator.clone(),
        &fixture.workspace,
        fixture.creator_store.clone(),
    )
    .expect("authority reopens");
    let committed = authority
        .commit_transition(&prepared)
        .expect("next epoch commits");
    let mut removed_membership = fixture.membership.clone();
    removed_membership
        .insert(removal)
        .expect("removal finalizes");
    creator
        .replace_membership(removed_membership)
        .expect("runtime membership updates");
    assert!(creator
        .answer_recovery_request(fixture.member.public_identity(), old_request.bytes())
        .is_err());
    assert!(creator
        .pending_commonware_records_for(fixture.member.public_identity())
        .is_err());
    let ConversationRecordV1::Epoch(epoch) = committed.epoch.record() else {
        panic!("epoch family");
    };
    assert!(!epoch
        .recipients
        .iter()
        .any(|recipient| recipient.member == *fixture.member.public_identity().as_bytes()));

    let current_fixture = RecoveryFixture::new();
    let creator = current_fixture.creator_runtime();
    assert_eq!(
        creator
            .network_synchronization_state(false, false)
            .expect("offline"),
        ConversationSyncState::Offline
    );
    assert_eq!(
        creator
            .network_synchronization_state(true, false)
            .expect("waiting"),
        ConversationSyncState::WaitingToSync
    );
}

#[test]
fn usable_peer_does_not_make_pending_duties_or_sparse_gaps_current() {
    let fixture = RecoveryFixture::new();
    let creator = fixture.creator_runtime();
    assert_eq!(
        creator
            .network_synchronization_state(true, true)
            .expect("epoch duty waits"),
        ConversationSyncState::WaitingToSync
    );

    let mut source = fixture.creator_runtime();
    let first = source
        .post_message(fixture.general_id(), "first", 10)
        .expect("first posts");
    let _missing = source
        .post_message(fixture.general_id(), "missing", 11)
        .expect("missing posts");
    let third = source
        .post_message(fixture.general_id(), "third", 12)
        .expect("third posts");
    let pending = source
        .pending_commonware_records_for(fixture.member.public_identity())
        .expect("pending records");
    let exact_for = |id| {
        pending
            .iter()
            .find(|bytes| ExactRecordV1::decode(bytes).is_ok_and(|exact| exact.id() == &id))
            .expect("message exact")
    };
    let mut member = fixture.empty_member_runtime();
    member
        .accept_epoch_record(fixture.current_epoch.bytes())
        .expect("epoch opens");
    member
        .accept_channel_record(fixture.general.bytes())
        .expect("catalog opens");
    member
        .accept_message_record(exact_for(first.message_id))
        .expect("first applies");
    member
        .accept_message_record(exact_for(third.message_id))
        .expect("third applies");
    assert_eq!(
        member
            .network_synchronization_state(true, true)
            .expect("sparse gap waits"),
        ConversationSyncState::WaitingToSync
    );
}

impl RecoveryFixture {
    fn general_id(&self) -> [u8; 16] {
        let ConversationRecordV1::Channel(channel) = self.general.record() else {
            panic!("general family");
        };
        channel.channel_id
    }
}
