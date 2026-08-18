#![forbid(unsafe_code)]

//! Bounded terminal output aggregation (GOAL §7.2): bytes are buffered in
//! Rust and crossed as batches of 4–32 KiB or at 8–16 ms intervals, never
//! one IPC call per byte.

use std::time::{Duration, Instant};

/// Default maximum bytes per emitted batch (upper bound of the 4–32 KiB range).
pub const DEFAULT_BYTE_THRESHOLD: usize = 32 * 1024;
/// Default maximum interval between emitted batches (upper bound of 8–16 ms).
pub const DEFAULT_MAX_INTERVAL: Duration = Duration::from_millis(16);

pub struct OutputAggregator {
    buffer: Vec<u8>,
    byte_threshold: usize,
    max_interval: Duration,
    first_byte_at: Option<Instant>,
}

impl OutputAggregator {
    #[must_use]
    pub fn new(byte_threshold: usize, max_interval: Duration) -> Self {
        Self {
            buffer: Vec::with_capacity(byte_threshold.min(64 * 1024)),
            byte_threshold,
            max_interval,
            first_byte_at: None,
        }
    }

    #[must_use]
    pub fn default_style() -> Self {
        Self::new(DEFAULT_BYTE_THRESHOLD, DEFAULT_MAX_INTERVAL)
    }

    /// Buffers `data`; returns a ready-to-send batch when the byte threshold
    /// was reached, otherwise `None`.
    pub fn push(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        if data.is_empty() {
            return None;
        }
        self.buffer.extend_from_slice(data);
        if self.first_byte_at.is_none() {
            self.first_byte_at = Some(Instant::now());
        }
        if self.buffer.len() >= self.byte_threshold {
            self.take_batch()
        } else {
            None
        }
    }

    /// Emits a batch when the buffered bytes reached the max interval.
    /// Callers should invoke this on an 8–16 ms ticker.
    pub fn tick(&mut self) -> Option<Vec<u8>> {
        match self.first_byte_at {
            Some(started) if started.elapsed() >= self.max_interval => self.take_batch(),
            _ => None,
        }
    }

    /// Forces emission of any buffered bytes.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        self.take_batch()
    }

    #[must_use]
    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    fn take_batch(&mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            return None;
        }
        self.first_byte_at = None;
        Some(std::mem::take(&mut self.buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_batch_at_byte_threshold() {
        let mut aggregator = OutputAggregator::new(64, Duration::from_secs(60));
        assert!(aggregator.push(&[0u8; 32]).is_none());
        let batch = aggregator.push(&[0u8; 32]);
        assert_eq!(batch, Some(vec![0u8; 64]));
        assert!(aggregator.is_empty());
    }

    #[test]
    fn emits_batch_after_interval() {
        let mut aggregator = OutputAggregator::new(1024, Duration::from_millis(5));
        assert!(aggregator.push(b"hello").is_none());
        std::thread::sleep(Duration::from_millis(20));
        let batch = aggregator.tick();
        assert_eq!(batch.as_deref(), Some(b"hello".as_slice()));
    }

    #[test]
    fn flush_emits_remaining_bytes() {
        let mut aggregator = OutputAggregator::new(1024, Duration::from_secs(60));
        assert!(aggregator.push(b"a").is_none());
        assert_eq!(aggregator.flush().as_deref(), Some(b"a".as_slice()));
        assert!(aggregator.flush().is_none(), "second flush is empty");
    }

    #[test]
    fn empty_push_is_ignored() {
        let mut aggregator = OutputAggregator::new(64, Duration::from_secs(60));
        assert!(aggregator.push(b"").is_none());
        assert!(aggregator.is_empty());
    }
}
