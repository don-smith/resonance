//! Stable Rust boundary for Resonance runtime services.
//!
//! Feature modules are added here instead of leaking persistence, package, or
//! delivery details into the desktop shell.

pub mod packages;
pub mod release;
pub mod workspace_store;

/// Returns the runtime label used by the desktop bootstrap.
#[must_use]
pub const fn runtime_name() -> &'static str {
    "resonance-runtime"
}

#[cfg(test)]
mod tests {
    use super::runtime_name;

    #[test]
    fn identifies_the_runtime_boundary() {
        assert_eq!(runtime_name(), "resonance-runtime");
    }
}
