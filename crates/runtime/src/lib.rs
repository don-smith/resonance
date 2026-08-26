//! Stable Rust boundary for Resonance runtime services.
//!
//! Feature modules are added here instead of leaking persistence, package, or
//! delivery details into the desktop shell.

#[cfg(all(feature = "debug-local-profiles", not(debug_assertions)))]
compile_error!("debug-local-profiles is limited to debug Rust builds");

pub mod identity;
pub mod invite;
pub mod iroh_transport;
pub mod local_root_binding;
pub mod membership_log;
pub mod packages;
pub mod protocol;
pub mod release;
pub mod workspace_application;
pub mod workspace_catalog;
pub mod workspace_domain;
pub mod workspace_file_runtime;
pub mod workspace_file_transport;
pub mod workspace_files;
pub mod workspace_session;
pub mod workspace_store;

/// Preferred application-facing surface for desktop integrations.
pub mod desktop {
    pub use crate::identity::{IdentityError, InstallationIdentity};
    pub use crate::workspace_application::{
        WorkspaceApplication, WorkspaceApplicationView, WorkspaceHealth,
    };
    pub use crate::workspace_domain::{
        KnownPeer, Member, PeerConnection, WorkspaceLifecycle, WorkspaceSummary,
    };
    pub use crate::workspace_session::WorkspaceTransition;
}
