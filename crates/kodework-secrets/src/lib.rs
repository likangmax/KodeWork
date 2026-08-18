#![forbid(unsafe_code)]

use kodework_domain::CredentialRef;
use std::collections::BTreeMap;
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecretError {
    #[error("secret not found")]
    NotFound,
    #[error("secret value must not be empty")]
    Empty,
}

pub trait SecretStore {
    fn put(&mut self, reference: CredentialRef, value: &[u8]) -> Result<(), SecretError>;
    fn get(&self, reference: &CredentialRef) -> Result<SecretValue, SecretError>;
    fn delete(&mut self, reference: &CredentialRef) -> Result<(), SecretError>;
}

#[derive(Clone)]
pub struct SecretValue(Zeroizing<Vec<u8>>);

impl SecretValue {
    /// Wraps bytes; the value is zeroized on drop.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    #[must_use]
    pub fn expose(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretValue(REDACTED)")
    }
}

#[derive(Default)]
pub struct MemorySecretStore {
    values: BTreeMap<String, Zeroizing<Vec<u8>>>,
}

impl SecretStore for MemorySecretStore {
    fn put(&mut self, reference: CredentialRef, value: &[u8]) -> Result<(), SecretError> {
        if value.is_empty() {
            return Err(SecretError::Empty);
        }
        self.values
            .insert(reference.opaque_id, Zeroizing::new(value.to_vec()));
        Ok(())
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretValue, SecretError> {
        self.values
            .get(&reference.opaque_id)
            .cloned()
            .map(SecretValue)
            .ok_or(SecretError::NotFound)
    }

    fn delete(&mut self, reference: &CredentialRef) -> Result<(), SecretError> {
        self.values
            .remove(&reference.opaque_id)
            .map(|_| ())
            .ok_or(SecretError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{CredentialProvider, CredentialRef};

    fn reference() -> CredentialRef {
        CredentialRef {
            provider: CredentialProvider::Test,
            opaque_id: "test-secret".into(),
        }
    }

    #[test]
    fn memory_store_round_trip_redacts_debug() {
        let mut store = MemorySecretStore::default();
        assert!(store.put(reference(), b"secret-value").is_ok());
        let value = store.get(&reference());
        assert!(value.is_ok());
        let value = value.unwrap_or_else(|_| unreachable!("checked above"));
        assert_eq!(value.expose(), b"secret-value");
        assert!(!format!("{value:?}").contains("secret-value"));
    }

    #[test]
    fn empty_secret_is_rejected() {
        assert_eq!(
            MemorySecretStore::default().put(reference(), b""),
            Err(SecretError::Empty)
        );
    }
}
