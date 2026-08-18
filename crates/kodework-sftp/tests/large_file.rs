//! Large-file streaming test: proves the TransferManager stays streaming
//! (64 KiB chunks) and byte-exact on a 512 MiB round trip through the
//! local-filesystem backend. No read_to_end anywhere in the path.

use kodework_domain::{TransferDirection, TransferStatus};
use kodework_sftp::manager::{TransferEvent, TransferManager};
use kodework_sftp::TransferRequest;
use kodework_testkit::local_fs_backend::LocalFsBackend;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const BIG_SIZE: u64 = 512 * 1024 * 1024;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kodework-big-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| unreachable!("scratch: {error}"));
    dir
}

async fn wait_terminal(
    rx: &mut mpsc::Receiver<TransferEvent>,
    id: kodework_domain::TransferId,
) -> TransferStatus {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(TransferEvent::State { id: eid, status })) if eid == id => {
                if matches!(
                    status,
                    TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled
                ) {
                    return status;
                }
            }
            Ok(Some(_)) => {}
            _ => return TransferStatus::Failed,
        }
    }
    TransferStatus::Failed
}

#[tokio::test]
async fn large_file_round_trip_is_streaming_and_byte_exact() {
    let dir = scratch_dir("big");
    let source = dir.join("source.bin");
    let remote_dir = dir.join("remote");
    let download = dir.join("download.bin");
    std::fs::create_dir_all(&remote_dir)
        .unwrap_or_else(|error| unreachable!("remote dir: {error}"));

    // Generate a 512 MiB deterministic pseudo-random source.
    {
        let mut file =
            std::fs::File::create(&source).unwrap_or_else(|error| unreachable!("create: {error}"));
        use std::io::Write;
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut block = vec![0u8; 1024 * 1024];
        let mut written = 0u64;
        while written < BIG_SIZE {
            for chunk in block.chunks_mut(8) {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                chunk.copy_from_slice(&seed.to_le_bytes());
            }
            file.write_all(&block)
                .unwrap_or_else(|error| unreachable!("write: {error}"));
            written += block.len() as u64;
        }
    }

    let backend = Arc::new(LocalFsBackend::new(&remote_dir));
    let (manager, mut rx) = TransferManager::new(backend.clone(), 2, 512);

    let remote = "/big.bin".to_string();
    let id = manager
        .enqueue(
            TransferRequest {
                local_path: source.to_string_lossy().into_owned(),
                remote_path: remote.clone(),
                direction: TransferDirection::Upload,
                resume: false,
            },
            0,
        )
        .await
        .unwrap_or_else(|error| unreachable!("enqueue upload: {error}"));
    assert_eq!(
        wait_terminal(&mut rx, id).await,
        TransferStatus::Completed,
        "512 MiB upload must complete"
    );

    let id = manager
        .enqueue(
            TransferRequest {
                local_path: download.to_string_lossy().into_owned(),
                remote_path: remote.clone(),
                direction: TransferDirection::Download,
                resume: false,
            },
            0,
        )
        .await
        .unwrap_or_else(|error| unreachable!("enqueue download: {error}"));
    assert_eq!(
        wait_terminal(&mut rx, id).await,
        TransferStatus::Completed,
        "512 MiB download must complete"
    );

    // Byte-exact comparison.
    let source_meta =
        std::fs::metadata(&source).unwrap_or_else(|error| unreachable!("meta: {error}"));
    let download_meta =
        std::fs::metadata(&download).unwrap_or_else(|error| unreachable!("meta: {error}"));
    assert_eq!(source_meta.len(), BIG_SIZE);
    assert_eq!(download_meta.len(), BIG_SIZE);
    let mut same = true;
    let mut a = std::fs::File::open(&source).unwrap_or_else(|error| unreachable!("open: {error}"));
    let mut b =
        std::fs::File::open(&download).unwrap_or_else(|error| unreachable!("open: {error}"));
    use std::io::Read;
    let mut buf_a = vec![0u8; 64 * 1024];
    let mut buf_b = vec![0u8; 64 * 1024];
    loop {
        let na = a
            .read(&mut buf_a)
            .unwrap_or_else(|error| unreachable!("read a: {error}"));
        let nb = b
            .read(&mut buf_b)
            .unwrap_or_else(|error| unreachable!("read b: {error}"));
        if na != nb || (na > 0 && buf_a[..na] != buf_b[..nb]) {
            same = false;
            break;
        }
        if na == 0 {
            break;
        }
    }
    assert!(
        same,
        "downloaded content must match the source byte-for-byte"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
