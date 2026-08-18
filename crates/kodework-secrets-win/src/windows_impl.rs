#![allow(unsafe_code)]
//! Windows platform secret stores.
//!
//! - Credential Manager for passwords/passphrases/tokens.
//! - DPAPI-protected per-user files for private-key material.
//!
//! This module is the only place in the workspace that calls Win32 APIs
//! directly; everything else keeps unsafe_code = forbid.

use kodework_domain::CredentialRef;
use kodework_secrets::{SecretError, SecretStore, SecretValue};
use std::ffi::{c_void, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Foundation::TRUE;
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// Windows Credential Manager backed store.
///
/// Values are stored under a target name derived from the opaque id, so
/// secrets never appear in the app database.
#[derive(Debug, Default)]
pub struct CredentialManagerStore {
    /// Prefix for credential target names.
    target_prefix: String,
}

impl CredentialManagerStore {
    #[must_use]
    pub fn new(target_prefix: impl Into<String>) -> Self {
        Self {
            target_prefix: target_prefix.into(),
        }
    }

    fn target_name(&self, reference: &CredentialRef) -> OsString {
        let raw = format!("{}/{}", self.target_prefix, reference.opaque_id);
        OsString::from(raw)
    }

    fn valid_reference(reference: &CredentialRef) -> bool {
        !reference.opaque_id.trim().is_empty() && !reference.opaque_id.chars().any(char::is_control)
    }
}

impl SecretStore for CredentialManagerStore {
    fn put(&mut self, reference: CredentialRef, value: &[u8]) -> Result<(), SecretError> {
        if value.is_empty() || !Self::valid_reference(&reference) {
            return Err(SecretError::Empty);
        }
        let target = self.target_name(&reference);
        let target_wide: Vec<u16> = target.encode_wide().chain(std::iter::once(0)).collect();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target_wide.as_ptr() as *mut u16,
            Comment: std::ptr::null_mut(),
            LastWritten: unsafe { std::mem::zeroed() },
            CredentialBlobSize: u32::try_from(value.len()).map_err(|_| SecretError::Empty)?,
            CredentialBlob: value.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: std::ptr::null_mut(),
            UserName: std::ptr::null_mut(),
        };
        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok != TRUE {
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            eprintln!("CredWriteW failed with code {code}");
            return Err(SecretError::NotFound);
        }
        Ok(())
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretValue, SecretError> {
        if !Self::valid_reference(reference) {
            return Err(SecretError::NotFound);
        }
        let target = self.target_name(reference);
        let target_wide: Vec<u16> = target.encode_wide().chain(std::iter::once(0)).collect();
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        let ok = unsafe { CredReadW(target_wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
        if ok != TRUE || credential.is_null() {
            return Err(SecretError::NotFound);
        }
        let read = unsafe { &*credential };
        let size = usize::try_from(read.CredentialBlobSize).unwrap_or(0);
        let bytes = if read.CredentialBlob.is_null() || size == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(read.CredentialBlob, size) }.to_vec()
        };
        unsafe { CredFree(credential as *mut c_void) };
        if bytes.is_empty() {
            return Err(SecretError::NotFound);
        }
        Ok(SecretValue::new(bytes))
    }

    fn delete(&mut self, reference: &CredentialRef) -> Result<(), SecretError> {
        if !Self::valid_reference(reference) {
            return Err(SecretError::NotFound);
        }
        let target = self.target_name(reference);
        let target_wide: Vec<u16> = target.encode_wide().chain(std::iter::once(0)).collect();
        let ok = unsafe { CredDeleteW(target_wide.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok != TRUE {
            return Err(SecretError::NotFound);
        }
        Ok(())
    }
}

/// DPAPI-protected per-user file store for private-key material.
#[derive(Debug, Clone)]
pub struct DpapiFileStore {
    directory: PathBuf,
}

impl DpapiFileStore {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn path_for(&self, reference: &CredentialRef) -> PathBuf {
        let digest = stable_digest(&reference.opaque_id);
        self.directory.join(format!("{digest}.blob"))
    }
}

impl SecretStore for DpapiFileStore {
    fn put(&mut self, reference: CredentialRef, value: &[u8]) -> Result<(), SecretError> {
        if value.is_empty() {
            return Err(SecretError::Empty);
        }
        std::fs::create_dir_all(&self.directory).map_err(|_| SecretError::NotFound)?;
        let encrypted =
            zeroize::Zeroizing::new(dpapi_protect(value).map_err(|_| SecretError::NotFound)?);
        let path = self.path_for(&reference);
        // Atomic write: never leave a truncated blob behind on crash.
        static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
        let temp = path.with_file_name(format!(
            ".{}.tmp.{}.{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("secret"),
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&temp, &encrypted).map_err(|_| SecretError::NotFound)?;
        if let Err(error) = atomic_replace(&temp, &path) {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        Ok(())
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretValue, SecretError> {
        let path = self.path_for(reference);
        let encrypted =
            zeroize::Zeroizing::new(std::fs::read(&path).map_err(|_| SecretError::NotFound)?);
        let plain = dpapi_unprotect(&encrypted).map_err(|_| SecretError::NotFound)?;
        if plain.is_empty() {
            return Err(SecretError::NotFound);
        }
        Ok(SecretValue::new(plain))
    }

    fn delete(&mut self, reference: &CredentialRef) -> Result<(), SecretError> {
        let path = self.path_for(reference);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|_| SecretError::NotFound)?;
            Ok(())
        } else {
            Err(SecretError::NotFound)
        }
    }
}

fn atomic_replace(temp: &std::path::Path, target: &std::path::Path) -> Result<(), SecretError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = temp
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let to: Vec<u16> = target
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let ok = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            return Err(SecretError::NotFound);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(temp, target).map_err(|_| SecretError::NotFound)
    }
}

fn stable_digest(input: &str) -> String {
    // FNV-1a 64-bit; stable across runs and platforms (filename only).
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, ()> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plain.len()).map_err(|_| ())?,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok != TRUE || output.pbData.is_null() {
        return Err(());
    }
    let size = usize::try_from(output.cbData).unwrap_or(0);
    let mut bytes = Vec::with_capacity(size);
    unsafe {
        bytes.extend_from_slice(std::slice::from_raw_parts(output.pbData, size));
        LocalFree(output.pbData as *mut c_void);
    }
    Ok(bytes)
}

fn dpapi_unprotect(encrypted: &[u8]) -> Result<Vec<u8>, ()> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(encrypted.len()).map_err(|_| ())?,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok != TRUE || output.pbData.is_null() {
        return Err(());
    }
    let size = usize::try_from(output.cbData).unwrap_or(0);
    let mut bytes = Vec::with_capacity(size);
    unsafe {
        bytes.extend_from_slice(std::slice::from_raw_parts(output.pbData, size));
        LocalFree(output.pbData as *mut c_void);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::CredentialProvider;

    fn reference(name: &str) -> CredentialRef {
        CredentialRef {
            provider: CredentialProvider::Test,
            opaque_id: format!("kodework-secrets-test-{name}-{}", std::process::id()),
        }
    }

    #[test]
    fn dpapi_file_round_trip() {
        let directory =
            std::env::temp_dir().join(format!("kodework-dpapi-test-{}", std::process::id()));
        let mut store = DpapiFileStore::new(&directory);
        let reference = reference("dpapi");
        assert!(store
            .put(reference.clone(), b"private-key-material")
            .is_ok());
        let value = store
            .get(&reference)
            .unwrap_or_else(|error| unreachable!("get must succeed: {error}"));
        assert_eq!(value.expose(), b"private-key-material");
        assert!(store.delete(&reference).is_ok());
        assert!(matches!(store.get(&reference), Err(SecretError::NotFound)));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn credential_manager_round_trip() {
        let mut store = CredentialManagerStore::new("kodework-test");
        let reference = reference("cm");
        let put = store.put(reference.clone(), b"token-value");
        if put.is_err() {
            // Credential Manager may be unavailable in CI; skip gracefully.
            eprintln!("Credential Manager unavailable, skipping assertion");
            return;
        }
        let value = store
            .get(&reference)
            .unwrap_or_else(|error| unreachable!("get must succeed: {error}"));
        eprintln!("read back {} bytes", value.expose().len());
        assert_eq!(
            value.expose(),
            b"token-value",
            "credential content mismatch for {}",
            reference.opaque_id
        );
        assert!(store.delete(&reference).is_ok());
        assert!(
            matches!(store.get(&reference), Err(SecretError::NotFound)),
            "deleted credential still readable for {}",
            reference.opaque_id
        );
    }
}
