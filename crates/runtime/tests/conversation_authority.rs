use std::path::{Path, PathBuf};

use resonance_runtime::{
    conversations::{
        testing::{
            self,
            authority::{ConversationAuthority, ConversationAuthorityError, EpochMaterialProvider},
            crypto::{
                self, EpochEnvelopeContextV1, EpochKey, HpkeEpochEnvelopeV1, RecipientPrivateKey,
            },
            runtime::ConversationRuntime,
            wire::{ChannelOperationV1, ChannelRecordV1, ConversationRecordV1, ExactRecordV1},
        },
        ConversationError,
    },
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{MembershipLog, SignedMembershipOperation, SignedSelfRemovalRequestV1},
    workspace_store::WorkspaceStore,
};
use rusqlite::Connection;

fn identity(seed: u8) -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![seed; 32]))
        .expect("identity loads")
}

fn open_store(root: &Path, workspace: &str) -> WorkspaceStore {
    WorkspaceStore::open(root, workspace).expect("store opens")
}

fn database(root: &Path, workspace: &str) -> PathBuf {
    root.join(".resonance/workspaces")
        .join(workspace)
        .join("workspace.sqlite3")
}

struct DeterministicMaterial {
    next: u8,
    generate_calls: usize,
    wrap_calls: usize,
    open_calls: usize,
}

impl DeterministicMaterial {
    fn new(next: u8) -> Self {
        Self {
            next,
            generate_calls: 0,
            wrap_calls: 0,
            open_calls: 0,
        }
    }
}

impl EpochMaterialProvider for DeterministicMaterial {
    fn generate_epoch_key(&mut self) -> Result<EpochKey, ConversationError> {
        self.generate_calls += 1;
        Ok(testing::epoch_key([self.next; 32]))
    }

    fn wrap_epoch_key(
        &mut self,
        key: &EpochKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
        self.wrap_calls += 1;
        let randomness = [self.next.wrapping_add(self.wrap_calls as u8); 32];
        testing::wrap_epoch_key(key, context, randomness)
    }

    fn open_epoch_key(
        &mut self,
        envelope: &HpkeEpochEnvelopeV1,
        recipient: &RecipientPrivateKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<EpochKey, ConversationError> {
        self.open_calls += 1;
        crypto::unwrap_epoch_key(envelope, recipient, context)
    }
}

fn hex_head(value: &str) -> [u8; 32] {
    let mut output = [0; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        output[index] =
            u8::from_str_radix(std::str::from_utf8(pair).expect("hex"), 16).expect("hex byte");
    }
    output
}

struct FailIfCalled;

impl EpochMaterialProvider for FailIfCalled {
    fn generate_epoch_key(&mut self) -> Result<EpochKey, ConversationError> {
        panic!("requester must not generate the next epoch key")
    }

    fn wrap_epoch_key(
        &mut self,
        _key: &EpochKey,
        _context: &EpochEnvelopeContextV1,
    ) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
        panic!("requester must not wrap the next epoch key")
    }

    fn open_epoch_key(
        &mut self,
        _envelope: &HpkeEpochEnvelopeV1,
        _recipient: &RecipientPrivateKey,
        _context: &EpochEnvelopeContextV1,
    ) -> Result<EpochKey, ConversationError> {
        panic!("requester must not open the next epoch key")
    }
}

#[test]
fn atomic_genesis_creates_epoch_general_channel_and_durable_duties() {
    let root = tempfile::tempdir().expect("root");
    let workspace = "a1".repeat(32);
    let creator = identity(21);
    let store = open_store(root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("opens");
    let log = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis signs");
    let prepared = log.prepare(&workspace, genesis).expect("genesis prepares");
    let mut material = DeterministicMaterial::new(31);

    let committed = authority
        .commit_transition_with(&prepared, &mut material)
        .expect("genesis atomically commits");

    assert_eq!(material.generate_calls, 1);
    assert_eq!(material.wrap_calls, 1);
    assert_eq!(material.open_calls, 1);
    assert!(committed.genesis_channel.is_some());
    assert_eq!(committed.lookup_peer_set_version, 1);
    assert_eq!(
        store.membership_operation_ids().expect("operations").len(),
        1
    );
    let duties = store.durable_publication_duties().expect("duties");
    assert_eq!(
        duties
            .iter()
            .filter(|d| d.transport == "iroh-membership")
            .count(),
        1
    );
    assert_eq!(
        duties
            .iter()
            .filter(|d| d.transport == "commonware-record")
            .count(),
        2
    );
    let exact = store
        .exact_epoch_for_head(&match committed.epoch.record() {
            ConversationRecordV1::Epoch(epoch) => epoch.resulting_membership_head,
            _ => panic!("epoch family"),
        })
        .expect("epoch query")
        .expect("epoch exists");
    assert_eq!(exact, committed.epoch.bytes());
}

#[test]
fn three_identity_branch_replacement_invalidates_losing_runtime_epoch() {
    let root = tempfile::tempdir().expect("root");
    let first_root = tempfile::tempdir().expect("first root");
    let second_root = tempfile::tempdir().expect("second root");
    let workspace = "a6".repeat(32);
    let creator = identity(110);
    let first_member = identity(111);
    let second_member = identity(112);
    let store = open_store(root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("authority");
    for (member, root) in [
        (&first_member, first_root.path()),
        (&second_member, second_root.path()),
    ] {
        let member_authority =
            ConversationAuthority::open(member.clone(), &workspace, open_store(root, &workspace))
                .expect("member authority");
        authority
            .stage_recipient_record(
                member_authority.local_recipient_record().bytes(),
                member.public_identity(),
            )
            .expect("member key stages");
    }
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let first = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &genesis_id,
        1,
        *first_member.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("first branch");
    let second = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &genesis_id,
        2,
        *second_member.public_identity().as_bytes(),
        "Mia",
        2,
    )
    .expect("second branch");
    let (winner, loser) =
        if first.operation_id().expect("first ID") < second.operation_id().expect("second ID") {
            (first, second)
        } else {
            (second, first)
        };
    let mut losing_log = MembershipLog::new();
    let genesis_prepared = losing_log
        .prepare(&workspace, genesis.clone())
        .expect("genesis prepares");
    authority
        .commit_transition_with(&genesis_prepared, &mut DeterministicMaterial::new(92))
        .expect("genesis commits");
    losing_log.insert(genesis).expect("genesis finalizes");
    let losing_prepared = losing_log
        .prepare(&workspace, loser.clone())
        .expect("losing branch prepares while alone");
    let losing_epoch = authority
        .commit_transition_with(&losing_prepared, &mut DeterministicMaterial::new(93))
        .expect("losing epoch commits");
    losing_log.insert(loser).expect("losing branch finalizes");
    let ConversationRecordV1::Epoch(epoch) = losing_epoch.epoch.record() else {
        panic!("epoch");
    };
    let losing_head = epoch.resulting_membership_head;
    let mut runtime =
        ConversationRuntime::open(creator, &workspace, store.clone(), losing_log.clone())
            .expect("runtime opens losing head");
    losing_log.insert(winner).expect("winner arrives");

    runtime
        .replace_membership(losing_log)
        .expect("runtime replaces canonical branch");

    assert!(store
        .exact_epoch_for_head(&losing_head)
        .expect("epoch query")
        .is_none());
    assert!(store
        .durable_publication_duties()
        .expect("duties")
        .iter()
        .all(|duty| duty.membership_head != Some(losing_head)));
}

#[test]
fn losing_head_invalidation_preserves_exact_diagnostics_and_cancels_duties() {
    let root = tempfile::tempdir().expect("root");
    let workspace = "a5".repeat(32);
    let creator = identity(28);
    let store = open_store(root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("opens");
    let log = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let prepared = log.prepare(&workspace, genesis).expect("prepare");
    let committed = authority
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(80))
        .expect("commit");
    let ConversationRecordV1::Epoch(epoch) = committed.epoch.record() else {
        panic!("epoch");
    };
    let head = epoch.resulting_membership_head;

    store
        .invalidate_conversation_head(&head)
        .expect("losing head invalidates");

    assert!(store
        .exact_epoch_for_head(&head)
        .expect("epoch query")
        .is_none());
    assert!(store
        .durable_publication_duties()
        .expect("duties")
        .iter()
        .all(|duty| duty.membership_head != Some(head)));
    let connection = Connection::open(database(root.path(), &workspace)).expect("database");
    let diagnostic: Vec<u8> = connection
        .query_row(
            "SELECT exact_record FROM conversation_epochs WHERE resulting_membership_head = ?1 AND accepted = 0",
            [head.as_slice()],
            |row| row.get(0),
        )
        .expect("diagnostic bytes remain");
    assert_eq!(diagnostic, committed.epoch.bytes());
}

#[test]
fn missing_recipient_key_fails_before_epoch_generation_or_storage() {
    let root = tempfile::tempdir().expect("root");
    let member_root = tempfile::tempdir().expect("member root");
    let workspace = "a2".repeat(32);
    let creator = identity(22);
    let member = identity(23);
    let store = open_store(root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("opens");
    let mut log = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let prepared_genesis = log.prepare(&workspace, genesis.clone()).expect("prepare");
    authority
        .commit_transition_with(&prepared_genesis, &mut DeterministicMaterial::new(40))
        .expect("genesis commits");
    log.insert(genesis).expect("genesis finalizes");
    let addition = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        log.projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("addition");
    let prepared = log
        .prepare(&workspace, addition)
        .expect("addition prepares");
    let mut fail = FailIfCalled;

    assert!(matches!(
        authority.commit_transition_with(&prepared, &mut fail),
        Err(ConversationAuthorityError::MissingRecipientKey(identity)) if identity == member.public_identity()
    ));
    assert_eq!(
        store.membership_operation_ids().expect("operations").len(),
        1
    );

    let member_authority = ConversationAuthority::open(
        member.clone(),
        &workspace,
        open_store(member_root.path(), &workspace),
    )
    .expect("member authority");
    authority
        .stage_recipient_record(
            member_authority.local_recipient_record().bytes(),
            member.public_identity(),
        )
        .expect("prospective recipient record stages");
    authority
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(41))
        .expect("addition commits once key exists");
    assert_eq!(
        store.recipient_key_records().expect("accepted keys").len(),
        2
    );
}

#[test]
fn requester_never_generates_opens_or_receives_the_next_epoch() {
    let root = tempfile::tempdir().expect("root");
    let requester_root = tempfile::tempdir().expect("requester root");
    let survivor_root = tempfile::tempdir().expect("survivor root");
    let workspace = "a3".repeat(32);
    let creator = identity(24);
    let requester = identity(25);
    let survivor = identity(29);
    let store = open_store(root.path(), &workspace);
    let requester_store = open_store(requester_root.path(), &workspace);
    let mut creator_authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("opens");
    let requester_authority =
        ConversationAuthority::open(requester.clone(), &workspace, requester_store.clone())
            .expect("requester opens");
    let requester_record = requester_authority
        .local_recipient_record()
        .bytes()
        .to_vec();
    creator_authority
        .stage_recipient_record(&requester_record, requester.public_identity())
        .expect("requester key stages");
    let survivor_authority = ConversationAuthority::open(
        survivor.clone(),
        &workspace,
        open_store(survivor_root.path(), &workspace),
    )
    .expect("survivor opens");
    creator_authority
        .stage_recipient_record(
            survivor_authority.local_recipient_record().bytes(),
            survivor.public_identity(),
        )
        .expect("survivor key stages");

    let mut log = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let genesis_prepared = log.prepare(&workspace, genesis.clone()).expect("prepare");
    creator_authority
        .commit_transition_with(&genesis_prepared, &mut DeterministicMaterial::new(50))
        .expect("genesis commits");
    log.insert(genesis).expect("genesis finalizes");
    let addition = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        log.projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        1,
        *requester.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("addition");
    let addition_prepared = log.prepare(&workspace, addition.clone()).expect("prepare");
    creator_authority
        .commit_transition_with(&addition_prepared, &mut DeterministicMaterial::new(51))
        .expect("addition commits");
    log.insert(addition).expect("addition finalizes");
    let survivor_addition = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        log.projection(&workspace)
            .canonical_head
            .expect("head")
            .to_string(),
        2,
        *survivor.public_identity().as_bytes(),
        "Mia",
        3,
    )
    .expect("survivor addition");
    let survivor_prepared = log
        .prepare(&workspace, survivor_addition.clone())
        .expect("survivor prepares");
    creator_authority
        .commit_transition_with(&survivor_prepared, &mut DeterministicMaterial::new(52))
        .expect("survivor commits");
    log.insert(survivor_addition).expect("survivor finalizes");

    let projection = log.projection(&workspace);
    let interval = projection
        .interval_id(&requester.public_identity())
        .expect("requester interval")
        .clone();
    let request = SignedSelfRemovalRequestV1::create_with_nonce(
        &requester,
        &workspace,
        interval.as_str(),
        *creator.public_identity().as_bytes(),
        [52; 32],
        3,
    )
    .expect("request");
    store
        .record_departure_request(&request)
        .expect("request persists before processing");
    let removal = SignedMembershipOperation::remove_member_by_request(
        &creator,
        &workspace,
        projection.canonical_head.expect("head").to_string(),
        3,
        request,
        4,
    )
    .expect("removal");
    let prepared = log.prepare(&workspace, removal).expect("removal prepares");

    let mut requester_attempt =
        ConversationAuthority::open(requester.clone(), &workspace, requester_store.clone())
            .expect("requester reopens");
    assert!(matches!(
        requester_attempt.commit_transition_with(&prepared, &mut FailIfCalled),
        Err(ConversationAuthorityError::UnauthorizedCoordinator)
    ));

    let committed = creator_authority
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(53))
        .expect("creator commits next epoch");
    let ConversationRecordV1::Epoch(epoch) = committed.epoch.record() else {
        panic!("epoch family");
    };
    assert_eq!(epoch.recipients.len(), 2);
    assert!(epoch
        .recipients
        .iter()
        .any(|entry| entry.member == *creator.public_identity().as_bytes()));
    assert!(epoch
        .recipients
        .iter()
        .any(|entry| entry.member == *survivor.public_identity().as_bytes()));
    assert!(epoch
        .recipients
        .iter()
        .all(|entry| entry.member != *requester.public_identity().as_bytes()));
    assert!(requester_store
        .exact_epoch_for_head(&epoch.resulting_membership_head)
        .expect("requester epoch query")
        .is_none());
    assert_eq!(
        store
            .recipient_key_records()
            .expect("active recipient keys")
            .len(),
        2
    );
    assert!(store
        .pending_departure_requests()
        .expect("processed requests")
        .is_empty());
}

#[test]
fn one_member_v10_workspace_bootstraps_once_and_missing_coordinator_waits() {
    let root = tempfile::tempdir().expect("root");
    let workspace = "ab".repeat(32);
    let path = database(root.path(), &workspace);
    std::fs::create_dir_all(path.parent().expect("database parent")).expect("directory");
    Connection::open(&path)
        .expect("legacy database")
        .execute_batch(include_str!("fixtures/legacy_workspace_v10.sql"))
        .expect("legacy fixture");
    let store = open_store(root.path(), &workspace);
    let genesis =
        SignedMembershipOperation::decode(include_bytes!("fixtures/membership/v1/genesis.bin"))
            .expect("genesis fixture");
    let mut log = MembershipLog::new();
    log.insert(genesis).expect("genesis inserts");
    let prepared = log
        .current_transition(&workspace)
        .expect("current transition");

    let other = identity(27);
    let mut unavailable =
        ConversationAuthority::open(other, &workspace, store.clone()).expect("other opens");
    assert!(matches!(
        unavailable.commit_transition_with(&prepared, &mut FailIfCalled),
        Err(ConversationAuthorityError::UnauthorizedCoordinator)
    ));
    assert!(store
        .exact_epoch_for_head(&hex_head(prepared.resulting_head.as_str()))
        .expect("epoch query")
        .is_none());

    let creator = identity(41);
    let mut creator_authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("creator");
    let first = creator_authority
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(70))
        .expect("bootstrap commits");
    let mut restarted =
        ConversationAuthority::open(creator, &workspace, store.clone()).expect("restart");
    let second = restarted
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(71))
        .expect("bootstrap reuses");
    assert_eq!(first.epoch.bytes(), second.epoch.bytes());
    assert_eq!(store.lookup_peer_set_version().expect("version"), 1);
}

struct ChannelFixture {
    _root: tempfile::TempDir,
    _member_root: tempfile::TempDir,
    workspace: String,
    creator: InstallationIdentity,
    member: InstallationIdentity,
    membership: MembershipLog,
    store: WorkspaceStore,
    epoch: [u8; 32],
}

impl ChannelFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("root");
        let member_root = tempfile::tempdir().expect("member root");
        let workspace = "d1".repeat(32);
        let creator = identity(101);
        let member = identity(102);
        let store = open_store(root.path(), &workspace);
        let mut authority = ConversationAuthority::open(creator.clone(), &workspace, store.clone())
            .expect("authority");
        let member_authority = ConversationAuthority::open(
            member.clone(),
            &workspace,
            open_store(member_root.path(), &workspace),
        )
        .expect("member authority");
        authority
            .stage_recipient_record(
                member_authority.local_recipient_record().bytes(),
                member.public_identity(),
            )
            .expect("member key stages");
        let mut membership = MembershipLog::new();
        let genesis =
            SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
        let prepared = membership
            .prepare(&workspace, genesis.clone())
            .expect("genesis prepares");
        authority
            .commit_transition_with(&prepared, &mut DeterministicMaterial::new(90))
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
            .expect("addition prepares");
        let committed = authority
            .commit_transition_with(&prepared, &mut DeterministicMaterial::new(91))
            .expect("addition commits");
        membership.insert(addition).expect("addition finalizes");
        let ConversationRecordV1::Epoch(epoch) = committed.epoch.record() else {
            panic!("epoch family");
        };
        Self {
            _root: root,
            _member_root: member_root,
            workspace,
            creator,
            member,
            membership,
            store,
            epoch: epoch.resulting_membership_head,
        }
    }

    fn runtime(&self) -> ConversationRuntime {
        ConversationRuntime::open(
            self.creator.clone(),
            &self.workspace,
            self.store.clone(),
            self.membership.clone(),
        )
        .expect("runtime opens")
    }

    fn record(
        &self,
        signer: &InstallationIdentity,
        channel_id: [u8; 16],
        creator: [u8; 32],
        sequence: u64,
        predecessor: Option<[u8; 32]>,
        operation: ChannelOperationV1,
    ) -> ExactRecordV1 {
        ExactRecordV1::author(
            ConversationRecordV1::Channel(ChannelRecordV1 {
                workspace_id: hex_head(&self.workspace),
                channel_id,
                authorization_epoch: self.epoch,
                creator,
                author: *signer.public_identity().as_bytes(),
                author_sequence: sequence,
                created_at: sequence as i64 + 10,
                predecessor,
                operation,
            }),
            signer,
        )
        .expect("channel record signs")
    }
}

#[test]
fn epoch_acceptance_rejects_semantic_recipient_coordinator_and_key_mismatches() {
    let fixture = ChannelFixture::new();
    let bytes = fixture
        .store
        .exact_epoch_for_head(&fixture.epoch)
        .expect("epoch query")
        .expect("epoch bytes");
    let exact = ExactRecordV1::decode(&bytes).expect("epoch decodes");
    let ConversationRecordV1::Epoch(valid) = exact.record() else {
        panic!("epoch family");
    };
    let mut runtime = fixture.runtime();

    let mut missing_recipient = valid.clone();
    missing_recipient.recipients.pop();
    let missing_recipient = ExactRecordV1::author(
        ConversationRecordV1::Epoch(missing_recipient),
        &fixture.creator,
    )
    .expect("semantic-invalid epoch signs");
    assert!(runtime
        .accept_epoch_record(missing_recipient.bytes())
        .is_err());

    let mut wrong_previous = valid.clone();
    wrong_previous.previous_membership_head = None;
    let wrong_previous = ExactRecordV1::author(
        ConversationRecordV1::Epoch(wrong_previous),
        &fixture.creator,
    )
    .expect("wrong previous signs");
    assert!(runtime.accept_epoch_record(wrong_previous.bytes()).is_err());

    let mut wrong_key = valid.clone();
    wrong_key.recipients[0].recipient_key_record_id = [0; 32];
    let wrong_key = ExactRecordV1::author(ConversationRecordV1::Epoch(wrong_key), &fixture.creator)
        .expect("wrong key signs");
    assert!(runtime.accept_epoch_record(wrong_key.bytes()).is_err());

    let mut wrong_coordinator = valid.clone();
    wrong_coordinator.coordinator = *fixture.member.public_identity().as_bytes();
    let wrong_coordinator = ExactRecordV1::author(
        ConversationRecordV1::Epoch(wrong_coordinator),
        &fixture.member,
    )
    .expect("wrong coordinator signs");
    assert!(runtime
        .accept_epoch_record(wrong_coordinator.bytes())
        .is_err());
}

#[test]
fn channel_replay_enforces_general_creator_lifecycle_normalization_and_terminal_archive() {
    let fixture = ChannelFixture::new();
    let mut runtime = fixture.runtime();
    assert_eq!(runtime.channels().len(), 1);
    assert_eq!(runtime.channels()[0].name, "#general");

    let channel_id = [7; 16];
    let create = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        0,
        None,
        ChannelOperationV1::Create {
            name: "#design".to_owned(),
        },
    );
    assert!(runtime
        .accept_channel_record(create.bytes())
        .expect("member create applies"));
    let unauthorized = fixture.record(
        &fixture.creator,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        1,
        Some(*create.id()),
        ChannelOperationV1::Rename {
            name: "#owner-only".to_owned(),
        },
    );
    assert!(!runtime
        .accept_channel_record(unauthorized.bytes())
        .expect("non-creator extension is diagnostic"));
    let rename = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        1,
        Some(*create.id()),
        ChannelOperationV1::Rename {
            name: "#product".to_owned(),
        },
    );
    assert!(runtime
        .accept_channel_record(rename.bytes())
        .expect("creator rename applies"));
    let archive = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        2,
        Some(*rename.id()),
        ChannelOperationV1::Archive,
    );
    assert!(runtime
        .accept_channel_record(archive.bytes())
        .expect("archive applies"));
    assert!(runtime
        .post_message(channel_id, "archived channels reject posts", 20)
        .is_err());
    let after_archive = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        3,
        Some(*archive.id()),
        ChannelOperationV1::Rename {
            name: "#resurrected".to_owned(),
        },
    );
    assert!(!runtime
        .accept_channel_record(after_archive.bytes())
        .expect("terminal child is diagnostic"));
    let invalid_name = fixture.record(
        &fixture.member,
        [8; 16],
        *fixture.member.public_identity().as_bytes(),
        4,
        None,
        ChannelOperationV1::Create {
            name: " #not-normalized".to_owned(),
        },
    );
    assert!(!runtime
        .accept_channel_record(invalid_name.bytes())
        .expect("invalid name is diagnostic"));
    assert!(runtime
        .channels()
        .iter()
        .any(|channel| channel.channel_id == channel_id && channel.archived));
    assert!(runtime.diagnostic_count().expect("diagnostics") >= 3);
}

#[test]
fn channel_predecessor_and_active_name_conflicts_are_arrival_order_independent() {
    let fixture = ChannelFixture::new();
    let channel_id = [9; 16];
    let create = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        0,
        None,
        ChannelOperationV1::Create {
            name: "#initial".to_owned(),
        },
    );
    let left = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        1,
        Some(*create.id()),
        ChannelOperationV1::Rename {
            name: "#left".to_owned(),
        },
    );
    let right = fixture.record(
        &fixture.member,
        channel_id,
        *fixture.member.public_identity().as_bytes(),
        2,
        Some(*create.id()),
        ChannelOperationV1::Rename {
            name: "#right".to_owned(),
        },
    );
    let winning_name = if left.id() < right.id() {
        "#left"
    } else {
        "#right"
    };
    let first_claim = fixture.record(
        &fixture.member,
        [10; 16],
        *fixture.member.public_identity().as_bytes(),
        3,
        None,
        ChannelOperationV1::Create {
            name: "#collision".to_owned(),
        },
    );
    let second_claim = fixture.record(
        &fixture.creator,
        [11; 16],
        *fixture.creator.public_identity().as_bytes(),
        0,
        None,
        ChannelOperationV1::Create {
            name: "#COLLISION".to_owned(),
        },
    );
    let winning_claim = if first_claim.id() < second_claim.id() {
        [10; 16]
    } else {
        [11; 16]
    };

    let mut forward = fixture.runtime();
    for record in [&create, &left, &right, &first_claim, &second_claim] {
        forward
            .accept_channel_record(record.bytes())
            .expect("record replays");
    }
    let second_fixture = ChannelFixture::new();
    let mut reverse = second_fixture.runtime();
    for record in [&second_claim, &first_claim, &right, &left, &create] {
        reverse
            .accept_channel_record(record.bytes())
            .expect("record replays in reverse");
    }
    let summarize = |runtime: &ConversationRuntime| {
        runtime
            .channels()
            .into_iter()
            .map(|channel| (channel.channel_id, channel.name, channel.archived))
            .collect::<Vec<_>>()
    };
    assert_eq!(summarize(&forward), summarize(&reverse));
    assert!(forward
        .channels()
        .iter()
        .any(|channel| channel.channel_id == channel_id && channel.name == winning_name));
    assert!(forward
        .channels()
        .iter()
        .any(|channel| channel.channel_id == winning_claim));
    assert_eq!(
        forward
            .channels()
            .iter()
            .filter(|channel| channel.name.eq_ignore_ascii_case("#collision"))
            .count(),
        1
    );
}

#[test]
fn transaction_failure_exposes_no_membership_or_publication_and_restart_reuses_epoch() {
    let root = tempfile::tempdir().expect("root");
    let workspace = "a4".repeat(32);
    let creator = identity(26);
    let store = open_store(root.path(), &workspace);
    let mut authority =
        ConversationAuthority::open(creator.clone(), &workspace, store.clone()).expect("opens");
    let log = MembershipLog::new();
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let prepared = log.prepare(&workspace, genesis).expect("prepare");
    let database = Connection::open(database(root.path(), &workspace)).expect("database");
    database
        .execute_batch(
            "CREATE TRIGGER fail_epoch BEFORE INSERT ON conversation_epochs
             BEGIN SELECT RAISE(FAIL, 'simulated epoch failure'); END;",
        )
        .expect("trigger");
    assert!(authority
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(60))
        .is_err());
    assert!(store
        .membership_operation_ids()
        .expect("operations")
        .is_empty());
    assert!(store
        .durable_publication_duties()
        .expect("duties")
        .is_empty());
    database
        .execute("DROP TRIGGER fail_epoch", [])
        .expect("trigger drops");

    let first = authority
        .commit_transition_with(&prepared, &mut DeterministicMaterial::new(61))
        .expect("retry commits");
    let first_bytes = first.epoch.bytes().to_vec();
    let mut reopened =
        ConversationAuthority::open(creator, &workspace, store.clone()).expect("reopens");
    let mut retry_material = DeterministicMaterial::new(99);
    let second = reopened
        .commit_transition_with(&prepared, &mut retry_material)
        .expect("post-commit finalization retry reuses epoch");
    assert_eq!(second.epoch.bytes(), first_bytes);
    assert_eq!(retry_material.generate_calls, 0);
    assert_eq!(retry_material.wrap_calls, 0);
    assert_eq!(retry_material.open_calls, 1);
    assert_eq!(store.lookup_peer_set_version().expect("version"), 1);
}
