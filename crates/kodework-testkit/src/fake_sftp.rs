#![forbid(unsafe_code)]

//! In-memory SFTP backend with fault injection for offline transfer tests.
//!
//! Files live in a `HashMap<path, Vec<u8>>`. `.part` staging is modelled by
//! writing into the same map under the `.part` key until `rename` moves the
//! entry to the final name, mirroring the atomic-rename contract.

use kodework_sftp::backend::{RemoteFileMeta, SftpBackend, SftpReader, SftpWriter};
use kodework_sftp::SftpError;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Fault injection knobs shared with the backend.
#[derive(Debug, Clone, Default)]
pub struct FakeSftpFaults {
    /// Total bytes that may be written remotely before `DiskFull` fires.
    pub remote_write_quota: Option<u64>,
    /// Fail the next N `write` calls on new writers with a backend error.
    pub fail_next_writes: u64,
    /// Fail the next N `rename` calls.
    pub fail_next_renames: u64,
    /// Make `stat` return `None` for everything (source missing).
    pub missing_sources: bool,
    /// Artificial per-write delay in milliseconds (slow-transfer tests).
    pub write_delay_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct FakeSftpBackend {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    faults: Arc<FakeSftpFaults>,
    written_bytes: Arc<AtomicU64>,
    failed_writes: Arc<AtomicU64>,
    failed_renames: Arc<AtomicU64>,
}

impl FakeSftpBackend {
    #[must_use]
    pub fn new(faults: FakeSftpFaults) -> Self {
        let fail_next_writes = faults.fail_next_writes;
        let fail_next_renames = faults.fail_next_renames;
        Self {
            files: Arc::new(Mutex::new(HashMap::new())),
            faults: Arc::new(faults),
            written_bytes: Arc::new(AtomicU64::new(0)),
            failed_writes: Arc::new(AtomicU64::new(fail_next_writes)),
            failed_renames: Arc::new(AtomicU64::new(fail_next_renames)),
        }
    }

    /// Seeds a remote file for download tests.
    pub fn seed(&self, path: &str, contents: Vec<u8>) {
        if let Ok(mut guard) = self.files.lock() {
            guard.insert(path.to_string(), contents);
        }
    }

    /// Reads back a stored file (final name or `.part`).
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let guard = self.files.lock().ok()?;
        guard.get(path).cloned()
    }

    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.files
            .lock()
            .map(|g| g.contains_key(path))
            .unwrap_or(false)
    }

    fn meta_for(path: &str, contents: &[u8]) -> RemoteFileMeta {
        RemoteFileMeta {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            size: contents.len() as u64,
            is_dir: false,
            modified_ms: None,
        }
    }
}

struct FakeReader {
    data: Vec<u8>,
    pos: usize,
}

#[async_trait::async_trait]
impl SftpReader for FakeReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, SftpError> {
        if self.pos >= self.data.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }

    async fn seek(&mut self, offset: u64) -> Result<(), SftpError> {
        self.pos = usize::try_from(offset)
            .map_err(|_| SftpError::Backend("seek offset too large".into()))?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), SftpError> {
        Ok(())
    }
}

struct FakeWriter {
    backend: Arc<FakeSftpBackend>,
    target: String,
    pos: u64,
}

#[async_trait::async_trait]
impl SftpWriter for FakeWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<(), SftpError> {
        if self.backend.faults.write_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.backend.faults.write_delay_ms)).await;
        }
        if self.backend.failed_writes.load(Ordering::SeqCst) > 0 {
            self.backend.failed_writes.fetch_sub(1, Ordering::SeqCst);
            return Err(SftpError::Backend("injected write failure".into()));
        }
        if let Some(quota) = self.backend.faults.remote_write_quota {
            let written = self
                .backend
                .written_bytes
                .fetch_add(buf.len() as u64, Ordering::SeqCst)
                + buf.len() as u64;
            if written > quota {
                self.backend
                    .written_bytes
                    .fetch_sub(buf.len() as u64, Ordering::SeqCst);
                return Err(SftpError::DiskFull);
            }
        }
        // Real SFTP writes land on the remote filesystem immediately; a
        // cancelled transfer therefore leaves a usable .part.
        let mut guard = self
            .backend
            .files
            .lock()
            .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
        let entry = guard.entry(self.target.clone()).or_default();
        let start = usize::try_from(self.pos).unwrap_or(usize::MAX);
        if start > entry.len() {
            entry.resize(start, 0);
        }
        let end = start + buf.len();
        if end > entry.len() {
            entry.resize(end, 0);
        }
        entry[start..end].copy_from_slice(buf);
        drop(guard);
        self.pos += buf.len() as u64;
        Ok(())
    }

    async fn seek(&mut self, offset: u64) -> Result<(), SftpError> {
        self.pos = offset;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), SftpError> {
        Ok(())
    }

    async fn close(&mut self) -> Result<(), SftpError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl SftpBackend for FakeSftpBackend {
    async fn stat(&self, path: &str) -> Result<Option<RemoteFileMeta>, SftpError> {
        if self.faults.missing_sources {
            return Ok(None);
        }
        let guard = self
            .files
            .lock()
            .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
        Ok(guard
            .get(path)
            .map(|contents| Self::meta_for(path, contents)))
    }

    async fn list(&self, path: &str) -> Result<Vec<RemoteFileMeta>, SftpError> {
        let guard = self
            .files
            .lock()
            .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        Ok(guard
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(key, contents)| Self::meta_for(key, contents))
            .collect())
    }

    async fn remove(&self, path: &str) -> Result<(), SftpError> {
        let mut guard = self
            .files
            .lock()
            .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
        guard.remove(path);
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        if self.failed_renames.load(Ordering::SeqCst) > 0 {
            self.failed_renames.fetch_sub(1, Ordering::SeqCst);
            return Err(SftpError::Backend("injected rename failure".into()));
        }
        let mut guard = self
            .files
            .lock()
            .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
        let contents = guard
            .remove(from)
            .ok_or_else(|| SftpError::Backend(format!("rename source missing: {from}")))?;
        guard.insert(to.to_string(), contents);
        Ok(())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn SftpReader>, SftpError> {
        let guard = self
            .files
            .lock()
            .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
        let data = guard.get(path).cloned().ok_or(SftpError::SourceNotFound)?;
        Ok(Box::new(FakeReader { data, pos: 0 }))
    }

    async fn open_write(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn SftpWriter>, SftpError> {
        if truncate {
            let mut guard = self
                .files
                .lock()
                .map_err(|_| SftpError::Backend("fake store lock poisoned".into()))?;
            guard.remove(path);
        }
        Ok(Box::new(FakeWriter {
            backend: Arc::new(self.clone()),
            target: path.to_string(),
            pos: 0,
        }))
    }
}
