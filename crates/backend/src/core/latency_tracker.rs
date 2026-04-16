//! In-memory rolling latency tracker for agent → backend messages.
//!
//! Gives operators visibility into end-to-end delay (agent-emitted
//! timestamp → backend dequeue time) without depending on Prometheus /
//! Grafana. Samples live in a sliding window per message type; stats
//! (count, p50, p95, max) are computed on demand and are exposed via
//! a JSON endpoint plus a periodic summary log.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Sliding window length — samples older than this are dropped on insert.
const WINDOW: Duration = Duration::from_secs(300); // 5 minutes
/// Hard cap on per-bucket samples to keep memory bounded under load.
const MAX_SAMPLES_PER_BUCKET: usize = 4096;

#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencyBucketSnapshot {
    pub message_type: String,
    pub count: usize,
    pub p50_ms: i64,
    pub p95_ms: i64,
    pub max_ms: i64,
    pub window_seconds: u64,
}

pub struct LatencyTracker {
    buckets: Mutex<HashMap<&'static str, VecDeque<(Instant, i64)>>>,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Record a sample. `delta_ms` may be negative (clock skew); we store
    /// the raw value so the snapshot keeps the sign for diagnosis.
    pub fn record(&self, message_type: &'static str, delta_ms: i64) {
        let now = Instant::now();
        let mut buckets = match self.buckets.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = buckets.entry(message_type).or_default();
        entry.push_back((now, delta_ms));

        // Evict samples that fall out of the window.
        while let Some(&(t, _)) = entry.front() {
            if now.duration_since(t) > WINDOW {
                entry.pop_front();
            } else {
                break;
            }
        }
        // Bound memory even if the window is briefly flooded.
        while entry.len() > MAX_SAMPLES_PER_BUCKET {
            entry.pop_front();
        }
    }

    /// Snapshot current stats for all buckets.
    pub fn snapshot(&self) -> Vec<LatencyBucketSnapshot> {
        let now = Instant::now();
        let buckets = match self.buckets.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut out = Vec::with_capacity(buckets.len());
        for (kind, samples) in buckets.iter() {
            let live: Vec<i64> = samples
                .iter()
                .filter(|(t, _)| now.duration_since(*t) <= WINDOW)
                .map(|(_, v)| *v)
                .collect();
            if live.is_empty() {
                continue;
            }
            let mut sorted = live.clone();
            sorted.sort_unstable();
            let p50 = percentile(&sorted, 50);
            let p95 = percentile(&sorted, 95);
            let max = *sorted.last().unwrap_or(&0);
            out.push(LatencyBucketSnapshot {
                message_type: (*kind).to_string(),
                count: live.len(),
                p50_ms: p50,
                p95_ms: p95,
                max_ms: max,
                window_seconds: WINDOW.as_secs(),
            });
        }
        out.sort_by(|a, b| a.message_type.cmp(&b.message_type));
        out
    }
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn percentile(sorted_asc: &[i64], p: u8) -> i64 {
    if sorted_asc.is_empty() {
        return 0;
    }
    let idx = ((sorted_asc.len().saturating_sub(1)) * p as usize) / 100;
    sorted_asc[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_handles_empty() {
        assert_eq!(percentile(&[], 50), 0);
        assert_eq!(percentile(&[], 95), 0);
    }

    #[test]
    fn percentile_basic() {
        let xs: Vec<i64> = (1..=100).collect();
        assert_eq!(percentile(&xs, 50), 50);
        assert_eq!(percentile(&xs, 95), 95);
        assert_eq!(percentile(&xs, 100), 100);
    }

    #[test]
    fn snapshot_returns_per_bucket_stats() {
        let t = LatencyTracker::new();
        for v in 0..100 {
            t.record("heartbeat", v);
            t.record("check_result", v * 2);
        }
        let snap = t.snapshot();
        assert_eq!(snap.len(), 2);

        let hb = snap.iter().find(|s| s.message_type == "heartbeat").unwrap();
        assert_eq!(hb.count, 100);
        assert_eq!(hb.max_ms, 99);
        assert!(hb.p95_ms > hb.p50_ms);

        let cr = snap
            .iter()
            .find(|s| s.message_type == "check_result")
            .unwrap();
        assert_eq!(cr.max_ms, 198);
    }

    #[test]
    fn snapshot_sorted_alphabetically() {
        let t = LatencyTracker::new();
        t.record("zzz", 1);
        t.record("aaa", 2);
        let snap = t.snapshot();
        assert_eq!(snap[0].message_type, "aaa");
        assert_eq!(snap[1].message_type, "zzz");
    }
}
