//! Retransmission tracker with deduplication.
//!
//! Uses sequence IDs (monotonic per-agent) to:
//! 1. Detect duplicate messages (agent retransmits after reconnect)
//! 2. Detect gaps (missing messages requiring retransmit request)

use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

/// Maximum number of sequence IDs to track per agent for dedup.
const MAX_SEEN_PER_AGENT: usize = 1000;

/// Tracks received sequence IDs per agent for deduplication.
pub struct DeduplicationTracker {
    /// agent_id -> ring buffer of recently seen sequence_ids
    seen: HashMap<Uuid, VecDeque<u64>>,
    /// agent_id -> highest contiguous sequence_id received
    high_watermark: HashMap<Uuid, u64>,
}

impl DeduplicationTracker {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
            high_watermark: HashMap::new(),
        }
    }

    /// Check if a message is a duplicate. Returns true if the message should be processed.
    /// Returns false if it's a duplicate that should be dropped.
    pub fn check_and_record(&mut self, agent_id: Uuid, sequence_id: u64) -> bool {
        let seen = self.seen.entry(agent_id).or_default();

        // Check if already seen
        if seen.contains(&sequence_id) {
            return false;
        }

        // Record it
        seen.push_back(sequence_id);
        if seen.len() > MAX_SEEN_PER_AGENT {
            seen.pop_front();
        }

        // Update high watermark
        let hwm = self.high_watermark.entry(agent_id).or_insert(0);
        if sequence_id == *hwm + 1 {
            *hwm = sequence_id;
            // Advance past any consecutive IDs we already have
            while seen.contains(&(*hwm + 1)) {
                *hwm += 1;
            }
        }

        true
    }

    /// Get the highest contiguous sequence_id for an agent.
    pub fn high_watermark(&self, agent_id: Uuid) -> u64 {
        self.high_watermark.get(&agent_id).copied().unwrap_or(0)
    }

    /// Detect gaps: return missing sequence IDs between watermark and max seen.
    pub fn detect_gaps(&self, agent_id: Uuid) -> Vec<u64> {
        let hwm = self.high_watermark(agent_id);
        let seen = match self.seen.get(&agent_id) {
            Some(s) => s,
            None => return vec![],
        };

        let max_seen = seen.iter().copied().max().unwrap_or(hwm);
        let mut gaps = Vec::new();
        for seq in (hwm + 1)..max_seen {
            if !seen.contains(&seq) {
                gaps.push(seq);
            }
        }
        gaps
    }

    /// Remove tracking for a disconnected agent.
    pub fn remove_agent(&mut self, agent_id: Uuid) {
        self.seen.remove(&agent_id);
        self.high_watermark.remove(&agent_id);
    }
}

impl Default for DeduplicationTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_message_accepted() {
        let mut tracker = DeduplicationTracker::new();
        let agent = Uuid::new_v4();
        assert!(tracker.check_and_record(agent, 1));
        assert!(tracker.check_and_record(agent, 2));
    }

    #[test]
    fn test_duplicate_rejected() {
        let mut tracker = DeduplicationTracker::new();
        let agent = Uuid::new_v4();
        assert!(tracker.check_and_record(agent, 1));
        assert!(!tracker.check_and_record(agent, 1)); // duplicate
    }

    #[test]
    fn test_high_watermark_advances() {
        let mut tracker = DeduplicationTracker::new();
        let agent = Uuid::new_v4();
        tracker.check_and_record(agent, 1);
        tracker.check_and_record(agent, 2);
        tracker.check_and_record(agent, 3);
        assert_eq!(tracker.high_watermark(agent), 3);
    }

    #[test]
    fn test_out_of_order_watermark() {
        let mut tracker = DeduplicationTracker::new();
        let agent = Uuid::new_v4();
        tracker.check_and_record(agent, 1);
        tracker.check_and_record(agent, 3); // gap at 2
        assert_eq!(tracker.high_watermark(agent), 1);
        tracker.check_and_record(agent, 2); // fill gap
        assert_eq!(tracker.high_watermark(agent), 3);
    }

    #[test]
    fn test_detect_gaps() {
        let mut tracker = DeduplicationTracker::new();
        let agent = Uuid::new_v4();
        tracker.check_and_record(agent, 1);
        tracker.check_and_record(agent, 3);
        tracker.check_and_record(agent, 5);
        let gaps = tracker.detect_gaps(agent);
        assert_eq!(gaps, vec![2, 4]);
    }

    #[test]
    fn test_remove_agent() {
        let mut tracker = DeduplicationTracker::new();
        let agent = Uuid::new_v4();
        tracker.check_and_record(agent, 1);
        tracker.remove_agent(agent);
        assert_eq!(tracker.high_watermark(agent), 0);
    }

    #[test]
    fn test_independent_agents() {
        let mut tracker = DeduplicationTracker::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();
        tracker.check_and_record(a1, 1);
        tracker.check_and_record(a2, 1);
        assert_eq!(tracker.high_watermark(a1), 1);
        assert_eq!(tracker.high_watermark(a2), 1);
    }
}
