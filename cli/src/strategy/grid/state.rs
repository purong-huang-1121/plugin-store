//! Grid bot state management — persisted at ~/.plugin-store/grid_state.json.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config::GridConfig;
use super::engine::{Grid, Trade};

const STATE_VERSION: u32 = 4;
const MAX_PRICE_HISTORY: usize = 288;
const MAX_TRADES: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceSnapshot {
    pub eth: f64,
    pub usdc: f64,
    pub total_usd: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub total_trades: u32,
    pub successful_trades: u32,
    pub failed_trades: u32,
    pub total_buy_usd: f64,
    pub total_sell_usd: f64,
    pub grid_profit: f64,
    pub initial_portfolio_usd: Option<f64>,
    pub total_deposits_usd: f64,
    pub deposit_history: Vec<DepositRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRecord {
    pub time: String,
    pub amount_usd: f64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorState {
    pub consecutive_errors: u32,
    pub last_error_time: Option<String>,
    pub last_error_msg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedTrade {
    pub direction: String,
    pub amount_usd: f64,
    pub price: f64,
    pub grid_from: u32,
    pub grid_to: u32,
    pub reason: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridState {
    pub version: u32,
    pub grid: Option<Grid>,
    pub grid_set_at: Option<String>,
    pub current_level: Option<u32>,
    pub price_history: Vec<f64>,
    pub trades: Vec<Trade>,
    pub last_balances: Option<BalanceSnapshot>,
    pub stats: Stats,
    pub errors: ErrorState,
    pub last_trade_times: HashMap<String, String>,
    pub last_quiet_report: Option<String>,
    pub last_failed_trade: Option<FailedTrade>,
    #[serde(default)]
    pub last_blocked_reason: Option<String>,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            grid: None,
            grid_set_at: None,
            current_level: None,
            price_history: Vec::new(),
            trades: Vec::new(),
            last_balances: None,
            stats: Stats::default(),
            errors: ErrorState::default(),
            last_trade_times: HashMap::new(),
            last_quiet_report: None,
            last_failed_trade: None,
            last_blocked_reason: None,
        }
    }
}

impl GridState {
    /// Load state from ~/.plugin-store/grid_state.json.
    pub fn load() -> Result<Self> {
        let path = Self::state_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let state: Self = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(state)
    }

    /// Save state atomically: write to .tmp file, then rename.
    pub fn save(&self) -> Result<()> {
        let path = Self::state_path();
        let dir = path.parent().context("no parent dir")?;
        std::fs::create_dir_all(dir)?;

        let tmp_path = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp_path, &data)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, &path).with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    /// Delete state file.
    pub fn reset() -> Result<()> {
        let path = Self::state_path();
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn state_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".plugin-store")
            .join("grid_state.json")
    }

    pub fn pid_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".plugin-store")
            .join("grid_bot.pid")
    }

    /// Add a price to history, trimming to MAX_PRICE_HISTORY.
    pub fn push_price(&mut self, price: f64) {
        self.price_history.push(price);
        if self.price_history.len() > MAX_PRICE_HISTORY {
            let excess = self.price_history.len() - MAX_PRICE_HISTORY;
            self.price_history.drain(..excess);
        }
    }

    /// Add a trade, trimming to MAX_TRADES.
    pub fn push_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
        if self.trades.len() > MAX_TRADES {
            let excess = self.trades.len() - MAX_TRADES;
            self.trades.drain(..excess);
        }
    }

    /// Record a deposit or withdrawal.
    pub fn record_deposit(&mut self, amount_usd: f64, note: Option<String>) {
        self.stats.total_deposits_usd += amount_usd;
        self.stats.deposit_history.push(DepositRecord {
            time: chrono::Utc::now().to_rfc3339(),
            amount_usd,
            note,
        });
    }

    /// Check circuit breaker (uses compile-time defaults).
    pub fn check_circuit_breaker(&self) -> Option<String> {
        self.check_circuit_breaker_cfg(&GridConfig::default())
    }

    /// Check circuit breaker using user config.
    pub fn check_circuit_breaker_cfg(&self, cfg: &GridConfig) -> Option<String> {
        if self.errors.consecutive_errors >= cfg.max_consecutive_errors {
            if let Some(ref last_err_time) = self.errors.last_error_time {
                if let Ok(t) = chrono::DateTime::parse_from_rfc3339(last_err_time) {
                    let elapsed = chrono::Utc::now().signed_duration_since(t).num_seconds() as u64;
                    if elapsed < cfg.cooldown_after_errors {
                        return Some(format!(
                            "Circuit breaker: {} consecutive errors, cooldown {}s remaining",
                            self.errors.consecutive_errors,
                            cfg.cooldown_after_errors - elapsed
                        ));
                    }
                }
            }
        }
        None
    }

    /// Detect balance change (deposit/withdrawal) by comparing to last snapshot.
    pub fn detect_balance_change(&self, eth_bal: f64, usdc_bal: f64, price: f64) -> Option<f64> {
        let last = self.last_balances.as_ref()?;
        let current_total = eth_bal * price + usdc_bal;
        let diff = current_total - last.total_usd;
        if diff.abs() > 50.0 {
            Some(diff)
        } else {
            None
        }
    }
}
