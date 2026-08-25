#![forbid(unsafe_code)]

//! russh client handler: forwards channel/connection events into a bounded
//! channel with real backpressure (a full buffer pauses the SSH stream
//! instead of dropping bytes), and routes host-key checks through the
//! decision broker.

use crate::host_key::HostKeyBroker;
use crate::SshError;
use kodework_domain::HostId;
use russh::client::{self, DisconnectReason, Session};
use russh::keys::ssh_key::PublicKey;
use russh::ChannelId;
use russh::Sig;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Events produced by the SSH connection, tagged with the channel id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SessionEvent {
    /// stdout bytes for a channel.
    Data { channel: u32, bytes: Vec<u8> },
    /// stderr (ext == 1) or other extended stream bytes.
    ExtendedData {
        channel: u32,
        ext: u32,
        bytes: Vec<u8>,
    },
    /// The remote process on the channel exited.
    ExitStatus { channel: u32, status: u32 },
    /// The remote process was terminated by a signal.
    ExitSignal { channel: u32, signal: String },
    /// Server closed the channel.
    ChannelClosed { channel: u32 },
    /// Pre-authentication banner from the server.
    AuthBanner(String),
    /// The SSH connection ended.
    Disconnected { description: String },
    /// A transport-level error occurred.
    Error { description: String },
}

/// Client handler handed to russh. Holds no secrets.
pub struct SshHandler {
    hostname: String,
    port: u16,
    logical_host_id: Option<HostId>,
    host_key: Arc<HostKeyBroker>,
    events: mpsc::Sender<SessionEvent>,
    /// Channel numbers owned by non-terminal traffic (SFTP subsystem,
    /// leak into the terminal event stream.
    filtered_channels: Arc<Mutex<HashSet<u32>>>,
    /// Connection generation; attached by the caller to correlate events.
    generation: u64,
}

impl SshHandler {
    #[must_use]
    pub fn new(
        hostname: String,
        port: u16,
        logical_host_id: Option<HostId>,
        host_key: Arc<HostKeyBroker>,
        events: mpsc::Sender<SessionEvent>,
        generation: u64,
        filtered_channels: Arc<Mutex<HashSet<u32>>>,
    ) -> Self {
        Self {
            hostname,
            port,
            logical_host_id,
            host_key,
            events,
            generation,
            filtered_channels,
        }
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn is_sftp_channel(&self, channel: u32) -> bool {
        self.filtered_channels
            .lock()
            .map(|guard| guard.contains(&channel))
            .unwrap_or(false)
    }

    async fn send_event(&self, event: SessionEvent) -> Result<(), SshError> {
        self.events
            .send(event)
            .await
            .map_err(|_| SshError::Cancelled)
    }
}

impl client::Handler for SshHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        self.host_key
            .verify_for_host(
                self.logical_host_id,
                &self.hostname,
                self.port,
                server_public_key,
            )
            .await
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !banner.trim().is_empty() {
            self.send_event(SessionEvent::AuthBanner(banner.to_string()))
                .await?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.is_sftp_channel(channel.number()) {
            return Ok(());
        }
        self.send_event(SessionEvent::Data {
            channel: channel.number(),
            bytes: data.to_vec(),
        })
        .await
    }

    async fn extended_data(
        &mut self,
        channel: ChannelId,
        ext: u32,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.is_sftp_channel(channel.number()) {
            return Ok(());
        }
        self.send_event(SessionEvent::ExtendedData {
            channel: channel.number(),
            ext,
            bytes: data.to_vec(),
        })
        .await
    }

    async fn exit_status(
        &mut self,
        channel: ChannelId,
        exit_status: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.is_sftp_channel(channel.number()) {
            return Ok(());
        }
        self.send_event(SessionEvent::ExitStatus {
            channel: channel.number(),
            status: exit_status,
        })
        .await
    }

    async fn exit_signal(
        &mut self,
        channel: ChannelId,
        signal_name: Sig,
        _core_dumped: bool,
        _error_message: &str,
        _lang_tag: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.is_sftp_channel(channel.number()) {
            return Ok(());
        }
        self.send_event(SessionEvent::ExitSignal {
            channel: channel.number(),
            signal: sig_name(signal_name),
        })
        .await
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let filtered = self.is_sftp_channel(channel.number());
        // Reap the filter entry so long-lived transports do not
        // accumulate dead channel ids in the set.
        if let Ok(mut guard) = self.filtered_channels.lock() {
            guard.remove(&channel.number());
        }
        if filtered {
            return Ok(());
        }
        self.send_event(SessionEvent::ChannelClosed {
            channel: channel.number(),
        })
        .await
    }

    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        match reason {
            DisconnectReason::ReceivedDisconnect(info) => {
                let description = if info.message.is_empty() {
                    "remote peer disconnected".to_string()
                } else {
                    info.message
                };
                self.send_event(SessionEvent::Disconnected { description })
                    .await?;
                Ok(())
            }
            DisconnectReason::Error(error) => {
                let description = error.to_string();
                self.send_event(SessionEvent::Error { description }).await?;
                Err(error)
            }
        }
    }
}

#[must_use]
pub(crate) fn sig_name(signal: Sig) -> String {
    match signal {
        Sig::ABRT => "ABRT".into(),
        Sig::ALRM => "ALRM".into(),
        Sig::FPE => "FPE".into(),
        Sig::HUP => "HUP".into(),
        Sig::ILL => "ILL".into(),
        Sig::INT => "INT".into(),
        Sig::KILL => "KILL".into(),
        Sig::PIPE => "PIPE".into(),
        Sig::QUIT => "QUIT".into(),
        Sig::SEGV => "SEGV".into(),
        Sig::TERM => "TERM".into(),
        Sig::USR1 => "USR1".into(),
        Sig::Custom(custom) => custom,
    }
}
