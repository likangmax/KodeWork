#![forbid(unsafe_code)]

//! Cross-platform OS keyring adapter.
//!
//! The app stores only opaque credential references in SQLite.  On macOS the
//! `keyring` backend maps to Keychain; on Linux it maps to the login Secret
//! Service; on Windows this crate remains available for portability tests but
//! the desktop shell deliberately keeps using the existing Credential Manager
//! adapter so existing Windows installations do not lose their records.

use keyring::Entry;
use kodework_domain::{CredentialProvider, CredentialRef};
use kodework_secrets::{SecretError, SecretStore, SecretValue};

const DEFAULT_SERVICE: &str = "dev.kodework.credentials";
const MAX_REFERENCE_BYTES: usize = 512;

#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE)
    }
}

impl KeyringStore {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, reference: &CredentialRef) -> Result<Entry, SecretError> {
        if reference.provider != CredentialProvider::NativeKeyring
            || reference.opaque_id.is_empty()
            || reference.opaque_id.len() > MAX_REFERENCE_BYTES
            || reference.opaque_id.chars().any(char::is_control)
        {
            return Err(SecretError::NotFound);
        }
        Entry::new(&self.service, &reference.opaque_id).map_err(|_| SecretError::NotFound)
    }
}

impl SecretStore for KeyringStore {
    fn put(&mut self, reference: CredentialRef, value: &[u8]) -> Result<(), SecretError> {
        if value.is_empty() {
            return Err(SecretError::Empty);
        }
        self.entry(&reference)?
            .set_secret(value)
            .map_err(|_| SecretError::NotFound)
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretValue, SecretError> {
        let value = self
            .entry(reference)?
            .get_secret()
            .map_err(|_| SecretError::NotFound)?;
        if value.is_empty() {
            return Err(SecretError::NotFound);
        }
        Ok(SecretValue::new(value))
    }

    fn delete(&mut self, reference: &CredentialRef) -> Result<(), SecretError> {
        self.entry(reference)?
            .delete_credential()
            .map_err(|_| SecretError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{CredentialProvider, CredentialRef};

    fn reference(value: &str) -> CredentialRef {
        CredentialRef {
            provider: CredentialProvider::NativeKeyring,
            opaque_id: value.into(),
        }
    }

    #[test]
    fn rejects_invalid_reference_without_touching_the_backend() {
        let store = KeyringStore::default();
        assert!(store.entry(&reference("")).is_err());
        assert!(store.entry(&reference("bad\nreference")).is_err());
        assert!(store
            .entry(&reference(&"x".repeat(MAX_REFERENCE_BYTES + 1)))
            .is_err());
        let foreign = CredentialRef {
            provider: CredentialProvider::WindowsCredentialManager,
            opaque_id: "foreign".into(),
        };
        assert!(store.entry(&foreign).is_err());
    }

    #[test]
    fn rejects_empty_secret() {
        let mut store = KeyringStore::default();
        assert_eq!(
            store.put(reference("test-empty"), b""),
            Err(SecretError::Empty)
        );
    }
}
