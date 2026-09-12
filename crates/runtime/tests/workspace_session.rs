use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use resonance_runtime::{
    conversations::testing::{
        authority::ConversationAuthority,
        wire::{ConversationRecordV1, ExactRecordV1},
    },
    identity::{InMemoryKeyCustody, InstallationIdentity},
    invite::Invite,
    membership_log::{MembershipOperationBody, SignedMembershipOperation},
    protocol::{Envelope, EnvelopeBody},
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{PeerConnection, WorkspaceLifecycle},
    workspace_files::FileOperationBody,
    workspace_session::{FakeDeliveryPort, WorkspaceSession, WorkspaceTransition},
};
use rusqlite::Connection;

fn temporary_directory(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("resonance-{name}-{nonce}"));
    fs::create_dir_all(&directory).expect("temporary directory creates");
    directory
}

fn session(directory: &PathBuf) -> WorkspaceSession<FakeDeliveryPort> {
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let catalog = WorkspaceCatalog::open(directory).expect("catalog opens");
    WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default())
}

#[test]
fn creates_a_signed_plans_directory_before_the_workspace_is_ready() {
    let directory = temporary_directory("initial-file-operation");
    let mut workspace = session(&directory);
    let created = workspace
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");

    assert_eq!(created.workspace.lifecycle, WorkspaceLifecycle::Ready);
    let catalog = WorkspaceCatalog::open(&directory).expect("catalog reopens");
    let store = catalog
        .open_workspace(&created.workspace.id)
        .expect("workspace store reopens");
    let operations = store.file_operations().expect("file operations load");
    assert_eq!(operations.len(), 1);
    assert!(operations[0].verify().is_ok());
    assert_eq!(
        operations[0].operation.body,
        FileOperationBody::CreateDirectory {
            parent_node_id: None,
            name: "plans".to_owned()
        }
    );

    fs::remove_dir_all(directory).expect("directory removes");
}

#[test]
fn restart_completes_initialization_without_duplicate_genesis_or_plans_operation() {
    let directory = temporary_directory("initialization-recovery");
    let custody = InMemoryKeyCustody::default();
    let mut workspace = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity creates"),
        WorkspaceCatalog::open(&directory).expect("catalog opens"),
        FakeDeliveryPort::default(),
    );
    let created = workspace
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    drop(workspace);

    let workspace_database = directory
        .join(".resonance/workspaces")
        .join(created.workspace.id.as_str())
        .join("workspace.sqlite3");
    let workspace_connection =
        Connection::open(&workspace_database).expect("workspace database opens");
    workspace_connection
        .execute(
            "UPDATE workspace_configuration SET lifecycle = 'initializing' WHERE singleton = 1",
            [],
        )
        .expect("workspace lifecycle rewinds");

    let mut restarted = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity reloads"),
        WorkspaceCatalog::open(&directory).expect("catalog reopens"),
        FakeDeliveryPort::default(),
    );
    let recovered = restarted
        .activate_active_workspace()
        .expect("initialization recovers")
        .expect("active workspace exists");
    assert_eq!(recovered.workspace.lifecycle, WorkspaceLifecycle::Ready);

    let catalog = WorkspaceCatalog::open(&directory).expect("catalog inspects");
    let store = catalog
        .open_workspace(&created.workspace.id)
        .expect("store opens");
    assert_eq!(
        store
            .membership_operation_ids()
            .expect("membership reads")
            .len(),
        1
    );
    assert_eq!(
        store.file_operations().expect("file operations read").len(),
        1
    );

    fs::remove_dir_all(directory).expect("directory removes");
}

#[test]
fn restart_resumes_after_genesis_and_creates_exactly_one_plans_operation() {
    let directory = temporary_directory("initialization-after-genesis");
    let custody = InMemoryKeyCustody::default();
    let mut workspace = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity creates"),
        WorkspaceCatalog::open(&directory).expect("catalog opens"),
        FakeDeliveryPort::default(),
    );
    let created = workspace
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    drop(workspace);

    let workspace_database = directory
        .join(".resonance/workspaces")
        .join(created.workspace.id.as_str())
        .join("workspace.sqlite3");
    let workspace_connection =
        Connection::open(&workspace_database).expect("workspace database opens");
    workspace_connection
        .execute_batch(
            "UPDATE workspace_configuration SET lifecycle = 'initializing' WHERE singleton = 1;
             DELETE FROM workspace_file_operations;",
        )
        .expect("failure after genesis injects");

    let mut restarted = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity reloads"),
        WorkspaceCatalog::open(&directory).expect("catalog reopens"),
        FakeDeliveryPort::default(),
    );
    restarted
        .activate_active_workspace()
        .expect("initialization resumes")
        .expect("active workspace exists");
    let catalog = WorkspaceCatalog::open(&directory).expect("catalog inspects");
    let store = catalog
        .open_workspace(&created.workspace.id)
        .expect("store opens");
    assert_eq!(
        store
            .membership_operation_ids()
            .expect("membership reads")
            .len(),
        1
    );
    assert_eq!(
        store.file_operations().expect("file operations read").len(),
        1
    );

    fs::remove_dir_all(directory).expect("directory removes");
}

#[test]
fn restart_recovers_an_unpublished_initialization_without_duplicate_records() {
    let directory = temporary_directory("unpublished-initialization");
    let custody = InMemoryKeyCustody::default();
    let mut workspace = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity creates"),
        WorkspaceCatalog::open(&directory).expect("catalog opens"),
        FakeDeliveryPort::default(),
    );
    let created = workspace
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    drop(workspace);

    let catalog_database = directory.join(".resonance/catalog.sqlite3");
    let workspace_database = directory
        .join(".resonance/workspaces")
        .join(created.workspace.id.as_str())
        .join("workspace.sqlite3");
    Connection::open(&catalog_database)
        .expect("catalog database opens")
        .execute_batch(&format!(
            "DELETE FROM workspace_catalog WHERE workspace_id = '{}';
             UPDATE catalog_state SET active_workspace_id = NULL WHERE singleton = 1;",
            created.workspace.id.as_str()
        ))
        .expect("publication interruption injects");
    Connection::open(workspace_database)
        .expect("workspace database opens")
        .execute(
            "UPDATE workspace_configuration SET lifecycle = 'initializing' WHERE singleton = 1",
            [],
        )
        .expect("initialization stage rewinds");

    let mut restarted = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity reloads"),
        WorkspaceCatalog::open(&directory).expect("catalog recovers orphan"),
        FakeDeliveryPort::default(),
    );
    let recovered = restarted
        .activate_active_workspace()
        .expect("orphaned initialization recovers")
        .expect("recovered workspace is active");
    assert_eq!(recovered.workspace.lifecycle, WorkspaceLifecycle::Ready);

    let catalog = WorkspaceCatalog::open(&directory).expect("catalog reopens");
    let store = catalog
        .open_workspace(&created.workspace.id)
        .expect("store reopens");
    assert_eq!(
        store
            .membership_operation_ids()
            .expect("membership reads")
            .len(),
        1
    );
    assert_eq!(store.file_operations().expect("file reads").len(), 1);
    fs::remove_dir_all(directory).expect("directory removes");
}

#[test]
fn initialization_without_durable_creator_input_stays_unavailable() {
    let directory = temporary_directory("initialization-missing-input");
    let custody = InMemoryKeyCustody::default();
    let mut workspace = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity creates"),
        WorkspaceCatalog::open(&directory).expect("catalog opens"),
        FakeDeliveryPort::default(),
    );
    let created = workspace
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    drop(workspace);

    let workspace_database = directory
        .join(".resonance/workspaces")
        .join(created.workspace.id.as_str())
        .join("workspace.sqlite3");
    Connection::open(workspace_database)
        .expect("workspace database opens")
        .execute_batch(
            "UPDATE workspace_configuration
                SET lifecycle = 'initializing', creation_creator_display_name = NULL
              WHERE singleton = 1;
             DELETE FROM workspace_file_operations;",
        )
        .expect("unrecoverable failure injects");

    let mut restarted = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("identity reloads"),
        WorkspaceCatalog::open(&directory).expect("catalog reopens"),
        FakeDeliveryPort::default(),
    );
    assert!(restarted.activate_active_workspace().is_err());
    assert!(restarted.create_invite("bootstrap").is_err());
    assert!(!restarted
        .send_heartbeat()
        .expect("heartbeat remains blocked"));

    fs::remove_dir_all(directory).expect("directory removes");
}

#[test]
fn creates_an_invite_and_completes_a_named_inviter_join() {
    let inviter_directory = temporary_directory("inviter-session");
    let joiner_directory = temporary_directory("joiner-session");
    let mut inviter = session(&inviter_directory);
    let creator_view = inviter
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let invite = inviter
        .create_invite("opaque-bootstrap-address")
        .expect("invite creates");
    let mut joiner = session(&joiner_directory);
    let joining_view = joiner.join_workspace(&invite, "Lin").expect("join starts");

    assert_eq!(
        joining_view.workspace.lifecycle,
        WorkspaceLifecycle::Joining
    );
    assert_eq!(joining_view.members.len(), 0);
    let join_request = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("join request sends");
    let decoded_join = Envelope::decode(&join_request).expect("join envelope decodes");
    let EnvelopeBody::JoinRequest { recipient_key, .. } = decoded_join.body else {
        panic!("join request body");
    };
    let recipient_key = ExactRecordV1::decode(&recipient_key).expect("recipient record validates");
    let ConversationRecordV1::RecipientKey(recipient_key) = recipient_key.record() else {
        panic!("recipient-key family");
    };
    assert_eq!(
        recipient_key.installation,
        *joining_view.local_public_identity.as_bytes()
    );
    inviter
        .receive(&join_request)
        .expect("named inviter accepts");
    let admission = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("admission sends");
    joiner
        .receive(&admission)
        .expect("joiner receives admission");
    let joined_view = joiner.view().expect("joined view");

    assert_eq!(joined_view.workspace.id, creator_view.workspace.id);
    assert_eq!(joined_view.workspace.lifecycle, WorkspaceLifecycle::Ready);
    assert_eq!(joined_view.members.len(), 2);
    assert!(joined_view
        .members
        .iter()
        .any(|member| member.role == "developer" && member.display_name == "Ada"));
    assert!(joined_view
        .members
        .iter()
        .any(|member| member.role == "contributor" && member.display_name == "Lin"));

    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn member_file_history_notices_and_reconnect_state_request_recovery() {
    let inviter_directory = temporary_directory("file-notice-inviter");
    let joiner_directory = temporary_directory("file-notice-joiner");
    let mut inviter = session(&inviter_directory);
    let workspace = inviter
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let invite = inviter
        .create_invite("opaque-bootstrap-address")
        .expect("invite creates");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    let request = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("join request sends");
    inviter.receive(&request).expect("join request applies");
    let admission = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("admission sends");
    joiner.receive(&admission).expect("admission applies");
    assert!(joiner
        .take_file_history_recovery_needed()
        .expect("admission requests initial recovery"));

    inviter
        .announce_file_history(vec!["a".repeat(32)])
        .expect("notice signs");
    let notice = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("notice sends");
    joiner.receive(&notice).expect("member notice applies");
    assert!(joiner
        .take_file_history_recovery_needed()
        .expect("notice requests recovery"));

    let outsider = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("outsider identity creates");
    let unauthorized = Envelope::sign(
        &outsider,
        workspace.workspace.id.as_str(),
        EnvelopeBody::FileHistoryNotice {
            operation_ids: vec!["b".repeat(32)],
        },
    )
    .expect("outsider notice signs")
    .encode()
    .expect("outsider notice encodes");
    assert!(joiner.receive(&unauthorized).is_err());
    assert!(!joiner
        .take_file_history_recovery_needed()
        .expect("unauthorized notice has no effect"));

    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn ignores_a_joiners_own_gossip_echo() {
    let inviter_directory = temporary_directory("echo-inviter");
    let joiner_directory = temporary_directory("echo-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    let echo = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("join request queues");

    joiner.receive(&echo).expect("own echo is ignored");
    assert_eq!(
        joiner
            .view()
            .expect("joining view remains available")
            .workspace
            .lifecycle,
        WorkspaceLifecycle::Joining
    );

    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn pending_joiner_ignores_an_inviter_heartbeat_before_membership_arrives() {
    let inviter_directory = temporary_directory("early-heartbeat-inviter");
    let joiner_directory = temporary_directory("early-heartbeat-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");
    assert!(inviter.send_heartbeat().expect("inviter heartbeat queues"));
    let heartbeat = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("heartbeat is outbound");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");

    joiner
        .receive(&heartbeat)
        .expect("pre-admission inviter heartbeat is ignored");
    assert_eq!(
        joiner
            .view()
            .expect("joining view remains available")
            .workspace
            .lifecycle,
        WorkspaceLifecycle::Joining
    );

    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn pending_joiner_does_not_queue_a_heartbeat() {
    let inviter_directory = temporary_directory("heartbeat-inviter");
    let joiner_directory = temporary_directory("heartbeat-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    let _ = joiner.delivery_mut().take_outbound();

    assert!(!joiner
        .send_heartbeat()
        .expect("pending join heartbeat is safely ignored"));
    assert!(joiner.delivery_mut().take_outbound().is_empty());

    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn restores_pending_join_admission_after_a_session_restart() {
    let inviter_directory = temporary_directory("resume-inviter");
    let joiner_directory = temporary_directory("resume-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");

    let custody = InMemoryKeyCustody::default();
    let mut joiner = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("joiner identity creates"),
        WorkspaceCatalog::open(&joiner_directory).expect("joiner catalog opens"),
        FakeDeliveryPort::default(),
    );
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    drop(joiner);

    let mut resumed = WorkspaceSession::new(
        InstallationIdentity::load_or_create(&custody).expect("joiner identity reloads"),
        WorkspaceCatalog::open(&joiner_directory).expect("joiner catalog reopens"),
        FakeDeliveryPort::default(),
    );
    resumed
        .activate_active_workspace()
        .expect("pending workspace restores");
    assert!(resumed
        .retry_join("Lin")
        .expect("restored pending join retries"));
    assert_eq!(resumed.delivery_mut().take_outbound().len(), 1);

    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn admitted_join_retry_resends_membership_without_readding_the_member() {
    let inviter_directory = temporary_directory("admitted-retry-inviter");
    let joiner_directory = temporary_directory("admitted-retry-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    let initial_request = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("initial request sends");
    inviter
        .receive(&initial_request)
        .expect("inviter admits joiner");
    let _lost_response = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("initial response sends");

    assert!(joiner.retry_join("Lin").expect("join retry sends"));
    let retry_request = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("retry request sends");
    inviter
        .receive(&retry_request)
        .expect("inviter resends membership");
    let retry_response = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("retry response sends");
    let EnvelopeBody::MembershipSyncResponse(operations) = Envelope::decode(&retry_response)
        .expect("response decodes")
        .body
    else {
        panic!("retry response contains membership sync");
    };
    assert_eq!(operations.len(), 2);

    joiner
        .receive(&retry_response)
        .expect("joiner applies retried membership");
    assert_eq!(joiner.view().expect("view").members.len(), 2);
    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn keeps_joining_retryable_and_recovers_the_full_operation_set() {
    let inviter_directory = temporary_directory("retry-inviter");
    let joiner_directory = temporary_directory("retry-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    assert!(joiner
        .retry_join("Lin")
        .expect("join retries while pending"));

    let requests = joiner.delivery_mut().take_outbound();
    assert_eq!(requests.len(), 2);
    inviter
        .receive(&requests[1])
        .expect("retry reaches inviter");
    let admission = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("admission sends");
    joiner.receive(&admission).expect("admission applies");
    joiner.request_membership_sync().expect("sync requests");
    let sync_request = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("sync sends");
    inviter
        .receive(&sync_request)
        .expect("inviter answers sync");
    let sync_response = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("sync response sends");
    joiner
        .receive(&sync_response)
        .expect("joiner restores membership log");

    assert_eq!(joiner.view().expect("view").members.len(), 2);
    assert!(!joiner
        .retry_join("Lin")
        .expect("accepted join retry becomes a no-op"));
    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn derives_known_member_presence_from_signed_heartbeats_not_unknown_senders() {
    let inviter_directory = temporary_directory("presence-inviter");
    let joiner_directory = temporary_directory("presence-joiner");
    let mut inviter = session(&inviter_directory);
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let invite = inviter.create_invite("bootstrap").expect("invite creates");
    let mut joiner = session(&joiner_directory);
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    let request = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("join request sends");
    inviter.receive(&request).expect("inviter admits joiner");
    let admission = inviter
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("admission sends");
    joiner.receive(&admission).expect("joiner is admitted");

    assert!(joiner.send_heartbeat().expect("heartbeat sends"));
    let heartbeat = joiner
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("heartbeat queues");
    inviter
        .receive(&heartbeat)
        .expect("member heartbeat applies");
    let peers = inviter.view().expect("view remains available").peers;
    assert_eq!(peers.len(), 1);
    assert!(peers[0].online);
    assert_eq!(peers[0].connection, PeerConnection::Unknown);

    assert!(inviter.expire_presence(i64::MAX).expect("presence expires"));
    assert!(inviter
        .take_transitions()
        .iter()
        .any(|transition| matches!(transition, WorkspaceTransition::PeerPresenceChanged(peer) if !peer.online)));
    assert!(!inviter
        .expire_presence(i64::MAX)
        .expect("already-offline peer does not emit again"));
    assert!(inviter.take_transitions().is_empty());
    fs::remove_dir_all(inviter_directory).expect("inviter directory removes");
    fs::remove_dir_all(joiner_directory).expect("joiner directory removes");
}

#[test]
fn rejects_malformed_invites_and_invalid_relay_or_bootstrap_input() {
    let directory = temporary_directory("invalid-invite");
    let mut workspace = session(&directory);

    assert!(workspace
        .create_workspace("Team Resonance", Some("not a URL".to_owned()))
        .is_err());
    workspace
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    assert!(workspace.create_invite("").is_err());
    assert!(Invite::decode("this is not base58").is_err());

    fs::remove_dir_all(directory).expect("directory removes");
}

#[test]
fn creator_processes_a_persisted_interval_bound_departure_atomically() {
    let creator_directory = temporary_directory("departure-creator");
    let requester_directory = temporary_directory("departure-requester");
    let creator_identity =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![81; 32]))
            .expect("creator identity");
    let requester_identity =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![82; 32]))
            .expect("requester identity");
    let creator_catalog = WorkspaceCatalog::open(&creator_directory).expect("creator catalog");
    let requester_catalog =
        WorkspaceCatalog::open(&requester_directory).expect("requester catalog");
    let mut creator = WorkspaceSession::new(
        creator_identity.clone(),
        creator_catalog,
        FakeDeliveryPort::default(),
    );
    let created = creator
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let invite = creator.create_invite("bootstrap").expect("invite");
    let mut requester = WorkspaceSession::new(
        requester_identity.clone(),
        requester_catalog,
        FakeDeliveryPort::default(),
    );
    requester
        .join_workspace(&invite, "Lin")
        .expect("join starts");
    let join = requester
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("join request");
    creator.receive(&join).expect("creator admits");
    let admission = creator
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("admission");
    requester.receive(&admission).expect("requester admitted");

    assert!(!requester
        .local_conversation_authoring_blocked()
        .expect("authoring starts enabled"));
    let request = requester.request_departure(3).expect("request persists");
    assert_eq!(
        requester
            .request_departure(99)
            .expect("duplicate action reuses exact request")
            .request_id()
            .expect("duplicate ID"),
        request.request_id().expect("request ID")
    );
    assert!(requester
        .local_conversation_authoring_blocked()
        .expect("pending departure blocks local authoring"));
    assert_eq!(requester.view().expect("pending view").members.len(), 2);
    let prepared = creator
        .prepare_requested_departure(request, 4)
        .expect("creator automatically prepares valid request");
    assert_eq!(creator.view().expect("old view").members.len(), 2);
    let store = WorkspaceCatalog::open(&creator_directory)
        .expect("creator catalog reopens")
        .open_workspace(&created.workspace.id)
        .expect("creator store");
    let mut authority = ConversationAuthority::open(
        creator_identity.clone(),
        created.workspace.id.as_str(),
        store.clone(),
    )
    .expect("authority opens");
    let committed = authority
        .commit_transition(&prepared)
        .expect("membership and epoch commit");
    let ConversationRecordV1::Epoch(epoch) = committed.epoch.record() else {
        panic!("epoch record");
    };
    assert!(epoch
        .recipients
        .iter()
        .all(|recipient| recipient.member != *requester_identity.public_identity().as_bytes()));
    assert_eq!(
        creator
            .view()
            .expect("still old until finalize")
            .members
            .len(),
        2
    );
    let mut rebuilt = WorkspaceSession::new(
        creator_identity.clone(),
        WorkspaceCatalog::open(&creator_directory).expect("recovery catalog"),
        FakeDeliveryPort::default(),
    );
    assert_eq!(
        rebuilt
            .activate_active_workspace()
            .expect("post-commit restart rebuilds")
            .expect("active workspace")
            .members
            .len(),
        1
    );
    creator
        .finalize_prepared_transition(&prepared)
        .expect("projection finalizes after commit");
    let final_view = creator.view().expect("final view");
    assert_eq!(final_view.members.len(), 1);
    assert_eq!(
        final_view.members[0].public_identity,
        final_view.local_public_identity
    );
    assert_eq!(store.lookup_peer_set_version().expect("version"), 3);

    fs::remove_dir_all(creator_directory).expect("creator directory removes");
    fs::remove_dir_all(requester_directory).expect("requester directory removes");
}

#[test]
fn durable_iroh_departure_control_survives_both_restarts_and_creates_one_epoch() {
    let creator_directory = temporary_directory("departure-control-creator");
    let requester_directory = temporary_directory("departure-control-requester");
    let creator_identity =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![91; 32]))
            .expect("creator identity");
    let requester_identity =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![92; 32]))
            .expect("requester identity");
    let mut creator = WorkspaceSession::new(
        creator_identity.clone(),
        WorkspaceCatalog::open(&creator_directory).expect("creator catalog"),
        FakeDeliveryPort::default(),
    );
    let created = creator
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let invite = creator.create_invite("bootstrap").expect("invite");
    let mut requester = WorkspaceSession::new(
        requester_identity.clone(),
        WorkspaceCatalog::open(&requester_directory).expect("requester catalog"),
        FakeDeliveryPort::default(),
    );
    requester
        .join_workspace(&invite, "Lin")
        .expect("join starts");
    let join = requester
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("join request");
    creator.receive(&join).expect("creator admits");
    let admission = creator
        .delivery_mut()
        .take_outbound()
        .pop()
        .expect("admission");
    requester.receive(&admission).expect("requester admitted");
    let request_id = requester
        .request_departure(3)
        .expect("request persists")
        .request_id()
        .expect("request ID");
    drop(requester);

    let mut requester = WorkspaceSession::new(
        requester_identity.clone(),
        WorkspaceCatalog::open(&requester_directory).expect("requester restart catalog"),
        FakeDeliveryPort::default(),
    );
    requester
        .activate_active_workspace()
        .expect("requester restarts");
    requester
        .queue_iroh_publication_duties()
        .expect("durable request queues");
    let request_envelope = requester
        .delivery_mut()
        .take_outbound()
        .into_iter()
        .find(|bytes| {
            Envelope::decode(bytes)
                .is_ok_and(|envelope| matches!(envelope.body, EnvelopeBody::DepartureRequest(_)))
        })
        .expect("departure control queues after restart");
    drop(creator);

    let mut creator = WorkspaceSession::new(
        creator_identity.clone(),
        WorkspaceCatalog::open(&creator_directory).expect("creator restart catalog"),
        FakeDeliveryPort::default(),
    );
    creator
        .activate_active_workspace()
        .expect("creator restarts before control");
    creator
        .receive(&request_envelope)
        .expect("creator automatically commits request and epoch");
    assert_eq!(creator.view().expect("creator view").members.len(), 1);
    creator
        .receive(&request_envelope)
        .expect_err("removed requester cannot replay control as a member");

    let creator_store = WorkspaceCatalog::open(&creator_directory)
        .expect("creator inspection catalog")
        .open_workspace(&created.workspace.id)
        .expect("creator store");
    let duties = creator_store
        .durable_publication_duties()
        .expect("duties remain durable");
    let removal_duties = duties
        .iter()
        .filter(|duty| duty.transport == "iroh-membership")
        .filter(|duty| {
            SignedMembershipOperation::decode(&duty.exact_bytes).is_ok_and(|operation| {
                matches!(
                    operation.operation.body,
                    MembershipOperationBody::RemoveMember { .. }
                )
            })
        })
        .count();
    let next_epoch_duties = duties
        .iter()
        .filter(|duty| duty.transport == "commonware-record")
        .filter(|duty| {
            ExactRecordV1::decode(&duty.exact_bytes)
                .is_ok_and(|record| matches!(record.record(), ConversationRecordV1::Epoch(_)))
        })
        .count();
    assert_eq!(removal_duties, 1);
    assert_eq!(
        next_epoch_duties, 3,
        "genesis, admission, and removal each have one epoch"
    );
    assert_eq!(
        requester
            .request_departure(99)
            .expect("pending request remains exact until removal arrives")
            .request_id()
            .expect("request ID remains"),
        request_id
    );

    fs::remove_dir_all(creator_directory).expect("creator directory removes");
    fs::remove_dir_all(requester_directory).expect("requester directory removes");
}

#[test]
fn rejects_a_join_request_not_addressed_to_the_canonical_inviter() {
    let directory = temporary_directory("wrong-inviter");
    let mut inviter = session(&directory);
    let view = inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let outsider = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("outsider identity creates");
    let envelope = Envelope::sign(
        &outsider,
        view.workspace.id.as_str(),
        EnvelopeBody::JoinRequest {
            inviter: [7; 32],
            display_name: "Lin".to_owned(),
            recipient_key: Vec::new(),
        },
    )
    .expect("envelope signs")
    .encode()
    .expect("envelope encodes");

    assert!(inviter.receive(&envelope).is_err());
    assert_eq!(
        inviter
            .view()
            .expect("view remains available")
            .members
            .len(),
        1
    );
    fs::remove_dir_all(directory).expect("directory removes");
}
