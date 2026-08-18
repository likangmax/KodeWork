#![forbid(unsafe_code)]

//! Local filesystem SFTP backend: mirrors the SftpBackend contract against
//! a local directory, enabling large-file streaming tests without a
//! network or an in-memory store.

use kodework_sftp::backend::{RemoteFileMeta, SftpBackend, SftpReader, SftpWriter};
use kodework_sftp::SftpError;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// A backend rooted at a local directory. Paths are joined verbatim.
pub struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, remote: &str) -> PathBuf {
        self.root.join(remote.trim_start_matches('/'))
    }
}

struct LocalReader {
    file: tokio::fs::File,
}

#[async_trait::async_trait]
impl SftpReader for LocalReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, SftpError> {
        self.file
            .read(buf)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn seek(&mut self, offset: u64) -> Result<(), SftpError> {
        use tokio::io::AsyncSeekExt;
        self.file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map(|_| ())
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn close(&mut self) -> Result<(), SftpError> {
        Ok(())
    }
}

struct LocalWriter {
    file: tokio::fs::File,
    pos: u64,
}

#[async_trait::async_trait]
impl SftpWriter for LocalWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<(), SftpError> {
        self.file
            .write_all(buf)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        self.pos += buf.len() as u64;
        Ok(())
    }

    async fn seek(&mut self, offset: u64) -> Result<(), SftpError> {
        self.file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        self.pos = offset;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), SftpError> {
        self.file
            .flush()
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn close(&mut self) -> Result<(), SftpError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl SftpBackend for LocalFsBackend {
    async fn stat(&self, path: &str) -> Result<Option<RemoteFileMeta>, SftpError> {
        let local = self.resolve(path);
        match tokio::fs::metadata(&local).await {
            Ok(meta) => Ok(Some(RemoteFileMeta {
                name: local
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size: meta.len(),
                is_dir: meta.is_dir(),
                modified_ms: None,
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SftpError::Backend(error.to_string())),
        }
    }

    async fn list(&self, path: &str) -> Result<Vec<RemoteFileMeta>, SftpError> {
        let local = self.resolve(path);
        let mut entries = tokio::fs::read_dir(&local)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        let mut out = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?
        {
            let meta = entry
                .metadata()
                .await
                .map_err(|error| SftpError::Backend(error.to_string()))?;
            out.push(RemoteFileMeta {
                name: entry.file_name().to_string_lossy().into_owned(),
                size: meta.len(),
                is_dir: meta.is_dir(),
                modified_ms: None,
            });
        }
        Ok(out)
    }

    async fn remove(&self, path: &str) -> Result<(), SftpError> {
        tokio::fs::remove_file(self.resolve(path))
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        tokio::fs::rename(self.resolve(from), self.resolve(to))
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn SftpReader>, SftpError> {
        let file = tokio::fs::File::open(self.resolve(path))
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        Ok(Box::new(LocalReader { file }))
    }

    async fn open_write(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn SftpWriter>, SftpError> {
        let local = self.resolve(path);
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| SftpError::Backend(error.to_string()))?;
        }
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(truncate)
            .open(&local)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        Ok(Box::new(LocalWriter { file, pos: 0 }))
    }
}
