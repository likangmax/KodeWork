#![forbid(unsafe_code)]

//! SFTP file operations and the streaming TransferManager.
//!
//! Transfers stream in fixed-size chunks (never `read_to_end`), write to a
//! `.part` file and atomically rename on success. Pause, resume, cancel and
//! retry are idempotent and driven by shared control flags so a slow or
//! stalled backend never blocks the manager.

pub mod backend;
pub mod manager;

use kodework_domain::TransferDirection;
use thiserror::Error;

/// Default streaming chunk size.
pub const DEFAULT_CHUNK_SIZE: usize = 256 * 1024;
/// Default maximum concurrent transfers.
pub const DEFAULT_MAX_CONCURRENCY: usize = 2;
/// Hard ceiling for concurrent transfers (GOAL §10.1).
pub const MAX_CONCURRENCY_CEILING: usize = 4;

/// One transfer request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    pub local_path: String,
    pub remote_path: String,
    pub direction: TransferDirection,
    /// Whether an interrupted transfer may resume from the existing `.part`.
    pub resume: bool,
}

/// Progress snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TransferProgress {
    pub transferred: u64,
    pub total: Option<u64>,
    /// Bytes per second since the previous progress event.
    pub speed_bps: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SftpError {
    #[error("local path must not be empty")]
    EmptyLocalPath,
    #[error("remote path must be ~, absolute, or start with ~/ ")]
    InvalidRemotePath,
    #[error("transfer cancelled")]
    Cancelled,
    #[error("transfer paused")]
    Paused,
    #[error("local or remote disk is full")]
    DiskFull,
    #[error("source file not found")]
    SourceNotFound,
    #[error("transfer failed after {0} retries")]
    RetriesExhausted(u32),
    #[error("unknown transfer id")]
    UnknownTransfer,
    #[error("backend error: {0}")]
    Backend(String),
}

pub fn validate_request(request: &TransferRequest) -> Result<(), SftpError> {
    if request.local_path.trim().is_empty() || request.local_path.chars().any(char::is_control) {
        return Err(SftpError::EmptyLocalPath);
    }
    if !(request.remote_path == "~"
        || request.remote_path.starts_with("~/")
        || request.remote_path.starts_with('/'))
        || request.remote_path.chars().any(char::is_control)
    {
        return Err(SftpError::InvalidRemotePath);
    }
    Ok(())
}

#[must_use]
pub fn part_path(path: &str) -> String {
    format!("{path}.part")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_bounded_streaming_defaults() {
        assert_eq!(DEFAULT_CHUNK_SIZE, 262_144);
        assert_eq!(DEFAULT_MAX_CONCURRENCY, 2);
        assert_eq!(MAX_CONCURRENCY_CEILING, 4);
    }

    #[test]
    fn validates_and_stages_transfer() {
        let request = TransferRequest {
            local_path: "C:/tmp/a.bin".into(),
            remote_path: "~/uploads/a.bin".into(),
            direction: TransferDirection::Upload,
            resume: true,
        };
        assert!(validate_request(&request).is_ok());
        assert_eq!(part_path(&request.remote_path), "~/uploads/a.bin.part");
    }

    #[test]
    fn rejects_control_characters_in_paths() {
        let local = TransferRequest {
            local_path: "C:/tmp/a\n.bin".into(),
            remote_path: "~/uploads/a.bin".into(),
            direction: TransferDirection::Upload,
            resume: true,
        };
        assert_eq!(validate_request(&local), Err(SftpError::EmptyLocalPath));

        let remote = TransferRequest {
            local_path: "C:/tmp/a.bin".into(),
            remote_path: "~/uploads/a\t.bin".into(),
            direction: TransferDirection::Upload,
            resume: true,
        };
        assert_eq!(validate_request(&remote), Err(SftpError::InvalidRemotePath));
    }
}
