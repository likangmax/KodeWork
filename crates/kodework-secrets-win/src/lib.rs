#![deny(unsafe_code)]
//! Windows platform secret stores (Credential Manager + DPAPI files).
//!
//! This crate is the only workspace member that calls Win32 APIs. The
//! unsafe surface is confined to `windows_impl` and reviewed there; the
//! public API is plain `SecretStore` implementations.

#[cfg(windows)]
pub mod windows_impl;

#[cfg(windows)]
pub use windows_impl::{CredentialManagerStore, DpapiFileStore};

/// The Windows adapter remains a workspace member so portable CI can verify
/// the full graph.  It is intentionally unusable off Windows rather than
/// silently pretending to persist secrets in an insecure fallback store.
#[cfg(not(windows))]
mod unsupported {
    use kodework_domain::CredentialRef;
    use kodework_secrets::{SecretError, SecretStore, SecretValue};

    #[derive(Debug, Default)]
    pub struct CredentialManagerStore;

    impl CredentialManagerStore {
        #[must_use]
        pub fn new(_: impl Into<String>) -> Self {
            Self
        }
    }

    impl SecretStore for CredentialManagerStore {
        fn put(&mut self, _: CredentialRef, _: &[u8]) -> Result<(), SecretError> {
            Err(SecretError::NotFound)
        }

        fn get(&self, _: &CredentialRef) -> Result<SecretValue, SecretError> {
            Err(SecretError::NotFound)
        }

        fn delete(&mut self, _: &CredentialRef) -> Result<(), SecretError> {
            Err(SecretError::NotFound)
        }
    }

    #[derive(Debug, Default)]
    pub struct DpapiFileStore;

    impl DpapiFileStore {
        #[must_use]
        pub fn new(_: impl Into<std::path::PathBuf>) -> Self {
            Self
        }
    }
}

#[cfg(not(windows))]
pub use unsupported::{CredentialManagerStore, DpapiFileStore};
