#![forbid(unsafe_code)]

use kodework_domain::{CredentialProvider, CredentialRef};

#[cfg(windows)]
pub type Store = kodework_secrets_win::CredentialManagerStore;

#[cfg(not(windows))]
pub type Store = kodework_secrets_native::KeyringStore;

#[must_use]
pub fn new_store() -> Store {
    #[cfg(windows)]
    {
        kodework_secrets_win::CredentialManagerStore::new("kodework")
    }
    #[cfg(not(windows))]
    {
        kodework_secrets_native::KeyringStore::new("dev.kodework.credentials")
    }
}

#[must_use]
pub fn provider() -> CredentialProvider {
    #[cfg(windows)]
    {
        CredentialProvider::WindowsCredentialManager
    }
    #[cfg(not(windows))]
    {
        CredentialProvider::NativeKeyring
    }
}

#[must_use]
pub fn is_current_provider(reference: &CredentialRef) -> bool {
    reference.provider == provider()
}

/// Deletes only references owned by this platform adapter.  A database may
/// legitimately contain a reference created on another operating system; it
/// must not be interpreted as a local secret or blindly deleted.
#[must_use]
pub fn is_managed_reference(reference: &CredentialRef) -> bool {
    is_current_provider(reference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_provider_is_explicit() {
        let current = CredentialRef {
            provider: provider(),
            opaque_id: "test/current".into(),
        };
        assert!(is_managed_reference(&current));

        let foreign = CredentialRef {
            provider: if cfg!(windows) {
                CredentialProvider::NativeKeyring
            } else {
                CredentialProvider::WindowsCredentialManager
            },
            opaque_id: "test/foreign".into(),
        };
        assert!(!is_managed_reference(&foreign));
    }
}
