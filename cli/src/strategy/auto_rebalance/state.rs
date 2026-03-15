//! JSON state persistence for the auto-rebalance daemon.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ── Data models ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateData {
    /// Chain name this state belongs to (e.g. "base", "ethereum").
    /// TVL history is cleared when the chain changes, preventing
    /// cross-chain TVL comparisons from triggering false emergencies.
    #[serde(default)]
    pub chain: Option<String>,
    #[serde(default)]
    pub current_position: Option<PositionState>,
    #[serde(default)]
    pub rebalance_history: Vec<RebalanceRecord>,
    #[serde(default)]
    pub tvl_history: HashMap<String, Vec<TvlEntryState>>,
    #[serde(default)]
    pub last_check_timestamp: u64,
    #[serde(default)]
    pub config: DaemonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionState {
    pub protocol: String,
    pub balance_usd: f64,
    pub apy: f64,
    pub entered_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceRecord {
    pub timestamp: u64,
    pub from_protocol: String,
    pub to_protocol: String,
    pub amount: f64,
    pub gas: f64,
    pub spread: f64,
    pub tx_hashes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TvlEntryState {
    pub tvl_usd: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_interval")]
    pub interval_minutes: u64,
    #[serde(default = "default_min_spread")]
    pub min_spread: f64,
    #[serde(default = "default_max_break_even_days")]
    pub max_break_even_days: u64,
}

fn default_interval() -> u64 {
    60
}
fn default_min_spread() -> f64 {
    0.5
}
fn default_max_break_even_days() -> u64 {
    7
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_minutes: default_interval(),
            min_spread: default_min_spread(),
            max_break_even_days: default_max_break_even_days(),
        }
    }
}

// ── Persistence ─────────────────────────────────────────────────────

impl StateData {
    /// Default file path: `~/.plugin-store/auto-rebalance-state.json`.
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("cannot determine home directory")?;
        Ok(home.join(".plugin-store").join("auto-rebalance-state.json"))
    }

    /// Load from the default path, returning defaults if the file is missing.
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_from(&path)
    }

    /// Load from an arbitrary path, returning defaults if the file is missing.
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(path).context("failed to read state file")?;
        let state: StateData = serde_json::from_str(&data).context("failed to parse state")?;
        Ok(state)
    }

    /// Save to the default path.
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        self.save_to(&path)
    }

    /// Save to an arbitrary path, creating parent directories as needed.
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("failed to create state directory")?;
        }
        let data = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &data)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Append a rebalance record to history.
    pub fn add_rebalance(&mut self, record: RebalanceRecord) {
        self.rebalance_history.push(record);
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        let mut state = StateData::default();
        state.last_check_timestamp = 1234567890;
        state.current_position = Some(PositionState {
            protocol: "aave".into(),
            balance_usd: 1000.0,
            apy: 5.25,
            entered_at: 1234567800,
        });
        state.config.interval_minutes = 30;

        state.save_to(&path).unwrap();

        let loaded = StateData::load_from(&path).unwrap();
        assert_eq!(loaded.last_check_timestamp, 1234567890);
        let pos = loaded.current_position.unwrap();
        assert_eq!(pos.protocol, "aave");
        assert!((pos.balance_usd - 1000.0).abs() < f64::EPSILON);
        assert!((pos.apy - 5.25).abs() < f64::EPSILON);
        assert_eq!(pos.entered_at, 1234567800);
        assert_eq!(loaded.config.interval_minutes, 30);
    }

    #[test]
    fn state_load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let state = StateData::load_from(&path).unwrap();
        assert!(state.current_position.is_none());
        assert!(state.rebalance_history.is_empty());
        assert!(state.tvl_history.is_empty());
        assert_eq!(state.last_check_timestamp, 0);
        assert_eq!(state.config.interval_minutes, 60);
        assert!((state.config.min_spread - 0.5).abs() < f64::EPSILON);
        assert_eq!(state.config.max_break_even_days, 7);
    }

    #[test]
    fn state_add_rebalance_record() {
        let mut state = StateData::default();
        assert!(state.rebalance_history.is_empty());

        state.add_rebalance(RebalanceRecord {
            timestamp: 1000,
            from_protocol: "aave".into(),
            to_protocol: "compound".into(),
            amount: 500.0,
            gas: 2.5,
            spread: 0.8,
            tx_hashes: vec!["0xabc".into(), "0xdef".into()],
        });

        assert_eq!(state.rebalance_history.len(), 1);
        let rec = &state.rebalance_history[0];
        assert_eq!(rec.from_protocol, "aave");
        assert_eq!(rec.to_protocol, "compound");
        assert!((rec.amount - 500.0).abs() < f64::EPSILON);
        assert_eq!(rec.tx_hashes.len(), 2);
    }
}
