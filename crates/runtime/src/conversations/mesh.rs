//! Managed Commonware lookup transport behind bounded host queues.

use std::{
    collections::BTreeMap,
    fmt,
    net::{SocketAddr, TcpListener},
    num::NonZeroUsize,
    panic::{self, AssertUnwindSafe},
    sync::mpsc::{self, Receiver as EventReceiver, SyncSender},
    thread::{self, JoinHandle},
};

use commonware_codec::DecodeExt as _;
use commonware_cryptography::ed25519;
use commonware_p2p::{
    authenticated::lookup, Address, AddressableManager as _, Receiver as _, Recipients, Sender as _,
};
use commonware_runtime::{
    tokio as commonware_tokio, IoBuf, Quota, Runner as _, Spawner as _, Supervisor as _,
};
use commonware_utils::ordered::Map;
use tokio::sync::mpsc::{self as tokio_mpsc, error::TrySendError};

use crate::{identity::InstallationIdentity, identity::PublicIdentity};

use super::{
    address_directory::DirectPeerRoute, framing, wire::MAX_FRAME_BYTES, ConversationError,
};

const APPLICATION_CHANNEL: u64 = 0;
const COMMONWARE_NAMESPACE: &[u8] = b"resonance.conversation.lookup.v1";
const DEFAULT_QUEUE_CAPACITY: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationMeshEvent {
    Started {
        listen_addr: SocketAddr,
    },
    Received {
        authenticated_peer: PublicIdentity,
        exact_bytes: Vec<u8>,
    },
    AuthenticatedPeerObserved(PublicIdentity),
    SendDeferred(Vec<u8>),
    Fatal(String),
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationMeshError {
    InvalidListenAddress,
    Startup(String),
    Backpressure,
    UnauthorizedPeer,
    Stopped,
    ThreadPanicked,
    Conversation(ConversationError),
}

impl fmt::Display for ConversationMeshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidListenAddress => formatter.write_str("conversation listener is invalid"),
            Self::Startup(message) => {
                write!(formatter, "conversation mesh startup failed: {message}")
            }
            Self::Backpressure => formatter.write_str("conversation mesh command queue is full"),
            Self::UnauthorizedPeer => {
                formatter.write_str("conversation mesh recipient is not in the active peer set")
            }
            Self::Stopped => formatter.write_str("conversation mesh has stopped"),
            Self::ThreadPanicked => formatter.write_str("conversation mesh thread panicked"),
            Self::Conversation(error) => write!(formatter, "conversation framing failed: {error}"),
        }
    }
}

impl std::error::Error for ConversationMeshError {}

impl From<ConversationError> for ConversationMeshError {
    fn from(error: ConversationError) -> Self {
        Self::Conversation(error)
    }
}

#[derive(Clone, Debug)]
enum MeshCommand {
    Track {
        version: u64,
        routes: Vec<DirectPeerRoute>,
    },
    Overwrite(Vec<DirectPeerRoute>),
    Send {
        recipient: Option<PublicIdentity>,
        frame: Vec<u8>,
    },
    Stop,
    Panic,
}

pub struct ProductionConversationMesh {
    commands: Option<tokio_mpsc::Sender<MeshCommand>>,
    events: EventReceiver<ConversationMeshEvent>,
    thread: Option<JoinHandle<()>>,
    listen_addr: SocketAddr,
}

impl ProductionConversationMesh {
    pub fn start(
        identity: InstallationIdentity,
        workspace_id: [u8; 32],
        requested_listen: SocketAddr,
    ) -> Result<Self, ConversationMeshError> {
        Self::start_with_capacity(
            identity,
            workspace_id,
            requested_listen,
            DEFAULT_QUEUE_CAPACITY,
        )
    }

    pub fn start_with_capacity(
        identity: InstallationIdentity,
        workspace_id: [u8; 32],
        requested_listen: SocketAddr,
        queue_capacity: usize,
    ) -> Result<Self, ConversationMeshError> {
        let listen_addr = reserve_listen_address(requested_listen)?;
        let signer = identity
            .commonware_signer()
            .map_err(|error| ConversationMeshError::Startup(error.to_string()))?;
        let (commands, command_receiver) = tokio_mpsc::channel(queue_capacity.max(1));
        let (event_sender, events) = mpsc::sync_channel(queue_capacity.max(1));
        let thread = thread::Builder::new()
            .name(format!(
                "resonance-conversations-{}",
                &hex(&workspace_id)[..8]
            ))
            .spawn(move || {
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    run_commonware(
                        signer,
                        workspace_id,
                        listen_addr,
                        command_receiver,
                        event_sender.clone(),
                    );
                }));
                if result.is_err() {
                    let _ = event_sender.try_send(ConversationMeshEvent::Fatal(
                        "Commonware runtime panicked".to_owned(),
                    ));
                }
                let _ = event_sender.try_send(ConversationMeshEvent::Stopped);
            })
            .map_err(|error| ConversationMeshError::Startup(error.to_string()))?;
        Ok(Self {
            commands: Some(commands),
            events,
            thread: Some(thread),
            listen_addr,
        })
    }

    #[must_use]
    pub const fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub(crate) fn track_members(
        &self,
        version: u64,
        routes: Vec<DirectPeerRoute>,
    ) -> Result<(), ConversationMeshError> {
        self.try_command(MeshCommand::Track { version, routes })
    }

    pub(crate) fn overwrite_addresses(
        &self,
        routes: Vec<DirectPeerRoute>,
    ) -> Result<(), ConversationMeshError> {
        self.try_command(MeshCommand::Overwrite(routes))
    }

    pub fn send(
        &self,
        recipient: Option<PublicIdentity>,
        exact_bytes: &[u8],
    ) -> Result<(), ConversationMeshError> {
        self.try_command(MeshCommand::Send {
            recipient,
            frame: framing::encode(exact_bytes)?,
        })
    }

    pub fn try_event(&self) -> Option<ConversationMeshEvent> {
        self.events.try_recv().ok()
    }

    pub fn recv_event_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Option<ConversationMeshEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn stop(&mut self) -> Result<(), ConversationMeshError> {
        if let Some(commands) = self.commands.take() {
            match commands.try_send(MeshCommand::Stop) {
                Ok(()) => {}
                Err(TrySendError::Full(command)) => commands
                    .blocking_send(command)
                    .map_err(|_| ConversationMeshError::Stopped)?,
                Err(TrySendError::Closed(_)) => {}
            }
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| ConversationMeshError::ThreadPanicked)?;
        }
        Ok(())
    }

    fn try_command(&self, command: MeshCommand) -> Result<(), ConversationMeshError> {
        match self
            .commands
            .as_ref()
            .ok_or(ConversationMeshError::Stopped)?
            .try_send(command)
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(ConversationMeshError::Backpressure),
            Err(TrySendError::Closed(_)) => Err(ConversationMeshError::Stopped),
        }
    }

    pub(crate) fn force_thread_panic(&self) -> Result<(), ConversationMeshError> {
        self.try_command(MeshCommand::Panic)
    }
}

impl Drop for ProductionConversationMesh {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn reserve_listen_address(requested: SocketAddr) -> Result<SocketAddr, ConversationMeshError> {
    let listener = TcpListener::bind(requested)
        .map_err(|error| ConversationMeshError::Startup(error.to_string()))?;
    let address = listener
        .local_addr()
        .map_err(|error| ConversationMeshError::Startup(error.to_string()))?;
    drop(listener);
    Ok(address)
}

fn run_commonware(
    signer: ed25519::PrivateKey,
    workspace_id: [u8; 32],
    listen_addr: SocketAddr,
    mut commands: tokio_mpsc::Receiver<MeshCommand>,
    events: SyncSender<ConversationMeshEvent>,
) {
    let runner = commonware_tokio::Runner::default();
    runner.start(|context| async move {
        let mut namespace = COMMONWARE_NAMESPACE.to_vec();
        namespace.extend_from_slice(&workspace_id);
        let mut config = lookup::Config::recommended(
            signer,
            &namespace,
            listen_addr,
            MAX_FRAME_BYTES as u32,
        );
        config.allow_private_ips = true;
        config.allow_dns = false;
        config.peer_connection_cooldown = std::time::Duration::from_millis(250);
        config.dial_frequency = std::time::Duration::from_millis(100);
        config.tracked_peer_sets = NonZeroUsize::new(1).expect("one is non-zero");
        let (mut network, mut oracle) = lookup::Network::new(context.child("lookup"), config);
        let (mut sender, mut receiver) = network.register(
            APPLICATION_CHANNEL,
            Quota::per_second(std::num::NonZeroU32::new(1_000).expect("non-zero quota")),
            DEFAULT_QUEUE_CAPACITY,
        );
        network.start();
        let _ = events.try_send(ConversationMeshEvent::Started { listen_addr });
        loop {
            tokio::select! {
                command = commands.recv() => {
                    match command {
                        Some(MeshCommand::Track { version, routes }) => {
                            if let Ok(peers) = route_map(routes) {
                                oracle.track(version, peers);
                            }
                        }
                        Some(MeshCommand::Overwrite(routes)) => {
                            if let Ok(peers) = route_map(routes) {
                                oracle.overwrite(peers);
                            }
                        }
                        Some(MeshCommand::Send { recipient, frame }) => {
                            let recipients = match recipient {
                                Some(identity) => match commonware_public(identity) {
                                    Ok(identity) => Recipients::One(identity),
                                    Err(()) => {
                                        let _ = events.try_send(ConversationMeshEvent::SendDeferred(frame));
                                        continue;
                                    }
                                },
                                None => Recipients::All,
                            };
                            if sender.send(recipients, IoBuf::from(frame.clone()), true).is_empty() {
                                let _ = events.try_send(ConversationMeshEvent::SendDeferred(frame));
                            }
                        }
                        Some(MeshCommand::Stop) | None => break,
                        Some(MeshCommand::Panic) => panic!("injected Commonware thread panic"),
                    }
                }
                received = receiver.recv() => {
                    match received {
                        Ok((peer, payload)) => {
                            let peer_bytes: Result<[u8; 32], _> = peer.as_ref().try_into();
                            let Ok(peer_bytes) = peer_bytes else { continue; };
                            let identity = PublicIdentity::from_bytes(peer_bytes);
                            match framing::decode(payload.as_ref()) {
                                Ok(exact) => {
                                    let _ = events.try_send(ConversationMeshEvent::AuthenticatedPeerObserved(identity));
                                    let _ = events.try_send(ConversationMeshEvent::Received {
                                        authenticated_peer: identity,
                                        exact_bytes: exact.to_vec(),
                                    });
                                }
                                Err(error) => {
                                    let _ = events.try_send(ConversationMeshEvent::Fatal(error.to_string()));
                                }
                            }
                        }
                        Err(error) => {
                            let _ = events.try_send(ConversationMeshEvent::Fatal(error.to_string()));
                            break;
                        }
                    }
                }
            }
        }
        let _ = context.stop(0, None).await;
    });
}

fn route_map(routes: Vec<DirectPeerRoute>) -> Result<Map<ed25519::PublicKey, Address>, ()> {
    let mut peers = Vec::new();
    for route in routes {
        let Some(candidate) = route.candidates.first() else {
            continue;
        };
        peers.push((
            commonware_public(route.identity)?,
            Address::Symmetric(*candidate),
        ));
    }
    peers.try_into().map_err(|_| ())
}

fn commonware_public(identity: PublicIdentity) -> Result<ed25519::PublicKey, ()> {
    ed25519::PublicKey::decode(identity.as_bytes().as_slice()).map_err(|_| ())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[derive(Default)]
pub struct InMemoryConversationMesh {
    capacity: usize,
    peers: BTreeMap<PublicIdentity, Vec<SocketAddr>>,
    peer_set_version: Option<u64>,
    sent: Vec<(Option<PublicIdentity>, Vec<u8>)>,
    stopped: bool,
}

impl InMemoryConversationMesh {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            ..Self::default()
        }
    }

    pub fn track_members(
        &mut self,
        version: u64,
        routes: Vec<(PublicIdentity, Vec<SocketAddr>)>,
    ) -> Result<(), ConversationMeshError> {
        if self.stopped {
            return Err(ConversationMeshError::Stopped);
        }
        if self
            .peer_set_version
            .is_some_and(|current| version <= current)
        {
            return Err(ConversationMeshError::Startup(
                "peer-set version is not monotonic".to_owned(),
            ));
        }
        self.peer_set_version = Some(version);
        self.peers = routes.into_iter().collect();
        Ok(())
    }

    pub fn overwrite_addresses(&mut self, routes: Vec<(PublicIdentity, Vec<SocketAddr>)>) {
        for (identity, addresses) in routes {
            if let Some(current) = self.peers.get_mut(&identity) {
                *current = addresses;
            }
        }
    }

    pub fn send(
        &mut self,
        recipient: Option<PublicIdentity>,
        exact: &[u8],
    ) -> Result<(), ConversationMeshError> {
        if self.stopped {
            return Err(ConversationMeshError::Stopped);
        }
        if recipient.is_some_and(|identity| !self.peers.contains_key(&identity)) {
            return Err(ConversationMeshError::UnauthorizedPeer);
        }
        if self.sent.len() >= self.capacity {
            return Err(ConversationMeshError::Backpressure);
        }
        framing::encode(exact)?;
        self.sent.push((recipient, exact.to_vec()));
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stopped = true;
    }

    #[must_use]
    pub fn peer_set_version(&self) -> Option<u64> {
        self.peer_set_version
    }

    #[must_use]
    pub fn peers(&self) -> &BTreeMap<PublicIdentity, Vec<SocketAddr>> {
        &self.peers
    }

    pub fn take_sent(&mut self) -> Vec<(Option<PublicIdentity>, Vec<u8>)> {
        std::mem::take(&mut self.sent)
    }
}
