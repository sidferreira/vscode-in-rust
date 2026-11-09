//! The "recently used" commands of the Command Palette.
//!
//! upstream: src/vs/platform/quickinput/browser/commandsQuickAccess.ts @ 50e1350fc78d (`CommandsHistory`)
//!
//! An LRU map from command id to a counter that grows with every use, limited to
//! `workbench.commandPalette.history` entries (50 by default, 0 disables it). Reading with
//! [`CommandsHistory::peek`] does not change the order. Not persisted yet (no storage service).

use std::collections::HashMap;

/// `CommandsHistory.DEFAULT_COMMANDS_HISTORY_LENGTH`.
pub const DEFAULT_LIMIT: usize = 50;

#[derive(Clone, Debug)]
pub struct CommandsHistory {
    limit: usize,
    counter: u64,
    used: HashMap<String, u64>,
}

impl Default for CommandsHistory {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT)
    }
}

impl CommandsHistory {
    pub fn new(limit: usize) -> Self {
        Self { limit, counter: 1, used: HashMap::new() }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// `workbench.commandPalette.history` changed: shrinking drops the least recently used.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.trim();
    }

    /// A command was run.
    pub fn push(&mut self, id: &str) {
        if self.limit == 0 {
            return;
        }
        self.used.insert(id.to_string(), self.counter);
        self.counter += 1;
        self.trim();
    }

    /// The counter of `id` (higher is more recent), without touching the order.
    pub fn peek(&self, id: &str) -> Option<u64> {
        self.used.get(id).copied()
    }

    pub fn remove(&mut self, id: &str) {
        self.used.remove(id);
    }

    pub fn clear(&mut self) {
        self.used.clear();
        self.counter = 1;
    }

    pub fn len(&self) -> usize {
        self.used.len()
    }

    pub fn is_empty(&self) -> bool {
        self.used.is_empty()
    }

    /// Ids, most recently used first.
    pub fn recent(&self) -> Vec<&str> {
        let mut v: Vec<(&str, u64)> = self.used.iter().map(|(k, c)| (k.as_str(), *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.into_iter().map(|(k, _)| k).collect()
    }

    fn trim(&mut self) {
        while self.used.len() > self.limit {
            let Some(oldest) = self.used.iter().min_by_key(|(_, c)| **c).map(|(k, _)| k.clone()) else { return };
            self.used.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_first_and_reuse_moves_to_front() {
        let mut h = CommandsHistory::default();
        h.push("a");
        h.push("b");
        h.push("c");
        assert_eq!(h.recent(), ["c", "b", "a"]);
        h.push("a");
        assert_eq!(h.recent(), ["a", "c", "b"]);
        assert!(h.peek("a") > h.peek("c"));
        assert_eq!(h.peek("zzz"), None);
    }

    #[test]
    fn limit_evicts_the_oldest() {
        let mut h = CommandsHistory::new(2);
        h.push("a");
        h.push("b");
        h.push("c");
        assert_eq!(h.recent(), ["c", "b"]);
        h.set_limit(1);
        assert_eq!(h.recent(), ["c"]);
    }

    #[test]
    fn zero_disables_the_history() {
        let mut h = CommandsHistory::new(0);
        h.push("a");
        assert!(h.is_empty());
        let mut h = CommandsHistory::new(5);
        h.push("a");
        h.set_limit(0);
        assert!(h.is_empty());
    }

    #[test]
    fn remove_and_clear() {
        let mut h = CommandsHistory::default();
        h.push("a");
        h.push("b");
        h.remove("a");
        assert_eq!(h.recent(), ["b"]);
        h.clear();
        assert!(h.is_empty());
        h.push("x");
        assert_eq!(h.peek("x"), Some(1), "the counter restarts");
    }
}
