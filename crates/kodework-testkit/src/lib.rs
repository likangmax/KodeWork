#![forbid(unsafe_code)]

//! Kodework test kit: fake adapters for offline integration and fault tests.

pub mod fake_sftp;
pub mod fake_sftp_server;
pub mod fake_ssh;
pub mod local_fs_backend;

use kodework_domain::Host;

#[derive(Debug, Clone)]
pub struct FakeRemoteHost {
    pub host: Host,
    pub ssh_available: bool,
    pub tailscale_available: bool,
    pub herdr_installed: bool,
    pub herdr_server_running: bool,
}

impl FakeRemoteHost {
    #[must_use]
    pub fn with_host(host: Host) -> Self {
        Self {
            host,
            ssh_available: true,
            tailscale_available: true,
            herdr_installed: false,
            herdr_server_running: false,
        }
    }

    #[must_use]
    pub fn herdr_ready(&self) -> bool {
        self.herdr_installed && self.herdr_server_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{Address, AddressId, AddressKind, HostId, RuntimeKind};

    #[test]
    fn fake_host_models_faults_without_network() {
        let host = Host {
            id: HostId::new(),
            label: "fixture".into(),
            username: "tester".into(),
            port: 22,
            auth_ref: None,
            auth_mode: kodework_domain::AuthenticationMode::Password,
            private_key_path: None,
            default_remote_path: "/".into(),
            jump: None,
            addresses: vec![Address {
                id: AddressId::new(),
                kind: AddressKind::Tailscale,
                hostname_or_ip: "100.64.0.3".into(),
                port: 22,
                priority: 1,
                enabled: true,
            }],
            tailscale: None,
            default_runtime: RuntimeKind::Tmux,
        };
        let mut fixture = FakeRemoteHost::with_host(host);
        assert!(!fixture.herdr_ready());
        fixture.herdr_installed = true;
        fixture.herdr_server_running = true;
        assert!(fixture.herdr_ready());
    }
}
