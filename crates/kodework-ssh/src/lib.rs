#![forbid(unsafe_code)]

//! Kodework SSH boundary: russh-backed connection, PTY, host-key policy and
//! bounded event output. This crate is Tauri-independent and async.

pub mod aggregator;
pub mod connection;
pub mod handler;
pub mod host_key;
pub mod keyboard_interactive;

pub use connection::{
    AuthMethod, CommandOutput, ConnectionOptions, ProxyCommand, SshConnection, SshExec, SshPty,
};

use std::io;
use thiserror::Error;

/// Typed SSH boundary error. Every variant carries a stable, user-safe
/// message; the core layer maps these to `kodework_network::FailureClass`
/// for fallback policy decisions.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SshError {
    /// The server key differs from the key we recorded for this host.
    #[error("host key changed; connection blocked")]
    HostKeyChanged,
    /// The user rejected an unknown host key.
    #[error("host key was rejected")]
    HostKeyRejected,
    /// No host-key decision arrived before the decision deadline.
    #[error("host key decision timed out")]
    HostKeyDecisionTimeout,
    /// The local trust database could not be read. This is intentionally
    /// distinct from an absent record so storage failures cannot become an
    /// unknown-host prompt.
    #[error("host-key trust store unavailable: {0}")]
    HostKeyStoreUnavailable(String),
    /// Authentication failed after all configured methods were tried.
    #[error("authentication failed")]
    AuthenticationFailed,
    /// A configured authentication method cannot be used in this build.
    #[error("authentication method is unavailable: {0}")]
    AuthMethodUnavailable(&'static str),
    /// The connect/operation deadline expired.
    #[error("connection timed out")]
    Timeout,
    /// TCP connect was refused by the remote host.
    #[error("connection refused by the remote host")]
    ConnectionRefused,
    /// The remote host could not be reached (DNS/network failure).
    #[error("remote host is unreachable")]
    Unreachable,
    /// The operation was cancelled by the caller.
    #[error("connection was cancelled")]
    Cancelled,
    /// Invalid caller configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    /// SSH protocol-level failure with a diagnostic description.
    #[error("ssh protocol error: {0}")]
    Protocol(String),
    /// Local I/O failure.
    #[error("io error: {0}")]
    Io(String),
    /// The remote process ended without a usable exit status.
    #[error("remote process ended without an exit status")]
    MissingExitStatus,
    /// The channel was closed by the server before the operation completed.
    #[error("channel closed by server")]
    ChannelClosed,
}

impl From<russh::Error> for SshError {
    fn from(error: russh::Error) -> Self {
        match error {
            russh::Error::ConnectionTimeout
            | russh::Error::KeepaliveTimeout
            | russh::Error::InactivityTimeout => SshError::Timeout,
            russh::Error::IO(io) => io.into(),
            russh::Error::KeyChanged { .. } => SshError::HostKeyChanged,
            russh::Error::HUP => SshError::ChannelClosed,
            other => SshError::Protocol(other.to_string()),
        }
    }
}

impl From<russh::keys::Error> for SshError {
    fn from(error: russh::keys::Error) -> Self {
        SshError::InvalidConfiguration(format!("key error: {error}"))
    }
}

impl From<io::Error> for SshError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::ConnectionRefused => SshError::ConnectionRefused,
            io::ErrorKind::TimedOut => SshError::Timeout,
            io::ErrorKind::HostUnreachable | io::ErrorKind::NetworkUnreachable => {
                SshError::Unreachable
            }
            io::ErrorKind::NotFound => {
                SshError::InvalidConfiguration(format!("path not found: {error}"))
            }
            _ => SshError::Io(error.to_string()),
        }
    }
}

/// Returns whether trying another address for the same logical host is safe.
/// Keep this as an explicit transport-error allowlist: configuration,
/// authentication, host identity, cancellation and protocol failures require
/// user action or investigation and must not be hidden by address fallback.
#[must_use]
pub fn address_fallback_is_retryable(error: &SshError) -> bool {
    matches!(
        error,
        SshError::Timeout | SshError::ConnectionRefused | SshError::Unreachable
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_address_transport_failures_fallback() {
        assert!(!address_fallback_is_retryable(&SshError::HostKeyChanged));
        assert!(!address_fallback_is_retryable(
            &SshError::AuthenticationFailed
        ));
        assert!(!address_fallback_is_retryable(
            &SshError::InvalidConfiguration("bad key".into())
        ));
        assert!(!address_fallback_is_retryable(&SshError::Cancelled));
        assert!(address_fallback_is_retryable(&SshError::Timeout));
        assert!(address_fallback_is_retryable(&SshError::ConnectionRefused));
        assert!(address_fallback_is_retryable(&SshError::Unreachable));
    }

    #[test]
    fn io_error_mapping_is_typed() {
        let refused = SshError::from(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "tcp connect failed",
        ));
        assert_eq!(refused, SshError::ConnectionRefused);
        let timed_out = SshError::from(io::Error::new(io::ErrorKind::TimedOut, "t"));
        assert_eq!(timed_out, SshError::Timeout);
        let other = SshError::from(io::Error::other("boom"));
        assert!(matches!(other, SshError::Io(_)));
    }
}
