//! Iroh/Gossip delivery adapter for one active workspace.
//!
//! This is the only runtime module that imports Iroh or Gossip types. Callers
//! exchange signed protocol bytes and secret-free peer observations.

use std::fmt;

use iroh::{
    address_lookup::memory::MemoryLookup, endpoint::presets, protocol::Router, Endpoint,
    EndpointAddr, RelayMode, RelayUrl,
};
use iroh_gossip::{api::Event, Gossip, TopicId, ALPN};
use n0_future::StreamExt;

use crate::{
    identity::InstallationIdentity,
    workspace_domain::PeerConnection,
    workspace_session::{FakeDeliveryPort, WorkspaceSession, WorkspaceSessionError},
};

const TOPIC_DOMAIN: &[u8] = b"resonance.workspace-topic.v1\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelaySelection {
    Default,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerPath {
    Direct,
    Relayed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    Received(Vec<u8>),
    NeighborUp {
        public_identity: [u8; 32],
        path: PeerPath,
    },
    NeighborDown {
        public_identity: [u8; 32],
    },
    Lagged,
}

#[derive(Debug)]
pub enum IrohTransportError {
    Bind,
    Bootstrap,
    Subscribe,
    Broadcast,
    Receive,
    Shutdown,
}

impl fmt::Display for IrohTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind => formatter.write_str("Iroh endpoint could not start"),
            Self::Bootstrap => formatter.write_str("workspace bootstrap address is invalid"),
            Self::Subscribe => formatter.write_str("workspace gossip topic could not start"),
            Self::Broadcast => formatter.write_str("workspace gossip message could not send"),
            Self::Receive => formatter.write_str("workspace gossip subscription failed"),
            Self::Shutdown => formatter.write_str("Iroh endpoint could not stop cleanly"),
        }
    }
}

impl std::error::Error for IrohTransportError {}

#[derive(Debug)]
pub enum IrohSessionAdapterError {
    Transport(IrohTransportError),
    Session(WorkspaceSessionError),
}

impl fmt::Display for IrohSessionAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "workspace transport failed: {error}"),
            Self::Session(error) => write!(formatter, "workspace session failed: {error}"),
        }
    }
}

impl std::error::Error for IrohSessionAdapterError {}

impl From<IrohTransportError> for IrohSessionAdapterError {
    fn from(error: IrohTransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<WorkspaceSessionError> for IrohSessionAdapterError {
    fn from(error: WorkspaceSessionError) -> Self {
        Self::Session(error)
    }
}

/// Owns the endpoint, Router, Gossip handler, and one workspace topic.
pub struct IrohTransport {
    endpoint: Endpoint,
    router: Router,
    gossip: Gossip,
    sender: Option<iroh_gossip::api::GossipSender>,
    receiver: Option<iroh_gossip::api::GossipReceiver>,
    relay_selection: RelaySelection,
}

impl IrohTransport {
    /// Starts a supplied-identity endpoint and subscribes to the workspace topic.
    pub async fn start(
        identity: &InstallationIdentity,
        workspace_token: [u8; 32],
        relay_override: Option<&str>,
        bootstrap: Option<&str>,
    ) -> Result<Self, IrohTransportError> {
        let lookup = MemoryLookup::new();
        let bootstrap_peers = bootstrap
            .map(|value| register_bootstrap(&lookup, value))
            .transpose()?
            .into_iter()
            .collect();
        let relay_selection = relay_selection(relay_override)?;
        Self::start_with(
            identity,
            workspace_token,
            lookup,
            relay_mode(&relay_selection)?,
            relay_selection,
            bootstrap_peers,
        )
        .await
    }

    pub async fn start_for_session(
        session: &WorkspaceSession<FakeDeliveryPort>,
    ) -> Result<Self, IrohSessionAdapterError> {
        let (workspace_token, relay_override) = session.transport_settings()?;
        Ok(Self::start(
            session.transport_identity(),
            workspace_token,
            relay_override.as_deref(),
            session.transport_bootstrap()?,
        )
        .await?)
    }

    async fn start_with(
        identity: &InstallationIdentity,
        workspace_token: [u8; 32],
        lookup: MemoryLookup,
        relay_mode: RelayMode,
        relay_selection: RelaySelection,
        bootstrap_peers: Vec<iroh::EndpointId>,
    ) -> Result<Self, IrohTransportError> {
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(identity.transport_secret_key())
            .address_lookup(lookup)
            .relay_mode(relay_mode)
            .bind()
            .await
            .map_err(|_| IrohTransportError::Bind)?;
        Self::start_from_endpoint(endpoint, workspace_token, relay_selection, bootstrap_peers).await
    }

    async fn start_from_endpoint(
        endpoint: Endpoint,
        workspace_token: [u8; 32],
        relay_selection: RelaySelection,
        bootstrap_peers: Vec<iroh::EndpointId>,
    ) -> Result<Self, IrohTransportError> {
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let router = Router::builder(endpoint.clone())
            .accept(ALPN, gossip.clone())
            .spawn();
        let (sender, receiver) = gossip
            .subscribe(topic_id(workspace_token), bootstrap_peers)
            .await
            .map_err(|_| IrohTransportError::Subscribe)?
            .split();
        Ok(Self {
            endpoint,
            router,
            gossip,
            sender: Some(sender),
            receiver: Some(receiver),
            relay_selection,
        })
    }

    #[cfg(test)]
    async fn start_with_local_relay(
        identity: &InstallationIdentity,
        workspace_token: [u8; 32],
        relay_map: iroh::RelayMap,
        bootstrap: Option<&str>,
    ) -> Result<Self, IrohTransportError> {
        let lookup = MemoryLookup::new();
        let bootstrap_peers = bootstrap
            .map(|value| register_bootstrap(&lookup, value))
            .transpose()?
            .into_iter()
            .collect();
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(identity.transport_secret_key())
            .address_lookup(lookup)
            .relay_mode(RelayMode::Custom(relay_map))
            .ca_tls_config(iroh_relay::tls::CaTlsConfig::insecure_skip_verify())
            .bind()
            .await
            .map_err(|_| IrohTransportError::Bind)?;
        Self::start_from_endpoint(
            endpoint,
            workspace_token,
            RelaySelection::Custom("local relay".to_owned()),
            bootstrap_peers,
        )
        .await
    }

    #[cfg(test)]
    async fn start_with_local_relay_for_session(
        session: &WorkspaceSession<FakeDeliveryPort>,
        relay_map: iroh::RelayMap,
        bootstrap: Option<&str>,
    ) -> Result<Self, IrohSessionAdapterError> {
        let (workspace_token, _) = session.transport_settings()?;
        Ok(Self::start_with_local_relay(
            session.transport_identity(),
            workspace_token,
            relay_map,
            bootstrap,
        )
        .await?)
    }

    /// Returns a base58, postcard-encoded Iroh node address for an invite.
    pub async fn bootstrap_hint(&self) -> Result<String, IrohTransportError> {
        self.endpoint.online().await;
        let address = postcard::to_stdvec(&self.endpoint.addr())
            .map_err(|_| IrohTransportError::Bootstrap)?;
        Ok(bs58::encode(address).into_string())
    }

    #[must_use]
    pub fn relay_selection(&self) -> &RelaySelection {
        &self.relay_selection
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.endpoint.is_closed()
    }

    pub async fn broadcast(&self, bytes: Vec<u8>) -> Result<(), IrohTransportError> {
        self.sender
            .as_ref()
            .ok_or(IrohTransportError::Broadcast)?
            .broadcast(bytes.into())
            .await
            .map_err(|_| IrohTransportError::Broadcast)
    }

    /// Waits for the next workspace-scoped message or neighbor observation.
    pub async fn next_event(&mut self) -> Result<Option<TransportEvent>, IrohTransportError> {
        let Some(receiver) = self.receiver.as_mut() else {
            return Ok(None);
        };
        let Some(event) = receiver.next().await else {
            return Ok(None);
        };
        let event = event.map_err(|_| IrohTransportError::Receive)?;
        Ok(Some(match event {
            Event::Received(message) => TransportEvent::Received(message.content.to_vec()),
            Event::NeighborUp(endpoint_id) => TransportEvent::NeighborUp {
                public_identity: *endpoint_id.as_bytes(),
                path: self.peer_path(endpoint_id).await,
            },
            Event::NeighborDown(endpoint_id) => TransportEvent::NeighborDown {
                public_identity: *endpoint_id.as_bytes(),
            },
            Event::Lagged => TransportEvent::Lagged,
        }))
    }

    /// Broadcasts all queued signed session messages through Gossip.
    pub async fn flush_session(
        &self,
        session: &mut WorkspaceSession<FakeDeliveryPort>,
    ) -> Result<(), IrohSessionAdapterError> {
        for message in session.delivery_mut().take_outbound() {
            self.broadcast(message).await?;
        }
        Ok(())
    }

    /// Signs and broadcasts one heartbeat through the active workspace session.
    pub async fn send_session_heartbeat(
        &self,
        session: &mut WorkspaceSession<FakeDeliveryPort>,
    ) -> Result<(), IrohSessionAdapterError> {
        session.send_heartbeat()?;
        self.flush_session(session).await
    }

    /// Applies one network observation without allowing Gossip to author membership.
    pub async fn apply_next_session_event(
        &mut self,
        session: &mut WorkspaceSession<FakeDeliveryPort>,
    ) -> Result<bool, IrohSessionAdapterError> {
        let Some(event) = self.next_event().await? else {
            return Ok(false);
        };
        match event {
            TransportEvent::Received(bytes) => session.receive(&bytes)?,
            TransportEvent::NeighborUp {
                public_identity,
                path,
            } => {
                session.observe_connection(public_identity, peer_connection(path))?;
                if session.is_ready()? {
                    session.request_membership_sync()?;
                }
            }
            TransportEvent::NeighborDown { public_identity } => {
                session.observe_connection(public_identity, PeerConnection::Unknown)?;
            }
            TransportEvent::Lagged if session.is_ready()? => session.request_membership_sync()?,
            TransportEvent::Lagged => {}
        }
        Ok(true)
    }

    /// Leaves the topic before stopping Gossip and awaiting Router shutdown.
    pub async fn shutdown(&mut self) -> Result<(), IrohTransportError> {
        self.sender.take();
        self.receiver.take();
        self.gossip
            .shutdown()
            .await
            .map_err(|_| IrohTransportError::Shutdown)?;
        self.router
            .shutdown()
            .await
            .map_err(|_| IrohTransportError::Shutdown)
    }

    async fn peer_path(&self, endpoint_id: iroh::EndpointId) -> PeerPath {
        let Some(info) = self.endpoint.remote_info(endpoint_id).await else {
            return PeerPath::Unknown;
        };
        if info.addrs().any(|address| address.addr().is_ip()) {
            PeerPath::Direct
        } else if info.addrs().any(|address| address.addr().is_relay()) {
            PeerPath::Relayed
        } else {
            PeerPath::Unknown
        }
    }
}

fn peer_connection(path: PeerPath) -> PeerConnection {
    match path {
        PeerPath::Direct => PeerConnection::Direct,
        PeerPath::Relayed => PeerConnection::Relayed,
        PeerPath::Unknown => PeerConnection::Unknown,
    }
}

fn topic_id(workspace_token: [u8; 32]) -> TopicId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TOPIC_DOMAIN);
    hasher.update(&workspace_token);
    TopicId::from_bytes(*hasher.finalize().as_bytes())
}

fn relay_selection(relay_override: Option<&str>) -> Result<RelaySelection, IrohTransportError> {
    match relay_override {
        None => Ok(RelaySelection::Default),
        Some(value) => {
            let _: RelayUrl = value.parse().map_err(|_| IrohTransportError::Bind)?;
            Ok(RelaySelection::Custom(value.to_owned()))
        }
    }
}

fn relay_mode(selection: &RelaySelection) -> Result<RelayMode, IrohTransportError> {
    match selection {
        RelaySelection::Default => Ok(RelayMode::Default),
        RelaySelection::Custom(value) => value
            .parse::<RelayUrl>()
            .map(|relay| RelayMode::custom([relay]))
            .map_err(|_| IrohTransportError::Bind),
    }
}

#[cfg(test)]
mod tests;

fn register_bootstrap(
    lookup: &MemoryLookup,
    encoded: &str,
) -> Result<iroh::EndpointId, IrohTransportError> {
    let bytes = bs58::decode(encoded)
        .into_vec()
        .map_err(|_| IrohTransportError::Bootstrap)?;
    let address: EndpointAddr =
        postcard::from_bytes(&bytes).map_err(|_| IrohTransportError::Bootstrap)?;
    let endpoint_id = address.id;
    lookup.add_endpoint_info(address);
    Ok(endpoint_id)
}
