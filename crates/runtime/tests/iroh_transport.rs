use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    iroh_transport::{IrohTransport, RelaySelection},
};

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("test installation identity creates")
}

#[tokio::test]
async fn selects_the_default_production_relay_when_the_workspace_has_no_override() {
    let transport = IrohTransport::start(&identity(), [9; 32], None, None)
        .await
        .expect("default transport starts");
    assert_eq!(transport.relay_selection(), &RelaySelection::Default);
    let mut transport = transport;
    transport.shutdown().await.expect("transport stops cleanly");
    assert!(transport.is_closed());
}
