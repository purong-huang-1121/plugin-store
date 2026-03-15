//! Grid bot user-configurable parameters — persisted at ~/.plugin-store/grid_config.json.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::engine;

/// User-tunable grid bot parameters. Loaded from config file with defaults from engine constants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridConfig {
    pub grid_levels: u32,
    pub tick_interval_secs: u64,
    pub max_trade_pct: f64,
    pub min_trade_usd: f64,
    pub slippage_pct: String,
    pub ema_period: usize,
    pub volatility_multiplier: f64,
    pub step_min_pct: f64,
    pub step_max_pct: f64,
    pub step_floor: f64,
    pub grid_recalibrate_hours: f64,
    pub min_trade_interval: u64,
    pub max_same_dir_trades: usize,
    pub position_max_pct: f64,
    pub position_min_pct: f64,
    pub gas_reserve_eth: f64,
    pub max_consecutive_errors: u32,
    pub cooldown_after_errors: u64,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            grid_levels: engine::GRID_LEVELS,
            tick_interval_secs: engine::TICK_INTERVAL_SECS,
            max_trade_pct: engine::MAX_TRADE_PCT,
            min_trade_usd: engine::MIN_TRADE_USD,
            slippage_pct: engine::SLIPPAGE_PCT.to_string(),
            ema_period: engine::EMA_PERIOD,
            volatility_multiplier: engine::VOLATILITY_MULTIPLIER,
            step_min_pct: engine::STEP_MIN_PCT,
            step_max_pct: engine::STEP_MAX_PCT,
            step_floor: engine::STEP_FLOOR,
            grid_recalibrate_hours: engine::GRID_RECALIBRATE_HOURS,
            min_trade_interval: engine::MIN_TRADE_INTERVAL,
            max_same_dir_trades: engine::MAX_SAME_DIR_TRADES,
            position_max_pct: engine::POSITION_MAX_PCT,
            position_min_pct: engine::POSITION_MIN_PCT,
            gas_reserve_eth: engine::GAS_RESERVE_ETH,
            max_consecutive_errors: engine::MAX_CONSECUTIVE_ERRORS,
            cooldown_after_errors: engine::COOLDOWN_AFTER_ERRORS,
        }
    }
}

impl GridConfig {
    pub fn config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("grid_config.json")
    }

    /// Load config from file, falling back to defaults for missing fields.
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
