//! Multi-JVM target management.
//!
//! Manages connections to multiple JVM instances, each with its own
//! snapshot and collector tasks. Allows switching the "active" target
//! in the TUI and comparing metrics across instances.

use crate::model::JvmSnapshot;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Unique identifier for a target JVM.
pub type TargetId = String;

/// Configuration for a single JVM target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub id: TargetId,
    pub name: String,
    pub actuator_url: Option<String>,
    pub jolokia_url: Option<String>,
    pub gc_log_path: Option<String>,
    pub tags: Vec<String>,  // e.g., ["production", "api-server", "region-us-east"]
}

/// Connection status for a target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected(String), // reason
    Error(String),
}

/// State for a single managed target.
#[derive(Debug, Clone)]
pub struct ManagedTarget {
    pub config: TargetConfig,
    pub status: ConnectionStatus,
    pub last_snapshot: Option<JvmSnapshot>,
    pub last_update: Option<DateTime<Utc>>,
    pub error_count: u64,
    pub snapshot_count: u64,
}

impl ManagedTarget {
    pub fn new(config: TargetConfig) -> Self {
        Self {
            config,
            status: ConnectionStatus::Connecting,
            last_snapshot: None,
            last_update: None,
            error_count: 0,
            snapshot_count: 0,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.status == ConnectionStatus::Connected
    }

    /// Staleness check — flag as disconnected if no update in 30s.
    pub fn check_staleness(&mut self) {
        if let Some(last) = self.last_update {
            let age = Utc::now().signed_duration_since(last);
            if age.num_seconds() > 30 && self.status == ConnectionStatus::Connected {
                self.status = ConnectionStatus::Disconnected("No data for 30s".to_string());
            }
        }
    }
}

/// Manages multiple JVM targets.
pub struct TargetManager {
    targets: HashMap<TargetId, ManagedTarget>,
    active_id: Option<TargetId>,
    target_order: Vec<TargetId>, // insertion order for consistent display
}

impl TargetManager {
    pub fn new() -> Self {
        Self {
            targets: HashMap::new(),
            active_id: None,
            target_order: Vec::new(),
        }
    }

    /// Add a target. Makes it active if it's the first one.
    pub fn add_target(&mut self, config: TargetConfig) {
        let id = config.id.clone();
        if !self.targets.contains_key(&id) {
            self.target_order.push(id.clone());
        }
        self.targets.insert(id.clone(), ManagedTarget::new(config));
        if self.active_id.is_none() {
            self.active_id = Some(id);
        }
    }

    /// Remove a target by ID.
    pub fn remove_target(&mut self, id: &str) {
        self.targets.remove(id);
        self.target_order.retain(|t| t != id);
        if self.active_id.as_deref() == Some(id) {
            self.active_id = self.target_order.first().cloned();
        }
    }

    /// Set the active target.
    pub fn set_active(&mut self, id: &str) {
        if self.targets.contains_key(id) {
            self.active_id = Some(id.to_string());
        }
    }

    /// Cycle to the next target.
    pub fn next_target(&mut self) {
        if self.target_order.len() <= 1 { return; }
        if let Some(ref current) = self.active_id {
            if let Some(pos) = self.target_order.iter().position(|t| t == current) {
                let next = (pos + 1) % self.target_order.len();
                self.active_id = Some(self.target_order[next].clone());
            }
        }
    }

    /// Get the active target.
    pub fn active(&self) -> Option<&ManagedTarget> {
        self.active_id.as_ref().and_then(|id| self.targets.get(id))
    }

    pub fn active_mut(&mut self) -> Option<&mut ManagedTarget> {
        if let Some(ref id) = self.active_id {
            self.targets.get_mut(id)
        } else {
            None
        }
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    /// Update snapshot for a specific target.
    pub fn update_snapshot(&mut self, id: &str, snap: JvmSnapshot) {
        if let Some(target) = self.targets.get_mut(id) {
            target.last_snapshot = Some(snap);
            target.last_update = Some(Utc::now());
            target.status = ConnectionStatus::Connected;
            target.snapshot_count += 1;
        }
    }

    /// Record an error for a target.
    pub fn record_error(&mut self, id: &str, error: String) {
        if let Some(target) = self.targets.get_mut(id) {
            target.error_count += 1;
            target.status = ConnectionStatus::Error(error);
        }
    }

    /// Get all targets in display order.
    pub fn all_targets(&self) -> Vec<&ManagedTarget> {
        self.target_order.iter()
            .filter_map(|id| self.targets.get(id))
            .collect()
    }

    /// Get target count.
    pub fn count(&self) -> usize {
        self.targets.len()
    }

    /// Compare a metric across all connected targets.
    pub fn compare_heap_usage(&self) -> Vec<(&str, f64)> {
        self.all_targets().iter()
            .filter_map(|t| {
                let snap = t.last_snapshot.as_ref()?;
                let pct = snap.heap.usage_pct()?;
                Some((t.config.name.as_str(), pct))
            })
            .collect()
    }

    pub fn compare_cpu_usage(&self) -> Vec<(&str, f64)> {
        self.all_targets().iter()
            .filter_map(|t| {
                let snap = t.last_snapshot.as_ref()?;
                Some((t.config.name.as_str(), snap.cpu.process_cpu_pct()))
            })
            .collect()
    }

    /// Check all targets for staleness.
    pub fn check_all_staleness(&mut self) {
        for target in self.targets.values_mut() {
            target.check_staleness();
        }
    }
}

impl Default for TargetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(id: &str, name: &str) -> TargetConfig {
        TargetConfig {
            id: id.to_string(),
            name: name.to_string(),
            actuator_url: Some(format!("http://{}:8080/actuator", id)),
            jolokia_url: None,
            gc_log_path: None,
            tags: vec![],
        }
    }

    #[test]
    fn add_and_switch_targets() {
        let mut mgr = TargetManager::new();
        mgr.add_target(test_config("prod-1", "Production API"));
        mgr.add_target(test_config("staging-1", "Staging API"));

        assert_eq!(mgr.count(), 2);
        assert_eq!(mgr.active_id(), Some("prod-1"));

        mgr.next_target();
        assert_eq!(mgr.active_id(), Some("staging-1"));

        mgr.next_target();
        assert_eq!(mgr.active_id(), Some("prod-1"));
    }

    #[test]
    fn remove_active_switches() {
        let mut mgr = TargetManager::new();
        mgr.add_target(test_config("a", "A"));
        mgr.add_target(test_config("b", "B"));
        mgr.set_active("a");
        mgr.remove_target("a");
        assert_eq!(mgr.active_id(), Some("b"));
    }

    #[test]
    fn compare_metrics() {
        let mut mgr = TargetManager::new();
        mgr.add_target(test_config("a", "Service A"));
        mgr.add_target(test_config("b", "Service B"));

        let mut snap_a = JvmSnapshot::default();
        snap_a.heap.used = 256 * 1024 * 1024;
        snap_a.heap.max = 512 * 1024 * 1024;
        mgr.update_snapshot("a", snap_a);

        let mut snap_b = JvmSnapshot::default();
        snap_b.heap.used = 400 * 1024 * 1024;
        snap_b.heap.max = 512 * 1024 * 1024;
        mgr.update_snapshot("b", snap_b);

        let comparison = mgr.compare_heap_usage();
        assert_eq!(comparison.len(), 2);
        assert!((comparison[0].1 - 50.0).abs() < 1.0);
        assert!((comparison[1].1 - 78.1).abs() < 1.0);
    }
}
