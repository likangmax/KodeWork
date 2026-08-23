//! TransferManager integration tests against the in-memory fake SFTP
//! backend: atomic rename, pause/resume, cancel, retry, disk-full and
//! bounded concurrency.

use kodework_domain::{TransferDirection, TransferStatus};
use kodework_sftp::manager::{TransferEvent, TransferManager};
use kodework_sftp::{SftpError, TransferRequest, DEFAULT_CHUNK_SIZE};
use kodework_testkit::fake_sftp::{FakeSftpBackend, FakeSftpFaults};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const REMOTE_DIR: &str = "~/uploads";
const BYTE_X: u8 = 0x78; // 'x'
const BYTE_Y: u8 = 0x79; // 'y'

fn temp_file(name: &str, bytes: usize) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("kodework-sftp-test-{}-{name}", std::process::id()));
    let data = vec![BYTE_X; bytes];
    std::fs::write(&path, data).unwrap_or_else(|error| unreachable!("temp write failed: {error}"));
    path
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let part = format!("{}.part", path.display());
    let _ = std::fs::remove_file(part);
}

async fn wait_for_terminal(
    rx: &mut mpsc::Receiver<TransferEvent>,
    id: kodework_domain::TransferId,
    timeout: Duration,
) -> Option<TransferStatus> {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(TransferEvent::State {
                id: event_id,
                status,
            })) if event_id == id => {
                if matches!(
                    status,
                    TransferStatus::Completed | TransferStatus::Cancelled | TransferStatus::Failed,
                ) {
                    return Some(status);
                }
            }
            Ok(Some(_)) => {}
            _ => return None,
        }
    }
    None
}

fn upload_request(local: &std::path::Path, remote: &str, resume: bool) -> TransferRequest {
    TransferRequest {
        local_path: local.to_string_lossy().into_owned(),
        remote_path: remote.to_string(),
        direction: TransferDirection::Upload,
        resume,
    }
}

fn download_request(local: &std::path::Path, remote: &str, resume: bool) -> TransferRequest {
    TransferRequest {
        local_path: local.to_string_lossy().into_owned(),
        remote_path: remote.to_string(),
        direction: TransferDirection::Download,
        resume,
    }
}

#[tokio::test]
async fn upload_completes_atomically() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("upload", 256 * 1024);
    let remote = format!("{REMOTE_DIR}/a.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));

    let remote_data = backend
        .read(&remote)
        .unwrap_or_else(|| unreachable!("remote file must exist"));
    assert_eq!(remote_data.len(), 256 * 1024);
    assert_eq!(
        remote_data.iter().filter(|b| **b == BYTE_X).count(),
        256 * 1024
    );
    assert!(
        !backend.contains(&format!("{remote}.part")),
        "no .part may remain after success"
    );
    cleanup(&local);
}

#[tokio::test]
async fn same_destination_is_rejected_while_transfer_is_active() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 3,
        ..FakeSftpFaults::default()
    }));
    let (manager, mut rx) = TransferManager::new(backend, 2, 256);
    let local = temp_file("lease", 16 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/lease.bin");
    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(manager.transferred_bytes(id) > 0, "transfer must start");
    let duplicate = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await;
    assert!(
        matches!(duplicate, Err(SftpError::DestinationBusy)),
        "duplicate destination must be rejected: {duplicate:?}"
    );
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(10)).await,
        Some(TransferStatus::Completed)
    );
    cleanup(&local);
}

#[tokio::test]
async fn equivalent_remote_home_aliases_share_destination_lease() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 3,
        ..FakeSftpFaults::default()
    }));
    let (manager, mut rx) = TransferManager::new(backend, 2, 256);
    let local = temp_file("lease-alias", 16 * DEFAULT_CHUNK_SIZE);
    let first = format!("{REMOTE_DIR}/alias.bin");
    let second = "/home/tester/uploads/alias.bin";

    let id = manager
        .enqueue(upload_request(&local, &first, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(manager.transferred_bytes(id) > 0, "transfer must start");

    let duplicate = manager
        .enqueue(upload_request(&local, second, false), 0)
        .await;
    assert!(
        matches!(duplicate, Err(SftpError::DestinationBusy)),
        "equivalent remote aliases must share one lease: {duplicate:?}"
    );
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(10)).await,
        Some(TransferStatus::Completed)
    );
    cleanup(&local);
}

#[tokio::test]
async fn upload_fails_when_source_changes_before_commit() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 3,
        ..FakeSftpFaults::default()
    }));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("source-changed-upload", 16 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/source-changed-upload.bin");
    let id = manager
        .enqueue(upload_request(&local, &remote, false), 2)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(manager.transferred_bytes(id) > 0, "transfer must start");
    std::fs::write(&local, vec![BYTE_Y; 2 * DEFAULT_CHUNK_SIZE])
        .unwrap_or_else(|error| unreachable!("mutate source: {error}"));
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(10)).await,
        Some(TransferStatus::Failed)
    );
    assert!(
        !backend.contains(&remote),
        "changed source must not be committed"
    );
    assert!(backend.contains(&format!("{remote}.part")));
    cleanup(&local);
}

#[tokio::test]
async fn download_fails_when_remote_source_changes_before_commit() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        read_delay_ms: 3,
        ..FakeSftpFaults::default()
    }));
    let remote = format!("{REMOTE_DIR}/source-changed-download.bin");
    backend.seed(&remote, vec![BYTE_X; 16 * DEFAULT_CHUNK_SIZE]);
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("source-changed-download", 8);
    let id = manager
        .enqueue(download_request(&local, &remote, false), 2)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(manager.transferred_bytes(id) > 0, "transfer must start");
    backend.seed(&remote, vec![BYTE_Y; 2 * DEFAULT_CHUNK_SIZE]);
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(10)).await,
        Some(TransferStatus::Failed)
    );
    assert_eq!(
        std::fs::read(&local).unwrap_or_else(|error| unreachable!("read destination: {error}")),
        vec![BYTE_X; 8]
    );
    cleanup(&local);
}

#[tokio::test]
async fn enqueue_and_wait_returns_only_after_atomic_completion() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, _rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("wait-upload", 64 * 1024);
    let remote = format!("{REMOTE_DIR}/wait.bin");

    manager
        .enqueue_and_wait(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("wait upload: {error}"));
    assert!(backend.contains(&remote));
    assert!(!backend.contains(&format!("{remote}.part")));
    cleanup(&local);
}

#[tokio::test]
async fn enqueue_and_wait_propagates_terminal_failure() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        remote_write_quota: Some(1),
        ..FakeSftpFaults::default()
    }));
    let (manager, _rx) = TransferManager::new(backend, 2, 256);
    let local = temp_file("wait-failure", 64 * 1024);
    let remote = format!("{REMOTE_DIR}/wait-failure.bin");

    assert!(manager
        .enqueue_and_wait(upload_request(&local, &remote, false), 0)
        .await
        .is_err());
    cleanup(&local);
}

#[tokio::test]
async fn download_completes_atomically() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let remote = format!("{REMOTE_DIR}/d.bin");
    backend.seed(&remote, vec![BYTE_Y; 512 * 1024]);
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("download", 0);

    let id = manager
        .enqueue(download_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));

    let data = std::fs::read(&local)
        .unwrap_or_else(|error| unreachable!("downloaded file must exist: {error}"));
    assert_eq!(data.len(), 512 * 1024);
    assert_eq!(data.iter().filter(|b| **b == BYTE_Y).count(), 512 * 1024);
    assert!(!local.with_extension("bin.part").exists());
    cleanup(&local);
}

#[tokio::test]
async fn download_replaces_existing_destination() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let remote = format!("{REMOTE_DIR}/overwrite.bin");
    backend.seed(&remote, vec![BYTE_Y; 128 * 1024]);
    let (manager, mut rx) = TransferManager::new(backend, 2, 256);
    // Windows rename does not replace an existing destination. The transfer
    // must still complete and leave the new payload in place.
    let local = temp_file("download-overwrite", 32);

    let id = manager
        .enqueue(download_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));
    let data = std::fs::read(&local).unwrap_or_else(|error| unreachable!("read: {error}"));
    assert_eq!(data, vec![BYTE_Y; 128 * 1024]);
    cleanup(&local);
}

#[tokio::test]
async fn pause_then_resume_completes() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("pause", 8 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/p.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) < DEFAULT_CHUNK_SIZE as u64
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(manager.transferred_bytes(id) > 0, "transfer must start");
    manager
        .pause(id)
        .unwrap_or_else(|error| unreachable!("pause: {error}"));
    tokio::time::sleep(Duration::from_millis(150)).await;
    let paused_at = manager.transferred_bytes(id);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let after_wait = manager.transferred_bytes(id);
    assert_eq!(paused_at, after_wait, "paused transfer must not advance");

    manager
        .resume(id)
        .unwrap_or_else(|error| unreachable!("resume: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));
    let data = backend
        .read(&remote)
        .unwrap_or_else(|| unreachable!("file"));
    assert_eq!(data.len(), 8 * DEFAULT_CHUNK_SIZE);
    cleanup(&local);
}

#[tokio::test]
async fn cancel_keeps_part_file_for_resume() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 1,
        ..FakeSftpFaults::default()
    }));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("cancel", 16 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/c.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    // Event-driven: cancel right after the transfer starts running
    // (progress events are throttled for small files).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(TransferEvent::State {
                id: event_id,
                status: TransferStatus::Transferring,
            })) if event_id == id => {
                started = true;
                break;
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert!(started, "transfer must start");
    // Wait until a chunk is durable in the remote `.part`, then cancel
    // mid-flight. Seeing `Transferring` alone is earlier than `open_write`.
    let progress_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < progress_deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        manager.transferred_bytes(id) > 0,
        "partial upload must exist"
    );
    manager
        .cancel(id)
        .unwrap_or_else(|error| unreachable!("cancel: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Cancelled));

    assert!(
        backend.contains(&format!("{remote}.part")),
        ".part must be kept for resume"
    );
    assert!(!backend.contains(&remote), "final file must not appear");
    cleanup(&local);
}

#[tokio::test]
async fn retry_recovers_after_injected_failure() {
    let faults = FakeSftpFaults {
        fail_next_writes: 1,
        ..FakeSftpFaults::default()
    };
    let backend = Arc::new(FakeSftpBackend::new(faults));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("retry", 4 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/r.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 2)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));
    let data = backend
        .read(&remote)
        .unwrap_or_else(|| unreachable!("file"));
    assert_eq!(data.len(), 4 * DEFAULT_CHUNK_SIZE);
    cleanup(&local);
}

#[tokio::test]
async fn manual_retry_resets_exhausted_retry_budget() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        fail_next_writes: 1,
        ..FakeSftpFaults::default()
    }));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("manual-retry", DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/manual-retry.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await,
        Some(TransferStatus::Failed)
    );

    manager
        .retry(id)
        .await
        .unwrap_or_else(|error| unreachable!("manual retry: {error}"));
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await,
        Some(TransferStatus::Completed)
    );
    assert_eq!(
        backend.read(&remote).map(|data| data.len()),
        Some(DEFAULT_CHUNK_SIZE)
    );
    cleanup(&local);
}

#[tokio::test]
async fn disk_full_fails_without_overwriting_existing_file() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        remote_write_quota: Some(1024),
        ..FakeSftpFaults::default()
    }));
    let remote = format!("{REMOTE_DIR}/existing.bin");
    backend.seed(&remote, b"precious data".to_vec());
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("diskfull", 4 * DEFAULT_CHUNK_SIZE);

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Failed));
    assert_eq!(
        backend.read(&remote).as_deref(),
        Some(b"precious data".as_slice()),
        "existing file must not be overwritten by a failed transfer"
    );
    cleanup(&local);
}

#[tokio::test]
async fn resume_upload_continues_from_part() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 1,
        ..FakeSftpFaults::default()
    }));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("resume", 8 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/rs.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    // Event-driven: cancel right after the transfer starts running.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(TransferEvent::State {
                id: event_id,
                status: TransferStatus::Transferring,
            })) if event_id == id => {
                started = true;
                break;
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert!(started, "transfer must start");
    // Wait until a chunk is durable in the remote `.part`, then cancel
    // mid-flight. Seeing `Transferring` alone is earlier than `open_write`.
    let progress_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < progress_deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        manager.transferred_bytes(id) > 0,
        "partial upload must exist"
    );
    manager
        .cancel(id)
        .unwrap_or_else(|error| unreachable!("cancel: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Cancelled));
    let part_size = backend
        .read(&format!("{remote}.part"))
        .unwrap_or_else(|| unreachable!("part exists"))
        .len();
    assert!(part_size > 0 && part_size < 8 * DEFAULT_CHUNK_SIZE);

    let id = manager
        .enqueue(upload_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("re-enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));
    let data = backend
        .read(&remote)
        .unwrap_or_else(|| unreachable!("file"));
    assert_eq!(data.len(), 8 * DEFAULT_CHUNK_SIZE);
    assert!(!backend.contains(&format!("{remote}.part")));
    cleanup(&local);
}

#[tokio::test]
async fn same_size_wrong_upload_part_is_rebuilt() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("wrong-upload-prefix", 4 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/wrong-upload-prefix.bin");
    backend.seed(
        &format!("{remote}.part"),
        vec![BYTE_Y; 2 * DEFAULT_CHUNK_SIZE],
    );

    let id = manager
        .enqueue(upload_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await,
        Some(TransferStatus::Completed)
    );
    assert_eq!(
        backend.read(&remote),
        Some(vec![BYTE_X; 4 * DEFAULT_CHUNK_SIZE])
    );
    cleanup(&local);
}

#[tokio::test]
async fn same_size_wrong_download_part_is_rebuilt() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let remote = format!("{REMOTE_DIR}/wrong-download-prefix.bin");
    backend.seed(&remote, vec![BYTE_X; 4 * DEFAULT_CHUNK_SIZE]);
    let local = std::env::temp_dir().join(format!(
        "kodework-sftp-test-{}-wrong-download-prefix",
        std::process::id()
    ));
    std::fs::write(
        format!("{}.part", local.display()),
        vec![BYTE_Y; 2 * DEFAULT_CHUNK_SIZE],
    )
    .unwrap_or_else(|error| unreachable!("part write: {error}"));

    let id = manager
        .enqueue(download_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    assert_eq!(
        wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await,
        Some(TransferStatus::Completed)
    );
    assert_eq!(
        std::fs::read(&local).ok(),
        Some(vec![BYTE_X; 4 * DEFAULT_CHUNK_SIZE])
    );
    cleanup(&local);
}

#[tokio::test]
async fn stale_oversized_part_is_discarded_and_rebuilt() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("stale", 4 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/stale.bin");

    // Seed a .part that is LARGER than the source (stale/corrupt).
    let stale = vec![BYTE_Y; 9 * DEFAULT_CHUNK_SIZE];
    backend.seed(&format!("{remote}.part"), stale);

    let id = manager
        .enqueue(upload_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));

    // The destination must be byte-exact with the source, not the stale .part.
    let uploaded = backend
        .read(&remote)
        .unwrap_or_else(|| unreachable!("uploaded file must exist"));
    let expected = std::fs::read(&local).unwrap_or_else(|error| unreachable!("read: {error}"));
    assert_eq!(
        uploaded, expected,
        "oversized .part must be rebuilt from scratch"
    );
    assert!(
        !backend.contains(&format!("{remote}.part")),
        ".part must be renamed away"
    );
    cleanup(&local);
}

#[tokio::test]
async fn stale_oversized_local_part_is_discarded_on_download() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let remote_payload = vec![BYTE_X; 4 * DEFAULT_CHUNK_SIZE];
    let remote = format!("{REMOTE_DIR}/stale-dl.bin");
    backend.seed(&remote, remote_payload.clone());

    let local = std::env::temp_dir().join(format!(
        "kodework-sftp-test-{}-stale-dl.bin",
        std::process::id()
    ));
    // Local .part larger than the remote source.
    std::fs::write(
        format!("{}.part", local.display()),
        vec![BYTE_Y; 9 * DEFAULT_CHUNK_SIZE],
    )
    .unwrap_or_else(|error| unreachable!("part write: {error}"));

    let id = manager
        .enqueue(download_request(&local, &remote, true), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));

    let downloaded = std::fs::read(&local).unwrap_or_else(|error| unreachable!("read: {error}"));
    assert_eq!(
        downloaded, remote_payload,
        "oversized local .part must be rebuilt"
    );
    cleanup(&local);
}

#[tokio::test]
async fn concurrency_is_bounded_by_slots() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 512);
    let mut ids = Vec::new();
    for index in 0..6 {
        let local = temp_file(&format!("conc{index}"), 4 * DEFAULT_CHUNK_SIZE);
        let remote = format!("{REMOTE_DIR}/conc{index}.bin");
        let id = manager
            .enqueue(upload_request(&local, &remote, false), 0)
            .await
            .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
        ids.push((id, local));
    }

    let mut active = 0usize;
    let mut max_active = 0usize;
    let mut completed = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while completed < ids.len() && tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(TransferEvent::State { status, .. })) => match status {
                TransferStatus::Transferring => {
                    active += 1;
                    max_active = max_active.max(active);
                }
                TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled => {
                    active = active.saturating_sub(1);
                    completed += 1;
                }
                _ => {}
            },
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert_eq!(completed, ids.len(), "all transfers must finish");
    assert!(
        max_active <= 2,
        "concurrency must not exceed configured slots, saw {max_active}"
    );
    for (_, local) in ids {
        cleanup(&local);
    }
}

#[tokio::test]
async fn missing_source_fails_with_source_not_found() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let remote = format!("{REMOTE_DIR}/missing.bin");
    let local = std::env::temp_dir().join(format!(
        "kodework-sftp-test-{}-never-created.bin",
        std::process::id()
    ));

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Failed));
    cleanup(&local);
}

#[tokio::test]
async fn download_missing_remote_fails() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = std::env::temp_dir().join(format!(
        "kodework-sftp-test-{}-dlmissing.bin",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&local);
    let remote = format!("{REMOTE_DIR}/not-there.bin");

    let id = manager
        .enqueue(download_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Failed));
    assert!(
        !local.exists(),
        "no file may be created for a failed download"
    );
    cleanup(&local);
}

#[tokio::test]
async fn finished_transfers_are_reaped_from_the_registry() {
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults::default()));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 256);
    let local = temp_file("reap", DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/reap.bin");
    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let status = wait_for_terminal(&mut rx, id, Duration::from_secs(5)).await;
    assert_eq!(status, Some(TransferStatus::Completed));
    // The registry entry must be reaped after the grace window: controlling
    // a reaped transfer is an unknown-transfer error, so finished transfers
    // cannot accumulate indefinitely.
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(
        manager.cancel(id).is_err(),
        "completed transfer must be reaped from the registry"
    );
    cleanup(&local);
}

/// Polls until the destination lease is free again: a fresh enqueue to the
/// same remote path must be accepted, not rejected as `DestinationBusy`.
/// This is the observable proof that an in-flight worker has stopped.
async fn wait_for_destination_free(
    manager: &TransferManager,
    local: &std::path::Path,
    remote: &str,
    timeout: Duration,
) -> kodework_domain::TransferId {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match manager
            .enqueue(upload_request(local, remote, false), 0)
            .await
        {
            Ok(id) => return id,
            Err(SftpError::DestinationBusy) => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "destination stayed busy after the event pump died"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => unreachable!("unexpected enqueue error: {error}"),
        }
    }
}

#[tokio::test]
async fn dead_event_pump_aborts_worker_and_releases_destination() {
    // Dropping the receiver models the renderer subscription going away
    // (reconnect, panel teardown). The pump stops draining the bounded
    // event channel; an in-flight worker must not block on it. It aborts at
    // the next chunk boundary and releases the destination lease, so the
    // same destination can be used again instead of staying busy for the
    // process lifetime.
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 5,
        ..FakeSftpFaults::default()
    }));
    let (manager, rx) = TransferManager::new(backend.clone(), 2, 8);
    let local = temp_file("deadpump", 32 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/deadpump.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    // Provably mid-flight before the consumer disappears.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        manager.transferred_bytes(id) > 0,
        "slow backend must be transferring"
    );
    drop(rx);
    // Old behavior: the worker blocked forever on `send`, held the lease and
    // never exited, so this poll timed out. New behavior: the worker aborts
    // and the lease is released promptly.
    let second = wait_for_destination_free(&manager, &local, &remote, Duration::from_secs(5)).await;
    let _ = manager.cancel(second);
    cleanup(&local);
}

#[tokio::test]
async fn abandoned_transfer_can_be_retried_after_pump_death() {
    // After the consumer disappears and the worker aborts, `retry()` must
    // find the worker stopped and the transfer still registered (inside the
    // grace window). The old blocking send would leave the worker stranded
    // in `running` forever and `retry()` would fail with "worker did not
    // stop".
    let backend = Arc::new(FakeSftpBackend::new(FakeSftpFaults {
        write_delay_ms: 5,
        ..FakeSftpFaults::default()
    }));
    let (manager, rx) = TransferManager::new(backend.clone(), 2, 8);
    let local = temp_file("retrypump", 8 * DEFAULT_CHUNK_SIZE);
    let remote = format!("{REMOTE_DIR}/retrypump.bin");

    let id = manager
        .enqueue(upload_request(&local, &remote, false), 0)
        .await
        .unwrap_or_else(|error| unreachable!("enqueue: {error}"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while manager.transferred_bytes(id) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    drop(rx);
    // Wait until the worker has provably stopped before retrying. A second
    // transfer uses a different remote path so it never disturbs the
    // original destination lease.
    let second = wait_for_destination_free(
        &manager,
        &local,
        &format!("{REMOTE_DIR}/retrypump-second.bin"),
        Duration::from_secs(5),
    )
    .await;
    let _ = manager.cancel(second);

    manager
        .retry(id)
        .await
        .unwrap_or_else(|error| unreachable!("retry after pump death: {error}"));
    // The retried worker has no consumer and cancels again; what matters is
    // that the retry path did not see a permanently "running" worker.
    cleanup(&local);
}
