//! Auto-rebalance user-configurable parameters — persisted at ~/.plugin-store/auto_rebalance_config.json.
//! Log file at ~/.plugin-store/auto_rebalance.log.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Base directory for all auto-rebalance files.
fn base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".plugin-store")
}

/// User-tunable auto-rebalance parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRebalanceConfig {
    // ── Daemon ──
    /// Check interval in seconds (default 3600 = 1 hour)
    pub interval_secs: u64,
    /// Minimum APY spread (%) to trigger rebalance
    pub min_spread: f64,
    /// Maximum break-even days (gas cost / daily yield improvement)
    pub max_break_even: u64,

    // ── Chain ──
    /// Chain name: "base" or "ethereum"
    pub chain: String,

    // ── Engine ──
    /// Maximum gas cost in USD to allow rebalance
    pub max_gas_cost_usd: f64,
    /// Minimum seconds between rebalances (cooldown)
    pub min_rebalance_interval_secs: u64,
    /// TVL drop % that triggers a non-blocking alert (default 20.0)
    #[serde(default = "default_tvl_alert_threshold")]
    pub tvl_alert_threshold: f64,

    // ── Telegram ──
    /// Telegram Bot API token (optional, can also use TELEGRAM_BOT_TOKEN env)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_token: Option<String>,
    /// Telegram chat ID (optional, can also use TELEGRAM_CHAT_ID env)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_chat: Option<String>,
}

fn default_tvl_alert_threshold() -> f64 {
    20.0
}

impl Default for AutoRebalanceConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            min_spread: 0.5,
            max_break_even: 7,
            chain: "base".to_string(),
            max_gas_cost_usd: 0.50,
            min_rebalance_interval_secs: 86400,
            tvl_alert_threshold: 20.0,
            telegram_token: None,
            telegram_chat: None,
        }
    }
}

impl AutoRebalanceConfig {
    pub fn config_path() -> PathBuf {
        base_dir().join("auto_rebalance_config.json")
    }

    pub fn log_path() -> PathBuf {
        base_dir().join("auto_rebalance.log")
    }

    /// Load config from file, falling back to defaults if missing.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Self = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    /// Save config to file.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let dir = path.parent().context("no parent dir")?;
        std::fs::create_dir_all(dir)?;
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, &data)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

/// Append a log line to the log file.
pub fn log_to_file(msg: &str) {
    let path = AutoRebalanceConfig::log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let timestamp = chrono::Utc::now().to_rfc3339();
    let line = format!("[{}] {}\n", timestamp, msg);
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}
