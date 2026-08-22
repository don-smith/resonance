use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    invite::Invite,
    protocol::{Envelope, EnvelopeBody},
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{PeerConnection, WorkspaceLifecycle},
    workspace_session::{FakeDeliveryPort, WorkspaceSession, WorkspaceTransition},
};

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
fn creates_an_invite_and_completes_a_named_inviter_join() {
    let inviter_directory = temporary_directory("inviter-session");
    let joiner_directory = temporary_directory("joiner-session");
    let mut inviter = session(&inviter_directory);
    let creator_view = inviter
        .create_workspace("Team Resonance", None)
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
        .any(|member| member.role == "contributor" && member.display_name == "Lin"));

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
