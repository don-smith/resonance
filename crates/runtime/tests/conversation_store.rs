use std::path::{Path, PathBuf};

use resonance_runtime::{
    conversations::{
        testing::{
            self,
            authority::{ConversationAuthority, EpochMaterialProvider},
            crypto::{
                self, EpochEnvelopeContextV1, EpochKey, HpkeEpochEnvelopeV1, RecipientPrivateKey,
            },
            runtime::{
                ConversationRuntime, ConversationSyncState, MessageCommitOutcome,
                SparseRecoveryRange,
            },
            wire::{
                ChannelOperationV1, ChannelRecordV1, ConversationRecordV1, ExactRecordV1,
                RecoveryResponseV1,
            },
        },
        ConversationError, MAX_CHANNEL_SNAPSHOT,
    },
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{MembershipLog, SignedMembershipOperation, SignedSelfRemovalRequestV1},
    workspace_store::WorkspaceStore,
};
use rusqlite::Connection;

fn identity(signing_seed: u8, recipient_seed: u8) -> InstallationIdentity {
    let custody = InMemoryKeyCustody::with_secret(vec![signing_seed; 32]);
    custody.replace_recipient_secret(vec![recipient_seed; 32]);
    InstallationIdentity::load_or_create(&custody).expect("identity loads")
}

fn store(root: &Path, workspace: &str) -> WorkspaceStore {
    WorkspaceStore::open(root, workspace).expect("store opens")
}

fn database(root: &Path, workspace: &str) -> PathBuf {
    root.join(".resonance/workspaces")
        .join(workspace)
        .join("workspace.sqlite3")
}

struct FixtureMaterial {
    key: u8,
    next_randomness: u8,
}

impl FixtureMaterial {
    fn new(key: u8) -> Self {
        Self {
            key,
            next_randomness: key.wrapping_add(1),
        }
    }
}

impl EpochMaterialProvider for FixtureMaterial {
    fn generate_epoch_key(&mut self) -> Result<EpochKey, ConversationError> {
        Ok(testing::epoch_key([self.key; 32]))
    }

    fn wrap_epoch_key(
        &mut self,
        key: &EpochKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
        let randomness = [self.next_randomness; 32];
        self.next_randomness = self.next_randomness.wrapping_add(1);
        testing::wrap_epoch_key(key, context, randomness)
    }

    fn open_epoch_key(
        &mut self,
        envelope: &HpkeEpochEnvelopeV1,
        recipient: &RecipientPrivateKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<EpochKey, ConversationError> {
        crypto::unwrap_epoch_key(envelope, recipient, context)
    }
}

struct TwoMemberWorkspace {
    _creator_root: tempfile::TempDir,
    member_root: tempfile::TempDir,
    workspace: String,
    creator: InstallationIdentity,
    member: InstallationIdentity,
    membership: MembershipLog,
    creator_store: WorkspaceStore,
    member_store: WorkspaceStore,
    current_epoch: ExactRecordV1,
    general: ExactRecordV1,
}

impl TwoMemberWorkspace {
    fn new() -> Self {
        let creator_root = tempfile::tempdir().expect("creator root");
        let member_root = tempfile::tempdir().expect("member root");
        let workspace = "c1".repeat(32);
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
            .expect("member recipient stages");

        let mut membership = MembershipLog::new();
        let genesis =
            SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
        let genesis_prepared = membership
            .prepare(&workspace, genesis.clone())
            .expect("genesis prepares");
        let genesis_commit = creator_authority
            .commit_transition_with(&genesis_prepared, &mut FixtureMaterial::new(70))
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
        let addition_prepared = membership
            .prepare(&workspace, addition.clone())
            .expect("addition prepares");
        let addition_commit = creator_authority
            .commit_transition_with(&addition_prepared, &mut FixtureMaterial::new(71))
            .expect("addition commits");
        membership.insert(addition).expect("addition finalizes");

        member_store
            .record_recipient_key(&creator_recipient, true)
            .expect("creator recipient copies");
        member_store
            .record_recipient_key(&member_recipient, true)
            .expect("member recipient accepts");
        Self {
            _creator_root: creator_root,
            member_root,
            workspace,
            creator,
            member,
            membership,
            creator_store,
            member_store,
            current_epoch: addition_commit.epoch,
            general: genesis_commit.genesis_channel.expect("general channel"),
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

    fn member_runtime(&self) -> ConversationRuntime {
        let mut runtime = ConversationRuntime::open(
            self.member.clone(),
            &self.workspace,
            self.member_store.clone(),
            self.membership.clone(),
        )
        .expect("member runtime");
        runtime
            .accept_epoch_record(self.current_epoch.bytes())
            .expect("member opens current epoch");
        runtime
            .accept_channel_record(self.general.bytes())
            .expect("member accepts general");
        runtime
    }

    fn general_id(&self) -> [u8; 16] {
        let ConversationRecordV1::Channel(record) = self.general.record() else {
            panic!("general channel family");
        };
        record.channel_id
    }
}

#[test]
fn local_message_clock_archive_and_outbox_commit_atomically_and_survive_restart() {
    let fixture = TwoMemberWorkspace::new();
    let channel = fixture.general_id();
    let mut member = fixture.member_runtime();
    let first = member
        .post_message(channel, "first private body", 10)
        .expect("first posts");
    let second = member
        .post_message(channel, "second private body", 11)
        .expect("second posts");
    let third = member
        .post_message(channel, "third private body", 12)
        .expect("third posts");
    assert_eq!((first.author_sequence, first.lamport), (0, 1));
    assert_eq!((second.author_sequence, second.lamport), (1, 2));
    assert_eq!((third.author_sequence, third.lamport), (2, 3));
    let exact_outbox = testing::outbox_exact_records(&member).expect("outbox");
    assert_eq!(exact_outbox.len(), 3);

    let restarted = ConversationRuntime::open(
        fixture.member.clone(),
        &fixture.workspace,
        fixture.member_store.clone(),
        fixture.membership.clone(),
    )
    .expect("runtime restarts");
    assert_eq!(
        testing::outbox_exact_records(&restarted).expect("restart outbox"),
        exact_outbox
    );
    assert_eq!(
        restarted.messages(&channel).expect("restart messages")[0].markdown,
        "first private body"
    );
    assert_eq!(
        restarted.synchronization_state().expect("sync state"),
        ConversationSyncState::WaitingToSync
    );
}

#[test]
fn received_messages_order_stably_track_sparse_gaps_and_persist_local_read_position() {
    let fixture = TwoMemberWorkspace::new();
    let channel = fixture.general_id();
    let mut member = fixture.member_runtime();
    let mut records = Vec::new();
    for (index, body) in ["zero", "one", "two"].into_iter().enumerate() {
        member
            .post_message(channel, body, 20 + index as i64)
            .expect("message posts");
    }
    records.extend(testing::outbox_exact_records(&member).expect("member outbox"));
    let mut creator = fixture.creator_runtime();
    assert_eq!(
        creator
            .accept_message_record(&records[2])
            .expect("third arrives first"),
        MessageCommitOutcome::Accepted
    );
    creator
        .accept_message_record(&records[0])
        .expect("first arrives second");
    assert_eq!(
        creator
            .messages(&channel)
            .expect("ordered messages")
            .iter()
            .map(|message| message.author_sequence)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(
        creator.recovery_gaps().expect("gaps")[fixture.member.public_identity().as_bytes()][0],
        SparseRecoveryRange {
            first_sequence: 1,
            last_sequence: 1
        }
    );
    assert_eq!(creator.unread_count(channel).expect("unread"), 2);
    let messages = creator.messages(&channel).expect("messages");
    creator
        .mark_read(channel, messages[0].message_id)
        .expect("first read");
    assert_eq!(creator.unread_count(channel).expect("unread"), 1);
    creator
        .mark_read(channel, messages[1].message_id)
        .expect("second read");
    drop(creator);
    let mut restarted = fixture.creator_runtime();
    assert_eq!(restarted.unread_count(channel).expect("restart unread"), 0);
    assert_eq!(
        restarted
            .post_message(channel, "after observed Lamport", 23)
            .expect("local Lamport advances")
            .lamport,
        4
    );
}

#[test]
fn duplicates_are_idempotent_and_equivocation_quarantines_every_arrival_order() {
    for reverse in [false, true] {
        let fixture = TwoMemberWorkspace::new();
        let channel = fixture.general_id();
        let member = fixture.member_runtime();
        let first = testing::seal_runtime_message(&member, channel, "one", 7, 9, 30, [1; 24])
            .expect("first seals");
        let second = testing::seal_runtime_message(&member, channel, "two", 7, 9, 30, [2; 24])
            .expect("second seals");
        let (first, second) = if reverse {
            (second, first)
        } else {
            (first, second)
        };
        let mut creator = fixture.creator_runtime();
        assert_eq!(
            creator
                .accept_message_record(first.bytes())
                .expect("first accepts"),
            MessageCommitOutcome::Accepted
        );
        assert_eq!(
            creator
                .accept_message_record(first.bytes())
                .expect("duplicate accepts"),
            MessageCommitOutcome::Duplicate
        );
        assert_eq!(
            creator
                .accept_message_record(second.bytes())
                .expect("conflict quarantines"),
            MessageCommitOutcome::Equivocation
        );
        assert!(creator.messages(&channel).expect("projection").is_empty());
    }
}

#[test]
fn failed_outbox_insert_rolls_back_archive_clock_and_publication() {
    let fixture = TwoMemberWorkspace::new();
    let channel = fixture.general_id();
    let database = Connection::open(database(fixture.member_root.path(), &fixture.workspace))
        .expect("database opens");
    database
        .execute_batch(
            "CREATE TRIGGER fail_outbox BEFORE INSERT ON conversation_outbox
             BEGIN SELECT RAISE(FAIL, 'simulated outbox failure'); END;",
        )
        .expect("trigger");
    let mut member = fixture.member_runtime();
    assert!(member.post_message(channel, "must roll back", 40).is_err());
    assert!(testing::outbox_exact_records(&member)
        .expect("outbox")
        .is_empty());
    assert!(member.messages(&channel).expect("archive").is_empty());
    let clock: (u64, u64) = database
        .query_row(
            "SELECT next_author_sequence, lamport FROM conversation_local_clock",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("clock");
    assert_eq!(clock, (0, 0));
}

#[test]
fn canonical_history_filters_join_absence_readmission_and_next_epoch_authoring() {
    let creator_root = tempfile::tempdir().expect("creator root");
    let member_root = tempfile::tempdir().expect("member root");
    let joiner_root = tempfile::tempdir().expect("joiner root");
    let workspace = "c3".repeat(32);
    let creator = identity(34, 94);
    let member = identity(35, 95);
    let joiner = identity(36, 96);
    let creator_store = store(creator_root.path(), &workspace);
    let member_store = store(member_root.path(), &workspace);
    let joiner_store = store(joiner_root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, creator_store.clone())
            .expect("creator authority");
    let member_authority =
        ConversationAuthority::open(member.clone(), &workspace, member_store.clone())
            .expect("member authority");
    let joiner_authority =
        ConversationAuthority::open(joiner.clone(), &workspace, joiner_store.clone())
            .expect("joiner authority");
    let creator_key = authority.local_recipient_record().clone();
    let member_key = member_authority.local_recipient_record().clone();
    let joiner_key = joiner_authority.local_recipient_record().clone();
    authority
        .stage_recipient_record(member_key.bytes(), member.public_identity())
        .expect("member key");
    authority
        .stage_recipient_record(joiner_key.bytes(), joiner.public_identity())
        .expect("joiner key");

    let mut membership = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let prepared = membership
        .prepare(&workspace, genesis.clone())
        .expect("genesis prepares");
    let genesis_commit = authority
        .commit_transition_with(&prepared, &mut FixtureMaterial::new(80))
        .expect("genesis commits");
    membership.insert(genesis).expect("genesis finalizes");
    let add_member = SignedMembershipOperation::add_member(
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
    .expect("member addition");
    let member_interval = add_member.operation_id().expect("member interval");
    let prepared = membership
        .prepare(&workspace, add_member.clone())
        .expect("member prepares");
    let member_epoch = authority
        .commit_transition_with(&prepared, &mut FixtureMaterial::new(81))
        .expect("member epoch");
    membership.insert(add_member).expect("member finalizes");

    for key in [&creator_key, &member_key] {
        member_store
            .record_recipient_key(key, true)
            .expect("member key directory");
    }
    let mut member_runtime = ConversationRuntime::open(
        member.clone(),
        &workspace,
        member_store.clone(),
        membership.clone(),
    )
    .expect("member runtime");
    member_runtime
        .accept_epoch_record(member_epoch.epoch.bytes())
        .expect("member opens epoch");
    let general = genesis_commit.genesis_channel.expect("general");
    member_runtime
        .accept_channel_record(general.bytes())
        .expect("general");
    let ConversationRecordV1::Channel(general_record) = general.record() else {
        panic!("general family");
    };
    let old_message = member_runtime
        .post_message(general_record.channel_id, "eligible old interval", 3)
        .expect("old message");
    let old_exact = testing::outbox_exact_records(&member_runtime)
        .expect("old outbox")
        .into_iter()
        .find(|bytes| {
            ExactRecordV1::decode(bytes).expect("message").id() == &old_message.message_id
        })
        .expect("old exact");

    let add_joiner = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        membership
            .projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        2,
        *joiner.public_identity().as_bytes(),
        "Mia",
        4,
    )
    .expect("joiner addition");
    let prepared = membership
        .prepare(&workspace, add_joiner.clone())
        .expect("joiner prepares");
    let joiner_epoch = authority
        .commit_transition_with(&prepared, &mut FixtureMaterial::new(82))
        .expect("joiner epoch");
    membership.insert(add_joiner).expect("joiner finalizes");
    let remove_member = SignedMembershipOperation::expel_member(
        &creator,
        &workspace,
        membership
            .projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        3,
        *member.public_identity().as_bytes(),
        &member_interval,
        5,
    )
    .expect("member removal");
    let prepared = membership
        .prepare(&workspace, remove_member.clone())
        .expect("removal prepares");
    let absent_epoch = authority
        .commit_transition_with(&prepared, &mut FixtureMaterial::new(83))
        .expect("absence epoch");
    membership.insert(remove_member).expect("removal finalizes");

    let removed_runtime = ConversationRuntime::open(
        member.clone(),
        &workspace,
        member_store.clone(),
        membership.clone(),
    )
    .expect("removed member runtime");
    let mut removed_runtime = removed_runtime;
    assert!(removed_runtime
        .post_message(general_record.channel_id, "forbidden next epoch", 6)
        .is_err());

    for key in [&creator_key, &member_key, &joiner_key] {
        joiner_store
            .record_recipient_key(key, true)
            .expect("joiner key directory");
    }
    let mut joiner_runtime = ConversationRuntime::open(
        joiner.clone(),
        &workspace,
        joiner_store.clone(),
        membership.clone(),
    )
    .expect("joiner runtime");
    assert!(joiner_runtime
        .accept_epoch_record(member_epoch.epoch.bytes())
        .is_err());
    joiner_runtime
        .accept_epoch_record(joiner_epoch.epoch.bytes())
        .expect("joiner opens admission epoch");
    joiner_runtime
        .accept_epoch_record(absent_epoch.epoch.bytes())
        .expect("joiner opens absence epoch");
    joiner_runtime
        .accept_channel_record(general.bytes())
        .expect("joiner general");
    assert!(joiner_runtime.accept_message_record(&old_exact).is_err());
    let absent_message = joiner_runtime
        .post_message(general_record.channel_id, "member absent", 7)
        .expect("absence message");
    let absent_exact = testing::outbox_exact_records(&joiner_runtime)
        .expect("joiner outbox")
        .into_iter()
        .find(|bytes| {
            ExactRecordV1::decode(bytes).expect("message").id() == &absent_message.message_id
        })
        .expect("absence exact");

    let readd_member = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        membership
            .projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        4,
        *member.public_identity().as_bytes(),
        "Lin returned",
        8,
    )
    .expect("member readdition");
    let prepared = membership
        .prepare(&workspace, readd_member.clone())
        .expect("readdition prepares");
    let readmission_epoch = authority
        .commit_transition_with(&prepared, &mut FixtureMaterial::new(84))
        .expect("readmission epoch");
    membership
        .insert(readd_member)
        .expect("readdition finalizes");

    for key in [&creator_key, &member_key, &joiner_key] {
        member_store
            .record_recipient_key(key, true)
            .expect("readmitted key directory");
    }
    let mut readmitted =
        ConversationRuntime::open(member.clone(), &workspace, member_store, membership.clone())
            .expect("readmitted runtime");
    readmitted
        .accept_epoch_record(readmission_epoch.epoch.bytes())
        .expect("readmitted opens fresh epoch");
    assert!(readmitted.accept_message_record(&absent_exact).is_err());

    let mut creator_runtime =
        ConversationRuntime::open(creator, &workspace, creator_store, membership)
            .expect("creator runtime");
    creator_runtime
        .accept_message_record(&old_exact)
        .expect("historical member message accepts");
    creator_runtime
        .accept_message_record(&absent_exact)
        .expect("absence message accepts");
    let member_eligible = testing::records_eligible_for(&creator_runtime, member.public_identity())
        .expect("member eligibility");
    let joiner_eligible = testing::records_eligible_for(&creator_runtime, joiner.public_identity())
        .expect("joiner eligibility");
    assert!(member_eligible.contains(&old_exact));
    assert!(!member_eligible.contains(&absent_exact));
    assert!(!joiner_eligible.contains(&old_exact));
    assert!(joiner_eligible.contains(&absent_exact));
}

#[test]
fn persisted_pending_departure_blocks_only_local_authoring_and_reports_waiting() {
    let fixture = TwoMemberWorkspace::new();
    let interval = fixture
        .membership
        .projection(&fixture.workspace)
        .interval_id(&fixture.member.public_identity())
        .expect("member interval")
        .clone();
    let request = SignedSelfRemovalRequestV1::create_with_nonce(
        &fixture.member,
        &fixture.workspace,
        interval.as_str(),
        *fixture.creator.public_identity().as_bytes(),
        [97; 32],
        1,
    )
    .expect("request signs");
    fixture
        .member_store
        .record_departure_request(&request)
        .expect("request persists");
    let mut member = fixture.member_runtime();
    assert!(member
        .post_message(fixture.general_id(), "blocked", 1)
        .is_err());
    assert_eq!(
        member.synchronization_state().expect("sync state"),
        ConversationSyncState::WaitingToSync
    );
    let mut creator = fixture.creator_runtime();
    assert!(creator
        .post_message(fixture.general_id(), "other member remains authorized", 2)
        .is_ok());
}

#[test]
fn stale_channel_head_is_rejected_and_one_member_outbox_is_locally_current() {
    let root = tempfile::tempdir().expect("root");
    let workspace = "c2".repeat(32);
    let creator = identity(33, 93);
    let store = store(root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("authority");
    let mut membership = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let prepared = membership
        .prepare(&workspace, genesis.clone())
        .expect("prepare");
    authority
        .commit_transition_with(&prepared, &mut FixtureMaterial::new(72))
        .expect("commit");
    membership.insert(genesis).expect("finalize");
    let mut runtime =
        ConversationRuntime::open(creator, &workspace, store, membership).expect("runtime");
    let created = runtime
        .create_channel("#temporary", 2)
        .expect("channel creates");
    let stale_head = created.head;
    runtime
        .rename_channel(created.channel_id, "#renamed", 3)
        .expect("channel renames");
    let stale = testing::seal_runtime_message_for_head(
        &runtime,
        created.channel_id,
        stale_head,
        "stale",
        99,
        99,
        4,
        [9; 24],
    )
    .expect("stale fixture seals");
    assert!(runtime.accept_message_record(stale.bytes()).is_err());
    runtime
        .post_message(created.channel_id, "current", 5)
        .expect("current message posts");
    assert_eq!(
        runtime.synchronization_state().expect("one-member state"),
        ConversationSyncState::Current
    );
    assert_eq!(
        runtime
            .network_synchronization_state(false, false)
            .expect("one-member network state"),
        ConversationSyncState::Current,
        "local publication duties do not make a one-member workspace wait"
    );
}

#[test]
fn local_channel_snapshot_refuses_the_257th_channel_without_persisting_it() {
    let fixture = TwoMemberWorkspace::new();
    let mut runtime = fixture.creator_runtime();
    for index in 1..MAX_CHANNEL_SNAPSHOT {
        runtime
            .create_channel(format!("#channel-{index}"), index as i64)
            .expect("channel within snapshot limit persists");
    }
    assert_eq!(runtime.channels().len(), MAX_CHANNEL_SNAPSHOT);
    assert!(matches!(
        runtime.create_channel("#too-many", 999),
        Err(testing::runtime::ConversationRuntimeError::Conversation(
            ConversationError::SizeLimit { .. }
        ))
    ));
    drop(runtime);
    assert_eq!(
        fixture.creator_runtime().channels().len(),
        MAX_CHANNEL_SNAPSHOT,
        "the refused channel has no durable record"
    );
}

#[test]
fn recovered_channel_catalog_refuses_a_deterministic_257th_snapshot_chain() {
    let fixture = TwoMemberWorkspace::new();
    let ConversationRecordV1::Epoch(epoch) = fixture.current_epoch.record() else {
        panic!("current epoch");
    };
    let mut remote_chains = Vec::new();
    let mut remote_records = Vec::new();
    for index in 0..MAX_CHANNEL_SNAPSHOT {
        let mut channel_id = [0_u8; 16];
        channel_id[14..].copy_from_slice(&(index as u16).to_be_bytes());
        let exact = ExactRecordV1::author(
            ConversationRecordV1::Channel(ChannelRecordV1 {
                workspace_id: [0xc1; 32],
                channel_id,
                authorization_epoch: epoch.resulting_membership_head,
                creator: *fixture.member.public_identity().as_bytes(),
                author: *fixture.member.public_identity().as_bytes(),
                author_sequence: index as u64,
                created_at: index as i64,
                predecessor: None,
                operation: ChannelOperationV1::Create {
                    name: format!("#remote-{index}"),
                },
            }),
            &fixture.member,
        )
        .expect("remote channel signs");
        remote_chains.push((*exact.id(), channel_id));
        remote_records.push(exact.bytes().to_vec());
    }

    let mut runtime = fixture.creator_runtime();
    for page in remote_records.chunks(128) {
        let mut records = page.to_vec();
        records.sort_by_key(|bytes| blake3::hash(bytes).as_bytes().to_owned());
        let response = ExactRecordV1::author(
            ConversationRecordV1::RecoveryResponse(RecoveryResponseV1 {
                workspace_id: [0xc1; 32],
                sender: *fixture.member.public_identity().as_bytes(),
                records,
            }),
            &fixture.member,
        )
        .expect("catalog response signs");
        runtime
            .accept_recovery_response(fixture.member.public_identity(), response.bytes())
            .expect("catalog page applies");
    }

    remote_chains.push((*fixture.general.id(), fixture.general_id()));
    remote_chains.sort_unstable();
    let refused_channel = remote_chains[MAX_CHANNEL_SNAPSHOT].1;
    assert_eq!(runtime.channels().len(), MAX_CHANNEL_SNAPSHOT);
    assert!(
        !runtime
            .channels()
            .iter()
            .any(|channel| channel.channel_id == refused_channel),
        "the deterministic overflow chain is not exposed"
    );
    assert_eq!(
        runtime.diagnostic_count().expect("diagnostic count"),
        1,
        "the overflow exact record is retained as a bounded diagnostic"
    );
}

#[test]
fn malformed_diagnostic_custody_is_bounded() {
    let fixture = TwoMemberWorkspace::new();
    let mut runtime = fixture.creator_runtime();
    for index in 0..300_u16 {
        let bytes = index.to_be_bytes();
        assert!(runtime.accept_channel_record(&bytes).is_err());
    }
    assert_eq!(runtime.diagnostic_count().expect("diagnostic count"), 256);
}

#[test]
fn database_contains_only_exact_ciphertext_records_not_plaintext_or_keys() {
    let fixture = TwoMemberWorkspace::new();
    let channel = fixture.general_id();
    let mut member = fixture.member_runtime();
    member
        .post_message(channel, "plaintext-must-not-appear", 50)
        .expect("message posts");
    let database = Connection::open(database(fixture.member_root.path(), &fixture.workspace))
        .expect("database opens");
    let archive: Vec<u8> = database
        .query_row(
            "SELECT exact_record FROM conversation_message_archive",
            [],
            |row| row.get(0),
        )
        .expect("archive bytes");
    let outbox: Vec<u8> = database
        .query_row("SELECT exact_record FROM conversation_outbox", [], |row| {
            row.get(0)
        })
        .expect("outbox bytes");
    assert_eq!(archive, outbox);
    assert!(!archive
        .windows(b"plaintext-must-not-appear".len())
        .any(|window| window == b"plaintext-must-not-appear"));
    assert!(!archive.windows(32).any(|window| window == [70; 32]));
    assert!(!archive.windows(32).any(|window| window == [71; 32]));
    assert!(!archive.windows(32).any(|window| window == [92; 32]));
}
