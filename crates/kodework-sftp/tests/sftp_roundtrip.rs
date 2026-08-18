//! End-to-end SFTP protocol tests over an in-memory duplex stream: the
//! real russh-sftp client talks to the fake in-memory SFTP server, and
//! the transfer manager moves files through it byte-exactly.

use kodework_domain::{TransferDirection, TransferId, TransferStatus};
use kodework_sftp::backend::{RusshSftpBackend, SftpBackend};
use kodework_sftp::manager::{TransferEvent, TransferManager};
use kodework_sftp::{part_path, TransferRequest, DEFAULT_CHUNK_SIZE};
use kodework_testkit::fake_sftp_server::{FakeSftpContent, InMemorySftp};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::duplex;
use tokio::sync::mpsc;

/// Builds a client SFTP session connected to an in-memory server.
async fn in_memory_client(content: FakeSftpContent) -> Arc<russh_sftp::client::SftpSession> {
    let (client_stream, server_stream) = duplex(64 * 1024);
    tokio::spawn(russh_sftp::server::run(
        server_stream,
        InMemorySftp::new(content),
    ));
    let session = russh_sftp::client::SftpSession::new(client_stream)
        .await
        .unwrap_or_else(|error| unreachable!("sftp init: {error}"));
    session.set_timeout(10);
    Arc::new(session)
}

fn sample_content() -> FakeSftpContent {
    FakeSftpContent {
        files: HashMap::from([
            (
                "/home/tester/notes.txt".to_string(),
                b"hello sftp
"
                .to_vec(),
            ),
            (
                "/home/tester/code/main.rs".to_string(),
                b"fn main() {}
"
                .to_vec(),
            ),
            ("/home/tester/data.bin".to_string(), vec![0xAA; 300_000]),
        ]),
        dirs: vec!["/home/tester".to_string(), "/home/tester/code".to_string()],
    }
}

#[tokio::test]
async fn list_returns_files_and_directories() {
    let client = in_memory_client(sample_content()).await;
    let backend = RusshSftpBackend::new(client);

    let stat = backend
        .stat("/home/tester/notes.txt")
        .await
        .unwrap_or_else(|error| unreachable!("stat: {error}"));
    assert!(stat.is_some(), "notes.txt must exist");

    let entries = backend
        .list("/home/tester")
        .await
        .unwrap_or_else(|error| unreachable!("list: {error}"));
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert!(names.contains(&"notes.txt"), "got {names:?}");
    assert!(names.contains(&"code"), "got {names:?}");
    assert!(names.contains(&"data.bin"), "got {names:?}");

    let notes = entries
        .iter()
        .find(|entry| entry.name == "notes.txt")
        .unwrap_or_else(|| unreachable!("notes.txt"));
    assert_eq!(notes.size, 11);
    assert!(!notes.is_dir);

    let code = entries
        .iter()
        .find(|entry| entry.name == "code")
        .unwrap_or_else(|| unreachable!("code"));
    assert!(code.is_dir);
}

#[tokio::test]
async fn list_missing_directory_is_typed_error() {
    let client = in_memory_client(sample_content()).await;
    let backend = RusshSftpBackend::new(client);
    let error = match backend.list("/nope").await {
        Err(error) => error,
        Ok(_) => unreachable!("missing directory must error"),
    };
    assert!(error.to_string().contains("backend error"), "got {error:?}");
}

#[tokio::test]
async fn upload_download_round_trip_is_byte_exact() {
    let client = in_memory_client(sample_content()).await;
    let backend = Arc::new(RusshSftpBackend::new(client)) as Arc<dyn SftpBackend>;
    let (manager, mut events) = TransferManager::new(backend.clone(), 2, 256);

    // upload a local temp file
    let local = std::env::temp_dir().join(format!("kodework-upload-{}.bin", std::process::id()));
    let payload: Vec<u8> = (0..200_000).map(|i| (i % 251) as u8).collect();
    std::fs::write(&local, &payload).unwrap_or_else(|error| unreachable!("write: {error}"));

    let request = TransferRequest {
        local_path: local.to_string_lossy().into_owned(),
        remote_path: "/home/tester/uploaded.bin".into(),
        direction: TransferDirection::Upload,
        resume: true,
    };
    let id = manager
        .enqueue(request, 1)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    wait_for_state(&mut events, id, TransferStatus::Completed).await;

    // the .part file must have been renamed away
    let meta = backend
        .stat("/home/tester/uploaded.bin")
        .await
        .unwrap_or_else(|error| unreachable!("stat: {error}"))
        .unwrap_or_else(|| unreachable!("uploaded.bin must exist"));
    assert_eq!(meta.size, payload.len() as u64);
    let part = backend
        .stat(&part_path("/home/tester/uploaded.bin"))
        .await
        .unwrap_or_else(|error| unreachable!("stat part: {error}"));
    assert!(part.is_none(), ".part must be renamed on success");

    // download back and compare
    let download =
        std::env::temp_dir().join(format!("kodework-download-{}.bin", std::process::id()));
    let request = TransferRequest {
        local_path: download.to_string_lossy().into_owned(),
        remote_path: "/home/tester/uploaded.bin".into(),
        direction: TransferDirection::Download,
        resume: true,
    };
    let id = manager
        .enqueue(request, 1)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    wait_for_state(&mut events, id, TransferStatus::Completed).await;

    let downloaded = std::fs::read(&download).unwrap_or_else(|error| unreachable!("read: {error}"));
    assert_eq!(downloaded.len(), payload.len(), "byte-exact length");
    assert_eq!(downloaded, payload, "byte-exact content");

    std::fs::remove_file(&local).ok();
    std::fs::remove_file(&download).ok();
}

#[tokio::test]
async fn cancel_leaves_part_file_for_resume() {
    let client = in_memory_client(sample_content()).await;
    let backend = Arc::new(RusshSftpBackend::new(client)) as Arc<dyn SftpBackend>;
    let (manager, mut events) = TransferManager::new(backend.clone(), 2, 256);

    let local = std::env::temp_dir().join(format!("kodework-cancel-{}.bin", std::process::id()));
    std::fs::write(&local, vec![7u8; 64 * 1024 * 1024])
        .unwrap_or_else(|error| unreachable!("write: {error}"));
    let id = manager
        .enqueue(
            TransferRequest {
                local_path: local.to_string_lossy().into_owned(),
                remote_path: "/home/tester/cancel.bin".into(),
                direction: TransferDirection::Upload,
                resume: true,
            },
            0,
        )
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));

    // cancel once the worker is provably transferring
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker never started"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    manager
        .cancel(id)
        .unwrap_or_else(|error| unreachable!("cancel: {error}"));
    wait_for_state(&mut events, id, TransferStatus::Cancelled).await;

    let part = backend
        .stat(&part_path("/home/tester/cancel.bin"))
        .await
        .unwrap_or_else(|error| unreachable!("stat part: {error}"));
    assert!(part.is_some(), "cancel must keep the .part for resume");
    std::fs::remove_file(&local).ok();
}

/// Waits for the transfer event stream to report `status` for `id`
/// (with a deadline). Fails fast on a Failed event.
async fn wait_for_state(
    events: &mut mpsc::Receiver<TransferEvent>,
    id: TransferId,
    status: TransferStatus,
) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "transfer did not reach {status:?}");
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .unwrap_or_else(|_| unreachable!("event deadline"))
            .unwrap_or_else(|| unreachable!("event stream closed"));
        match event {
            TransferEvent::State {
                id: event_id,
                status: event_status,
            } if event_id == id && event_status == status => return,
            TransferEvent::Failed {
                id: event_id,
                message,
            } if event_id == id => {
                unreachable!("transfer failed: {message}")
            }
            _ => {}
        }
    }
}

#[test]
fn chunk_size_is_bounded_streaming() {
    assert_eq!(DEFAULT_CHUNK_SIZE, 256 * 1024);
}
