#![forbid(unsafe_code)]

//! Streaming transfer manager with bounded concurrency, idempotent
//! pause/resume/cancel/retry and `.part` + atomic-rename semantics.

use crate::backend::SftpBackend;
use crate::{part_path, SftpError, TransferProgress, TransferRequest, DEFAULT_CHUNK_SIZE};
use kodework_domain::{TransferDirection, TransferId, TransferStatus};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Notify, Semaphore};

/// How long a finished transfer entry stays queryable before reaping.
const REAP_DELAY: Duration = Duration::from_secs(5);

/// Events emitted by the transfer manager (bounded, ordered).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TransferEvent {
    Progress {
        id: TransferId,
        progress: TransferProgress,
    },
    State {
        id: TransferId,
        status: TransferStatus,
    },
    Failed {
        id: TransferId,
        message: String,
    },
}

/// Per-transfer control flags shared with the worker task.
struct TransferControls {
    paused: AtomicBool,
    cancelled: AtomicBool,
    running: AtomicBool,
    generation: AtomicU64,
    max_retries: u32,
    resume_notify: Notify,
    retries_left: AtomicU32,
    transferred: AtomicU64,
}

impl TransferControls {
    fn new(retries: u32) -> Self {
        Self {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            running: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            max_retries: retries,
            resume_notify: Notify::new(),
            retries_left: AtomicU32::new(retries),
            transferred: AtomicU64::new(0),
        }
    }
}

/// Clone-safe transfer manager. Use [`TransferManager::new`] to obtain the
/// manager together with the bounded event stream.
#[derive(Clone)]
pub struct TransferManager {
    backend: Arc<dyn SftpBackend>,
    max_concurrency: usize,
    semaphore: Arc<Semaphore>,
    controls: Arc<Mutex<HashMap<TransferId, Arc<TransferControls>>>>,
    requests: Arc<Mutex<HashMap<TransferId, TransferRequest>>>,
    events: mpsc::Sender<TransferEvent>,
    chunk_size: usize,
}

impl TransferManager {
    /// Creates a manager plus its bounded event stream. `max_concurrency`
    /// is clamped into `1..=MAX_CONCURRENCY_CEILING`.
    #[must_use]
    pub fn new(
        backend: Arc<dyn SftpBackend>,
        max_concurrency: usize,
        event_buffer: usize,
    ) -> (Self, mpsc::Receiver<TransferEvent>) {
        let max_concurrency = max_concurrency.clamp(1, crate::MAX_CONCURRENCY_CEILING);
        let (events, receiver) = mpsc::channel(event_buffer.max(8));
        let manager = Self {
            backend,
            max_concurrency,
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            controls: Arc::new(Mutex::new(HashMap::new())),
            requests: Arc::new(Mutex::new(HashMap::new())),
            events,
            chunk_size: DEFAULT_CHUNK_SIZE,
        };
        (manager, receiver)
    }

    #[must_use]
    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// Enqueues a transfer and starts it once a concurrency slot is free.
    pub async fn enqueue(
        &self,
        request: TransferRequest,
        retries: u32,
    ) -> Result<TransferId, SftpError> {
        let (id, _completion) = self.enqueue_internal(request, retries, false).await?;
        Ok(id)
    }

    /// Enqueues a transfer and waits for its terminal outcome. This is used
    /// by higher-level atomic workflows (for example clipboard staging) that
    /// must not expose the destination path before the upload is complete.
    pub async fn enqueue_and_wait(
        &self,
        request: TransferRequest,
        retries: u32,
    ) -> Result<TransferId, SftpError> {
        let (id, completion) = self.enqueue_internal(request, retries, true).await?;
        completion
            .ok_or_else(|| SftpError::Backend("transfer completion channel missing".into()))?
            .await
            .map_err(|_| SftpError::Backend("transfer worker stopped unexpectedly".into()))??;
        Ok(id)
    }

    async fn enqueue_internal(
        &self,
        request: TransferRequest,
        retries: u32,
        wait_for_completion: bool,
    ) -> Result<(TransferId, Option<oneshot::Receiver<Result<(), SftpError>>>), SftpError> {
        crate::validate_request(&request)?;
        let id = TransferId::new();
        let controls = Arc::new(TransferControls::new(retries));
        {
            let mut guard = self.controls.lock().map_err(lock_error)?;
            guard.insert(id, Arc::clone(&controls));
        }
        {
            let mut guard = self.requests.lock().map_err(lock_error)?;
            guard.insert(id, request.clone());
        }
        self.emit_state(id, TransferStatus::Queued).await;
        let (completion_tx, completion_rx) = if wait_for_completion {
            let (tx, rx) = oneshot::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        self.spawn_worker(id, request, controls, completion_tx);
        Ok((id, completion_rx))
    }

    /// Pauses a transfer; it resumes exactly where it stopped.
    pub fn pause(&self, id: TransferId) -> Result<(), SftpError> {
        let controls = self.controls_for(id)?;
        controls.paused.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn resume(&self, id: TransferId) -> Result<(), SftpError> {
        let controls = self.controls_for(id)?;
        controls.paused.store(false, Ordering::SeqCst);
        controls.resume_notify.notify_waiters();
        Ok(())
    }

    pub fn cancel(&self, id: TransferId) -> Result<(), SftpError> {
        let controls = self.controls_for(id)?;
        controls.cancelled.store(true, Ordering::SeqCst);
        controls.resume_notify.notify_waiters();
        Ok(())
    }

    /// Re-runs a failed/cancelled transfer with the same id and controls
    /// (idempotent: safe to call twice).
    pub async fn retry(&self, id: TransferId) -> Result<(), SftpError> {
        let request = {
            let guard = self.requests.lock().map_err(lock_error)?;
            guard.get(&id).cloned().ok_or(SftpError::UnknownTransfer)?
        };
        let controls = self.controls_for(id)?;
        // Wait for a previous worker (possibly still unwinding after a
        // failure) to fully exit before restarting, so two workers never
        // race on the same .part file.
        for _ in 0..200 {
            if !controls.running.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if controls.running.load(Ordering::SeqCst) {
            return Err(SftpError::Backend("transfer worker did not stop".into()));
        }
        controls.cancelled.store(false, Ordering::SeqCst);
        controls.paused.store(false, Ordering::SeqCst);
        controls
            .retries_left
            .store(controls.max_retries, Ordering::SeqCst);
        controls.transferred.store(0, Ordering::SeqCst);
        self.emit_state(id, TransferStatus::Queued).await;
        self.spawn_worker(id, request, controls, None);
        Ok(())
    }

    /// Current transferred bytes for an id (0 when unknown).
    #[must_use]
    pub fn transferred_bytes(&self, id: TransferId) -> u64 {
        self.controls_for(id)
            .map(|controls| controls.transferred.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    fn spawn_worker(
        &self,
        id: TransferId,
        request: TransferRequest,
        controls: Arc<TransferControls>,
        completion: Option<oneshot::Sender<Result<(), SftpError>>>,
    ) {
        let backend = Arc::clone(&self.backend);
        let semaphore = Arc::clone(&self.semaphore);
        let events = self.events.clone();
        let chunk_size = self.chunk_size;
        let controls_map = Arc::clone(&self.controls);
        let requests_map = Arc::clone(&self.requests);
        let generation = controls.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let reaper_controls = Arc::clone(&controls);
        tokio::spawn(async move {
            let permit = semaphore.acquire().await;
            let outcome = run_transfer(backend, id, request, controls, events, chunk_size).await;
            if let Some(completion) = completion {
                let _ = completion.send(outcome);
            }
            // Release the concurrency slot before the reap grace window,
            // otherwise finished transfers would block queued ones.
            drop(permit);
            // Reap terminal entries after a grace window: finished or
            // cancelled transfers must not accumulate in the maps forever,
            // but short retention keeps progress queries and the UI's
            // terminal-state display working right after completion.
            tokio::time::sleep(REAP_DELAY).await;
            let current_generation = reaper_controls.generation.load(Ordering::SeqCst);
            if current_generation == generation && !reaper_controls.running.load(Ordering::SeqCst) {
                if let Ok(mut guard) = controls_map.lock() {
                    guard.remove(&id);
                }
                if let Ok(mut guard) = requests_map.lock() {
                    guard.remove(&id);
                }
            }
        });
    }

    fn controls_for(&self, id: TransferId) -> Result<Arc<TransferControls>, SftpError> {
        let guard = self.controls.lock().map_err(lock_error)?;
        guard.get(&id).cloned().ok_or(SftpError::UnknownTransfer)
    }

    async fn emit_state(&self, id: TransferId, status: TransferStatus) {
        let _ = self.events.send(TransferEvent::State { id, status }).await;
    }
}

fn lock_error<T>(_: std::sync::PoisonError<std::sync::MutexGuard<'_, T>>) -> SftpError {
    SftpError::Backend("transfer manager lock poisoned".into())
}

async fn run_transfer(
    backend: Arc<dyn SftpBackend>,
    id: TransferId,
    request: TransferRequest,
    controls: Arc<TransferControls>,
    events: mpsc::Sender<TransferEvent>,
    chunk_size: usize,
) -> Result<(), SftpError> {
    controls.running.store(true, Ordering::SeqCst);
    let outcome = attempt_transfer(
        backend.as_ref(),
        id,
        &request,
        &controls,
        &events,
        chunk_size,
    )
    .await;

    // Ensure the running flag is cleared on every exit path.
    struct RunningGuard {
        controls: Arc<TransferControls>,
    }
    impl Drop for RunningGuard {
        fn drop(&mut self) {
            self.controls.running.store(false, Ordering::SeqCst);
        }
    }
    let _running = RunningGuard {
        controls: Arc::clone(&controls),
    };

    let mut current = outcome;
    while let Err(error) = current {
        if matches!(error, SftpError::Cancelled) || controls.cancelled.load(Ordering::SeqCst) {
            let _ = events
                .send(TransferEvent::State {
                    id,
                    status: TransferStatus::Cancelled,
                })
                .await;
            return Err(SftpError::Cancelled);
        }
        let retries_left = controls.retries_left.load(Ordering::SeqCst);
        if retries_left == 0 {
            let _ = events
                .send(TransferEvent::Failed {
                    id,
                    message: error.to_string(),
                })
                .await;
            let _ = events
                .send(TransferEvent::State {
                    id,
                    status: TransferStatus::Failed,
                })
                .await;
            return Err(error);
        }
        controls
            .retries_left
            .store(retries_left - 1, Ordering::SeqCst);
        let _ = events
            .send(TransferEvent::State {
                id,
                status: TransferStatus::Retrying,
            })
            .await;
        current = attempt_transfer(
            backend.as_ref(),
            id,
            &request,
            &controls,
            &events,
            chunk_size,
        )
        .await;
    }
    let _ = events
        .send(TransferEvent::State {
            id,
            status: TransferStatus::Completed,
        })
        .await;
    Ok(())
}

async fn attempt_transfer(
    backend: &dyn SftpBackend,
    id: TransferId,
    request: &TransferRequest,
    controls: &TransferControls,
    events: &mpsc::Sender<TransferEvent>,
    chunk_size: usize,
) -> Result<(), SftpError> {
    wait_while_paused(controls).await;
    if controls.cancelled.load(Ordering::SeqCst) {
        return Err(SftpError::Cancelled);
    }
    let _ = events
        .send(TransferEvent::State {
            id,
            status: TransferStatus::Transferring,
        })
        .await;
    match request.direction {
        TransferDirection::Upload => {
            upload(backend, id, request, controls, events, chunk_size).await
        }
        TransferDirection::Download => {
            download(backend, id, request, controls, events, chunk_size).await
        }
    }
}

async fn upload(
    backend: &dyn SftpBackend,
    id: TransferId,
    request: &TransferRequest,
    controls: &TransferControls,
    events: &mpsc::Sender<TransferEvent>,
    chunk_size: usize,
) -> Result<(), SftpError> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut local = tokio::fs::File::open(&request.local_path)
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                SftpError::SourceNotFound
            } else {
                SftpError::Backend(format!("local open: {error}"))
            }
        })?;
    let total = local
        .metadata()
        .await
        .map_err(|error| SftpError::Backend(format!("local metadata: {error}")))?
        .len();

    let remote_part = part_path(&request.remote_path);
    // Resume only when the partial file is consistent: it must not be
    // larger than the source. A stale/oversized .part is discarded so a
    // bad resume can never corrupt the destination.
    let resume_offset = if request.resume {
        let existing = existing_part_size(backend, &remote_part).await?;
        if existing > total {
            0
        } else {
            existing
        }
    } else {
        0
    };

    let mut writer = backend.open_write(&remote_part, resume_offset == 0).await?;
    if resume_offset > 0 {
        writer.seek(resume_offset).await?;
        local
            .seek(std::io::SeekFrom::Start(resume_offset))
            .await
            .map_err(|error| SftpError::Backend(format!("local seek: {error}")))?;
    }

    let mut transferred = resume_offset;
    controls.transferred.store(transferred, Ordering::SeqCst);
    let mut speed_window = SpeedWindow::new();
    let mut progress_throttle = ProgressThrottle::new();
    let mut buf = vec![0u8; chunk_size];
    loop {
        wait_while_paused(controls).await;
        if controls.cancelled.load(Ordering::SeqCst) {
            return Err(SftpError::Cancelled);
        }
        let n = local
            .read(&mut buf)
            .await
            .map_err(|error| SftpError::Backend(format!("local read: {error}")))?;
        if n == 0 {
            break;
        }
        writer.write(&buf[..n]).await.map_err(map_disk_error)?;
        transferred += n as u64;
        controls.transferred.store(transferred, Ordering::SeqCst);
        let speed_bps = speed_window.push(n as u64);
        let progress = TransferProgress {
            transferred,
            total: Some(total),
            speed_bps,
        };
        if progress_throttle.should_emit(progress) {
            let _ = events.send(TransferEvent::Progress { id, progress }).await;
        }
    }

    writer.flush().await?;
    writer.close().await?;
    backend
        .rename(&remote_part, &request.remote_path)
        .await
        .map_err(|error| match error {
            SftpError::Backend(message) if message.contains("No space left") => SftpError::DiskFull,
            other => other,
        })?;
    Ok(())
}

async fn download(
    backend: &dyn SftpBackend,
    id: TransferId,
    request: &TransferRequest,
    controls: &TransferControls,
    events: &mpsc::Sender<TransferEvent>,
    chunk_size: usize,
) -> Result<(), SftpError> {
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};
    let meta = backend
        .stat(&request.remote_path)
        .await?
        .ok_or(SftpError::SourceNotFound)?;
    if meta.is_dir {
        return Err(SftpError::Backend("remote path is a directory".into()));
    }
    let total = meta.size;

    let local_part = part_path(&request.local_path);
    // Resume only when the partial file is consistent: it must not be
    // larger than the remote source. A stale/oversized .part is
    // discarded so a bad resume can never corrupt the destination.
    let resume_offset = if request.resume {
        let existing = local_part_size(&local_part).await?;
        if existing > total {
            0
        } else {
            existing
        }
    } else {
        0
    };

    let mut reader = backend.open_read(&request.remote_path).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(resume_offset == 0)
        .open(&local_part)
        .await
        .map_err(|error| SftpError::Backend(format!("local create: {error}")))?;
    if resume_offset > 0 {
        file.seek(std::io::SeekFrom::Start(resume_offset))
            .await
            .map_err(|error| SftpError::Backend(format!("local seek: {error}")))?;
        // Seek the remote handle directly. Replaying a multi-gigabyte prefix
        // over SSH made resume needlessly slow and consumed the same network
        // bandwidth twice.
        reader.seek(resume_offset).await?;
    }

    let mut transferred = resume_offset;
    controls.transferred.store(transferred, Ordering::SeqCst);
    let mut speed_window = SpeedWindow::new();
    let mut progress_throttle = ProgressThrottle::new();
    let mut buf = vec![0u8; chunk_size];
    loop {
        wait_while_paused(controls).await;
        if controls.cancelled.load(Ordering::SeqCst) {
            return Err(SftpError::Cancelled);
        }
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::StorageFull
                || error.kind() == std::io::ErrorKind::WriteZero
            {
                SftpError::DiskFull
            } else {
                SftpError::Backend(format!("local write: {error}"))
            }
        })?;
        transferred += n as u64;
        controls.transferred.store(transferred, Ordering::SeqCst);
        let speed_bps = speed_window.push(n as u64);
        let progress = TransferProgress {
            transferred,
            total: Some(total),
            speed_bps,
        };
        if progress_throttle.should_emit(progress) {
            let _ = events.send(TransferEvent::Progress { id, progress }).await;
        }
    }

    file.flush()
        .await
        .map_err(|error| SftpError::Backend(format!("local flush: {error}")))?;
    file.sync_all()
        .await
        .map_err(|error| SftpError::Backend(format!("local sync: {error}")))?;
    drop(file);
    reader.close().await?;
    std::fs::rename(&local_part, &request.local_path)
        .map_err(|error| SftpError::Backend(format!("local rename: {error}")))?;
    Ok(())
}

/// Emits progress events at most once per interval or byte step.
struct ProgressThrottle {
    last_emit: Instant,
    last_bytes: u64,
}

impl ProgressThrottle {
    const MIN_INTERVAL: Duration = Duration::from_millis(200);
    const MIN_STEP: u64 = 1024 * 1024;

    fn new() -> Self {
        Self {
            last_emit: Instant::now(),
            last_bytes: 0,
        }
    }

    fn should_emit(&mut self, progress: TransferProgress) -> bool {
        let interval_passed = self.last_emit.elapsed() >= Self::MIN_INTERVAL;
        let step_passed = progress.transferred >= self.last_bytes + Self::MIN_STEP;
        // Keep progress bounded by time, not by byte volume. At high
        // throughput a 1 MiB step can otherwise generate hundreds of React
        // updates per second and compete with terminal rendering.
        if interval_passed
            && (step_passed
                || progress
                    .total
                    .is_some_and(|total| progress.transferred >= total))
        {
            self.last_emit = Instant::now();
            self.last_bytes = progress.transferred;
            true
        } else {
            false
        }
    }
}

/// 1-second sliding speed window.
struct SpeedWindow {
    started: Instant,
    bytes: u64,
    speed: u64,
}

impl SpeedWindow {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            bytes: 0,
            speed: 0,
        }
    }

    fn push(&mut self, bytes: u64) -> u64 {
        self.bytes += bytes;
        let elapsed = self.started.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.speed = (self.bytes as f64 / elapsed.as_secs_f64()) as u64;
            self.bytes = 0;
            self.started = Instant::now();
        }
        self.speed
    }
}

async fn existing_part_size(backend: &dyn SftpBackend, part: &str) -> Result<u64, SftpError> {
    Ok(backend.stat(part).await?.map(|meta| meta.size).unwrap_or(0))
}

async fn local_part_size(part: &str) -> Result<u64, SftpError> {
    match tokio::fs::metadata(part).await {
        Ok(meta) => Ok(meta.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(SftpError::Backend(format!("part metadata: {error}"))),
    }
}

async fn wait_while_paused(controls: &TransferControls) {
    while controls.paused.load(Ordering::SeqCst) {
        controls.resume_notify.notified().await;
    }
}

fn map_disk_error(error: SftpError) -> SftpError {
    match error {
        SftpError::Backend(message) if message.contains("No space left") => SftpError::DiskFull,
        other => other,
    }
}
