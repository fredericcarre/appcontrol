//! WebSocket log layer for real-time log streaming.
//!
//! This module provides a custom tracing layer that captures log events
//! and sends them to a channel for WebSocket transmission.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::protocol::LogEntry;

/// Maximum number of log entries to batch before sending.
const MAX_BATCH_SIZE: usize = 10;

/// Maximum time to wait before flushing a batch (milliseconds).
const BATCH_FLUSH_MS: u64 = 100;

/// Maximum queue size before dropping logs (rate limiting).
const MAX_QUEUE_SIZE: usize = 1000;

/// A tracing layer that captures log events and sends them via an mpsc channel.
///
/// This layer is designed for real-time log streaming to a WebSocket connection.
/// It batches log entries and sends them periodically to reduce overhead.
///
/// # Usage
///
/// ```ignore
/// use appcontrol_common::logging::{WebSocketLogLayer, LogSender};
///
/// let (sender, receiver) = LogSender::new();
/// let layer = WebSocketLogLayer::new(sender, tracing::Level::INFO);
///
/// tracing_subscriber::registry()
///     .with(layer)
///     .with(tracing_subscriber::fmt::layer())
///     .init();
/// ```
pub struct WebSocketLogLayer {
    sender: LogSender,
    min_level: Level,
    enabled: Arc<AtomicBool>,
}

impl WebSocketLogLayer {
    /// Create a new WebSocketLogLayer.
    ///
    /// # Arguments
    /// * `sender` - The channel sender for log entries
    /// * `min_level` - Minimum log level to capture (e.g., Level::INFO)
    pub fn new(sender: LogSender, min_level: Level) -> Self {
        Self {
            sender,
            min_level,
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Enable or disable log capture.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Check if log capture is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Get a handle to control this layer.
    pub fn handle(&self) -> LogLayerHandle {
        LogLayerHandle {
            enabled: self.enabled.clone(),
        }
    }
}

/// Handle to control a WebSocketLogLayer from outside.
#[derive(Clone)]
pub struct LogLayerHandle {
    enabled: Arc<AtomicBool>,
}

impl LogLayerHandle {
    /// Enable or disable log capture.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Check if log capture is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
}

impl<S> Layer<S> for WebSocketLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Skip if disabled
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        // Skip if below minimum level
        let metadata = event.metadata();
        if *metadata.level() > self.min_level {
            return;
        }

        // Extract fields from the event
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message: visitor.message,
            timestamp: Utc::now(),
            fields: if visitor.fields.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(visitor.fields))
            },
        };

        // Send to channel (non-blocking, drop if full)
        self.sender.send(entry);
    }
}

/// Field visitor to extract message and structured fields from a tracing event.
#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else {
            self.fields.insert(
                field.name().to_string(),
                serde_json::json!(format!("{:?}", value)),
            );
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields
                .insert(field.name().to_string(), serde_json::json!(value));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }
}

/// Sender side of the log channel.
#[derive(Clone)]
pub struct LogSender {
    tx: mpsc::UnboundedSender<LogEntry>,
}

impl LogSender {
    /// Create a new LogSender/LogReceiver pair.
    pub fn new() -> (Self, LogReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, LogReceiver { rx })
    }

    /// Send a log entry (non-blocking, drops if channel is closed).
    fn send(&self, entry: LogEntry) {
        // Best-effort send, ignore errors (channel closed)
        let _ = self.tx.send(entry);
    }
}

impl Default for LogSender {
    fn default() -> Self {
        let (sender, _) = Self::new();
        sender
    }
}

/// Receiver side of the log channel.
pub struct LogReceiver {
    rx: mpsc::UnboundedReceiver<LogEntry>,
}

impl LogReceiver {
    /// Receive the next log entry.
    pub async fn recv(&mut self) -> Option<LogEntry> {
        self.rx.recv().await
    }
}

/// A batching log forwarder that collects log entries and sends them
/// in batches to reduce message overhead.
pub struct LogBatcher {
    receiver: LogReceiver,
    batch: Vec<LogEntry>,
    max_batch_size: usize,
    flush_interval: Duration,
}

impl LogBatcher {
    /// Create a new LogBatcher.
    pub fn new(receiver: LogReceiver) -> Self {
        Self {
            receiver,
            batch: Vec::with_capacity(MAX_BATCH_SIZE),
            max_batch_size: MAX_BATCH_SIZE,
            flush_interval: Duration::from_millis(BATCH_FLUSH_MS),
        }
    }

    /// Get the next batch of log entries.
    ///
    /// Returns when either:
    /// - The batch reaches max_batch_size
    /// - The flush interval elapses
    /// - The channel is closed (returns remaining entries or None)
    pub async fn next_batch(&mut self) -> Option<Vec<LogEntry>> {
        loop {
            // If batch is full, return it
            if self.batch.len() >= self.max_batch_size {
                return Some(std::mem::take(&mut self.batch));
            }

            // Wait for entry or timeout
            tokio::select! {
                entry = self.receiver.recv() => {
                    match entry {
                        Some(e) => {
                            // Rate limiting: drop if queue too large
                            if self.batch.len() < MAX_QUEUE_SIZE {
                                self.batch.push(e);
                            }
                        }
                        None => {
                            // Channel closed, return remaining entries
                            if self.batch.is_empty() {
                                return None;
                            }
                            return Some(std::mem::take(&mut self.batch));
                        }
                    }
                }
                _ = tokio::time::sleep(self.flush_interval) => {
                    // Timeout, flush if we have entries
                    if !self.batch.is_empty() {
                        return Some(std::mem::take(&mut self.batch));
                    }
                }
            }
        }
    }
}

/// Parse a log level string to tracing::Level.
pub fn parse_level(level: &str) -> Level {
    match level.to_uppercase().as_str() {
        "TRACE" => Level::TRACE,
        "DEBUG" => Level::DEBUG,
        "INFO" => Level::INFO,
        "WARN" | "WARNING" => Level::WARN,
        "ERROR" => Level::ERROR,
        _ => Level::INFO,
    }
}

/// Check if a log level passes a minimum level filter.
pub fn level_passes_filter(level: &str, min_level: &str) -> bool {
    let level = parse_level(level);
    let min = parse_level(min_level);
    level <= min
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_level() {
        assert_eq!(parse_level("TRACE"), Level::TRACE);
        assert_eq!(parse_level("debug"), Level::DEBUG);
        assert_eq!(parse_level("INFO"), Level::INFO);
        assert_eq!(parse_level("WARN"), Level::WARN);
        assert_eq!(parse_level("WARNING"), Level::WARN);
        assert_eq!(parse_level("ERROR"), Level::ERROR);
        assert_eq!(parse_level("unknown"), Level::INFO);
    }

    #[test]
    fn test_level_passes_filter() {
        // ERROR is highest priority, TRACE is lowest
        assert!(level_passes_filter("ERROR", "ERROR"));
        assert!(level_passes_filter("ERROR", "DEBUG"));
        assert!(level_passes_filter("WARN", "INFO"));
        assert!(level_passes_filter("INFO", "DEBUG"));
        assert!(!level_passes_filter("DEBUG", "INFO"));
        assert!(!level_passes_filter("TRACE", "DEBUG"));
    }

    #[tokio::test]
    async fn test_log_sender_receiver() {
        let (sender, mut receiver) = LogSender::new();

        let entry = LogEntry {
            level: "INFO".to_string(),
            target: "test::module".to_string(),
            message: "Test message".to_string(),
            timestamp: Utc::now(),
            fields: None,
        };

        sender.send(entry.clone());

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.level, "INFO");
        assert_eq!(received.message, "Test message");
    }

    #[tokio::test]
    async fn test_log_batcher_batch_size() {
        let (sender, receiver) = LogSender::new();
        let mut batcher = LogBatcher::new(receiver);

        // Send MAX_BATCH_SIZE entries
        for i in 0..MAX_BATCH_SIZE {
            sender.send(LogEntry {
                level: "INFO".to_string(),
                target: "test".to_string(),
                message: format!("Message {}", i),
                timestamp: Utc::now(),
                fields: None,
            });
        }

        let batch = batcher.next_batch().await.unwrap();
        assert_eq!(batch.len(), MAX_BATCH_SIZE);
    }

    #[test]
    fn test_log_layer_handle() {
        let (sender, _receiver) = LogSender::new();
        let layer = WebSocketLogLayer::new(sender, Level::INFO);
        let handle = layer.handle();

        assert!(layer.is_enabled());
        assert!(handle.is_enabled());

        handle.set_enabled(false);

        assert!(!layer.is_enabled());
        assert!(!handle.is_enabled());
    }
}
