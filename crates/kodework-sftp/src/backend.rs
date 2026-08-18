#![forbid(unsafe_code)]

//! SFTP backend abstraction. The real implementation wraps russh-sftp;
//! fakes implement the same trait for offline transfer tests.

use crate::SftpError;

/// Remote file metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteFileMeta {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified_ms: Option<u64>,
}

/// Streaming read handle (remote side).
#[async_trait::async_trait]
pub trait SftpReader: Send {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, SftpError>;
    /// Moves the read position, used to resume downloads without replaying
    /// bytes already present in the local `.part` file.
    async fn seek(&mut self, offset: u64) -> Result<(), SftpError>;
    async fn close(&mut self) -> Result<(), SftpError>;
}

/// Streaming write handle (remote side).
#[async_trait::async_trait]
pub trait SftpWriter: Send {
    async fn write(&mut self, buf: &[u8]) -> Result<(), SftpError>;
    /// Moves the write position (required for resume from a .part file).
    async fn seek(&mut self, offset: u64) -> Result<(), SftpError>;
    async fn flush(&mut self) -> Result<(), SftpError>;
    async fn close(&mut self) -> Result<(), SftpError>;
}

/// Remote file system operations used by the transfer manager.
#[async_trait::async_trait]
pub trait SftpBackend: Send + Sync {
    async fn stat(&self, path: &str) -> Result<Option<RemoteFileMeta>, SftpError>;
    async fn list(&self, path: &str) -> Result<Vec<RemoteFileMeta>, SftpError>;
    async fn remove(&self, path: &str) -> Result<(), SftpError>;
    async fn rename(&self, from: &str, to: &str) -> Result<(), SftpError>;
    /// Opens a remote file for reading.
    async fn open_read(&self, path: &str) -> Result<Box<dyn SftpReader>, SftpError>;
    /// Opens (or creates) a remote file for writing. `truncate` controls
    /// whether an existing file is emptied (resume keeps the offset).
    async fn open_write(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn SftpWriter>, SftpError>;
}

/// russh-sftp backed implementation.
pub struct RusshSftpBackend {
    session: std::sync::Arc<russh_sftp::client::SftpSession>,
}

impl RusshSftpBackend {
    #[must_use]
    pub fn new(session: std::sync::Arc<russh_sftp::client::SftpSession>) -> Self {
        Self { session }
    }
}

struct RusshReader {
    file: Option<russh_sftp::client::fs::File>,
}

#[async_trait::async_trait]
impl SftpReader for RusshReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, SftpError> {
        use tokio::io::AsyncReadExt;
        let file = self
            .file
            .as_mut()
            .ok_or(SftpError::Backend("reader already closed".into()))?;
        file.read(buf)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn seek(&mut self, offset: u64) -> Result<(), SftpError> {
        use tokio::io::AsyncSeekExt;
        let file = self
            .file
            .as_mut()
            .ok_or(SftpError::Backend("reader already closed".into()))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map(|_| ())
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn close(&mut self) -> Result<(), SftpError> {
        if let Some(file) = self.file.take() {
            file.close()
                .await
                .map_err(|error| SftpError::Backend(error.to_string()))?;
        }
        Ok(())
    }
}

struct RusshWriter {
    file: Option<russh_sftp::client::fs::File>,
    offset: u64,
}

#[async_trait::async_trait]
impl SftpWriter for RusshWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<(), SftpError> {
        use tokio::io::AsyncWriteExt;
        let file = self
            .file
            .as_mut()
            .ok_or(SftpError::Backend("writer already closed".into()))?;
        file.write_all(buf)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        self.offset = self.offset.saturating_add(buf.len() as u64);
        Ok(())
    }

    async fn seek(&mut self, offset: u64) -> Result<(), SftpError> {
        use tokio::io::AsyncSeekExt;
        let file = self
            .file
            .as_mut()
            .ok_or(SftpError::Backend("writer already closed".into()))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        self.offset = offset;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), SftpError> {
        use tokio::io::AsyncWriteExt;
        let file = self
            .file
            .as_mut()
            .ok_or(SftpError::Backend("writer already closed".into()))?;
        file.flush()
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn close(&mut self) -> Result<(), SftpError> {
        if let Some(file) = self.file.take() {
            file.close()
                .await
                .map_err(|error| SftpError::Backend(error.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl SftpBackend for RusshSftpBackend {
    async fn stat(&self, path: &str) -> Result<Option<RemoteFileMeta>, SftpError> {
        let meta = match self.session.metadata(path).await {
            Ok(meta) => meta,
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                return Ok(None);
            }
            Err(error) => return Err(SftpError::Backend(error.to_string())),
        };
        Ok(Some(RemoteFileMeta {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            size: meta.len(),
            is_dir: meta.is_dir(),
            modified_ms: None,
        }))
    }

    async fn list(&self, path: &str) -> Result<Vec<RemoteFileMeta>, SftpError> {
        let entries = self
            .session
            .read_dir(path)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        let mut out = Vec::new();
        for entry in entries {
            let meta = entry.metadata();
            out.push(RemoteFileMeta {
                name: entry.file_name(),
                size: meta.len(),
                is_dir: meta.file_type().is_dir(),
                modified_ms: None,
            });
        }
        Ok(out)
    }

    async fn remove(&self, path: &str) -> Result<(), SftpError> {
        self.session
            .remove_file(path)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        self.session
            .rename(from, to)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn SftpReader>, SftpError> {
        let file = self
            .session
            .open(path)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        Ok(Box::new(RusshReader { file: Some(file) }))
    }

    async fn open_write(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn SftpWriter>, SftpError> {
        use russh_sftp::protocol::OpenFlags;
        let flags = if truncate {
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE
        } else {
            OpenFlags::CREATE | OpenFlags::WRITE
        };
        let file = self
            .session
            .open_with_flags(path, flags)
            .await
            .map_err(|error| SftpError::Backend(error.to_string()))?;
        Ok(Box::new(RusshWriter {
            file: Some(file),
            offset: 0,
        }))
    }
}
