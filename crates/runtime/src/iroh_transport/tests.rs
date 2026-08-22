use std::time::Duration;

use super::{IrohTransport, TransportEvent};
use crate::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    invite::Invite,
    workspace_catalog::WorkspaceCatalog,
    workspace_session::{FakeDeliveryPort, WorkspaceSession},
};

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("test installation identity creates")
}

async fn next_event(transport: &mut IrohTransport) -> TransportEvent {
    tokio::time::timeout(Duration::from_secs(10), transport.next_event())
        .await
        .expect("gossip event arrives")
        .expect("gossip event succeeds")
        .expect("gossip remains open")
}

async fn wait_for_neighbor(transport: &mut IrohTransport) {
    loop {
        if matches!(
            next_event(transport).await,
            TransportEvent::NeighborUp { .. }
        ) {
            return;
        }
    }
}

fn session(directory: &std::path::Path) -> WorkspaceSession<FakeDeliveryPort> {
    WorkspaceSession::new(
        identity(),
        WorkspaceCatalog::open(directory).expect("workspace catalog opens"),
        FakeDeliveryPort::default(),
    )
}

async fn receive_message(transport: &mut IrohTransport, expected: &[u8]) {
    loop {
        if let TransportEvent::Received(message) = next_event(transport).await {
            assert_eq!(message, expected);
            return;
        }
    }
}

#[tokio::test]
#[ignore = "requires the public default relay"]
async fn carries_workspace_scoped_bytes_between_independent_identities_over_the_default_relay() {
    let token = [24; 32];
    let mut inviter = IrohTransport::start(&identity(), token, None, None)
        .await
        .expect("inviter transport starts");
    inviter.endpoint.online().await;
    let bootstrap = inviter
        .bootstrap_hint()
        .await
        .expect("bootstrap hint encodes");
    let mut joiner = IrohTransport::start(&identity(), token, None, Some(&bootstrap))
        .await
        .expect("joiner transport starts");
    joiner
        .broadcast(vec![4, 5, 6])
        .await
        .expect("initial join message broadcasts before neighbor arrival");

    wait_for_neighbor(&mut joiner).await;
    inviter
        .broadcast(vec![1, 2, 3])
        .await
        .expect("membership envelope broadcasts");
    receive_message(&mut joiner, &[1, 2, 3]).await;

    inviter.shutdown().await.expect("inviter stops cleanly");
    joiner.shutdown().await.expect("joiner stops cleanly");
}

#[tokio::test]
async fn carries_workspace_scoped_bytes_between_independent_identities_over_a_local_relay() {
    let (relay_map, _relay_url, _server) = iroh::test_utils::run_relay_server()
        .await
        .expect("local relay starts");
    let token = [42; 32];
    let mut inviter =
        IrohTransport::start_with_local_relay(&identity(), token, relay_map.clone(), None)
            .await
            .expect("inviter transport starts");
    inviter.endpoint.online().await;
    let bootstrap = inviter
        .bootstrap_hint()
        .await
        .expect("bootstrap hint encodes");
    let mut joiner =
        IrohTransport::start_with_local_relay(&identity(), token, relay_map, Some(&bootstrap))
            .await
            .expect("joiner transport starts");

    wait_for_neighbor(&mut joiner).await;
    inviter
        .broadcast(vec![1, 2, 3])
        .await
        .expect("membership envelope broadcasts");
    receive_message(&mut joiner, &[1, 2, 3]).await;

    inviter.shutdown().await.expect("inviter stops cleanly");
    joiner.shutdown().await.expect("joiner stops cleanly");
    assert!(inviter.is_closed());
    assert!(joiner.is_closed());
}

#[tokio::test]
async fn routes_invite_join_and_membership_recovery_through_the_local_relay() {
    let (relay_map, _relay_url, _server) = iroh::test_utils::run_relay_server()
        .await
        .expect("local relay starts");
    let inviter_directory = tempfile::tempdir().expect("inviter directory creates");
    let joiner_directory = tempfile::tempdir().expect("joiner directory creates");
    let mut inviter = session(inviter_directory.path());
    inviter
        .create_workspace("Team Resonance", None)
        .expect("workspace creates");
    let mut inviter_transport =
        IrohTransport::start_with_local_relay_for_session(&inviter, relay_map.clone(), None)
            .await
            .expect("inviter transport starts");
    inviter_transport.endpoint.online().await;
    let invite = inviter
        .create_invite(
            inviter_transport
                .bootstrap_hint()
                .await
                .expect("bootstrap encodes"),
        )
        .expect("invite creates");
    let decoded = Invite::decode(&invite).expect("invite decodes");
    let mut joiner = session(joiner_directory.path());
    joiner.join_workspace(&invite, "Lin").expect("join starts");
    let mut joiner_transport = IrohTransport::start_with_local_relay_for_session(
        &joiner,
        relay_map,
        Some(decoded.bootstrap()),
    )
    .await
    .expect("joiner transport starts");

    for _ in 0..100 {
        let _ = tokio::time::timeout(
            Duration::from_millis(100),
            inviter_transport.apply_next_session_event(&mut inviter),
        )
        .await;
        inviter_transport
            .flush_session(&mut inviter)
            .await
            .expect("inviter flushes");
        let _ = tokio::time::timeout(
            Duration::from_millis(100),
            joiner_transport.apply_next_session_event(&mut joiner),
        )
        .await;
        joiner_transport
            .flush_session(&mut joiner)
            .await
            .expect("joiner flushes");
        if joiner.view().expect("view remains available").members.len() == 2 {
            break;
        }
    }

    assert_eq!(joiner.view().expect("joiner view").members.len(), 2);
    joiner_transport
        .send_session_heartbeat(&mut joiner)
        .await
        .expect("joiner heartbeat sends");
    for _ in 0..20 {
        let _ = tokio::time::timeout(
            Duration::from_millis(100),
            inviter_transport.apply_next_session_event(&mut inviter),
        )
        .await;
        if inviter
            .view()
            .expect("inviter view")
            .peers
            .iter()
            .any(|peer| peer.online)
        {
            break;
        }
    }
    assert!(inviter
        .view()
        .expect("inviter view")
        .peers
        .iter()
        .any(|peer| peer.online));
    inviter_transport.shutdown().await.expect("inviter stops");
    joiner_transport.shutdown().await.expect("joiner stops");
}
