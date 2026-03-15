//! Grid trading engine — pure functions, no I/O.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::config::GridConfig;

// ── Constants (ETH/USDC on Base) ─────────────────────────────────────

pub const ETH_ADDR: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
pub const USDC_ADDR: &str = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
pub const CHAIN_ID: &str = "8453";
pub const BASE_RPC: &str = "https://mainnet.base.org";
pub const USDC_DECIMALS: u8 = 6;

pub const GRID_LEVELS: u32 = 6;
pub const MAX_TRADE_PCT: f64 = 0.12;
pub const MIN_TRADE_USD: f64 = 5.0;
pub const GAS_RESERVE_ETH: f64 = 0.003;
pub const SLIPPAGE_PCT: &str = "1";
pub const EMA_PERIOD: usize = 20;
pub const VOLATILITY_MULTIPLIER: f64 = 2.5;
pub const STEP_MIN_PCT: f64 = 0.008;
pub const STEP_MAX_PCT: f64 = 0.060;
pub const STEP_FLOOR: f64 = 5.0;
pub const VOL_RECALIBRATE_RATIO: f64 = 0.3;
pub const MAX_CONSECUTIVE_ERRORS: u32 = 5;
pub const MAX_SAME_DIR_TRADES: usize = 3;
pub const COOLDOWN_AFTER_ERRORS: u64 = 3600;
pub const QUIET_INTERVAL: u64 = 3600;
pub const MIN_TRADE_INTERVAL: u64 = 1800;
pub const GRID_RECALIBRATE_HOURS: f64 = 12.0;
pub const TICK_INTERVAL_SECS: u64 = 60;
pub const POSITION_MAX_PCT: f64 = 65.0;
pub const POSITION_MIN_PCT: f64 = 35.0;

// ── Data Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grid {
    pub center: f64,
    pub step: f64,
    pub levels: u32,
    pub range: (f64, f64),
    pub vol_pct: f64,
}

pub struct TradeAmount {
    pub amount_usd: f64,
    pub amount_token: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub time: String,
    pub direction: String,
    pub price: f64,
    pub amount_usd: f64,
    pub tx: Option<String>,
    pub grid_from: u32,
    pub grid_to: u32,
    pub success: bool,
    pub failure_reason: Option<String>,
}

// ── Grid Calculation ─────────────────────────────────────────────────

/// Calculate exponential moving average.
pub fn calc_ema(prices: &[f64], period: usize) -> f64 {
    if prices.is_empty() {
        return 0.0;
    }
    if prices.len() <= period {
        return prices.iter().sum::<f64>() / prices.len() as f64;
    }
    let k = 2.0 / (period as f64 + 1.0);
    let mut ema = prices[..period].iter().sum::<f64>() / period as f64;
    for &p in &prices[period..] {
        ema = p * k + ema * (1.0 - k);
    }
    ema
}

/// Calculate standard deviation of prices.
pub fn calc_volatility(prices: &[f64]) -> f64 {
    if prices.len() < 2 {
        return 0.0;
    }
    let mean = prices.iter().sum::<f64>() / prices.len() as f64;
    let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / prices.len() as f64;
    variance.sqrt()
}

/// Compute dynamic grid from current price and history (uses compile-time defaults).
pub fn calc_dynamic_grid(current_price: f64, price_history: &[f64]) -> Grid {
    calc_dynamic_grid_cfg(current_price, price_history, &GridConfig::default())
}

/// Compute dynamic grid using user config.
pub fn calc_dynamic_grid_cfg(current_price: f64, price_history: &[f64], cfg: &GridConfig) -> Grid {
    let center = if price_history.len() >= cfg.ema_period {
        calc_ema(price_history, cfg.ema_period)
    } else if !price_history.is_empty() {
        price_history.iter().sum::<f64>() / price_history.len() as f64
    } else {
        current_price
    };

    let vol = calc_volatility(price_history);
    let mean = if price_history.is_empty() {
        current_price
    } else {
        price_history.iter().sum::<f64>() / price_history.len() as f64
    };
    let vol_pct = if mean > 0.0 { vol / mean * 100.0 } else { 0.0 };

    let half_levels = cfg.grid_levels as f64 / 2.0;
    let mut step = if half_levels > 0.0 {
        (cfg.volatility_multiplier * vol) / half_levels
    } else {
        current_price * 0.02
    };

    let min_step = current_price * cfg.step_min_pct;
    let max_step = current_price * cfg.step_max_pct;
    step = step.clamp(min_step, max_step);
    step = step.max(cfg.step_floor);

    let low = center - step * half_levels;
    let high = center + step * half_levels;

    Grid {
        center,
        step,
        levels: cfg.grid_levels,
        range: (low, high),
        vol_pct,
    }
}

/// Map a price to a grid level (0..=GRID_LEVELS).
pub fn price_to_level(price: f64, grid: &Grid) -> u32 {
    if grid.step <= 0.0 {
        return 0;
    }
    let raw = ((price - grid.range.0) / grid.step).floor() as i64;
    raw.clamp(0, grid.levels as i64) as u32
}

// ── Trade Sizing ─────────────────────────────────────────────────────

/// Calculate trade amount (uses compile-time defaults).
pub fn calc_trade_amount(
    direction: &str,
    eth_bal: f64,
    usdc_bal: f64,
    price: f64,
) -> Option<TradeAmount> {
    calc_trade_amount_cfg(direction, eth_bal, usdc_bal, price, &GridConfig::default())
}

/// Calculate trade amount using user config.
pub fn calc_trade_amount_cfg(
    direction: &str,
    eth_bal: f64,
    usdc_bal: f64,
    price: f64,
    cfg: &GridConfig,
) -> Option<TradeAmount> {
    let total_usd = eth_bal * price + usdc_bal;
    let max_trade_usd = total_usd * cfg.max_trade_pct;

    let amount_usd = match direction {
        "BUY" => (usdc_bal * 0.95).min(max_trade_usd),
        "SELL" => {
            let sellable_eth = (eth_bal - cfg.gas_reserve_eth).max(0.0);
            (sellable_eth * price).min(max_trade_usd)
        }
        _ => return None,
    };

    if amount_usd < cfg.min_trade_usd {
        return None;
    }

    let amount_token = match direction {
        "BUY" => amount_usd,
        "SELL" => amount_usd / price,
        _ => return None,
    };

    Some(TradeAmount {
        amount_usd,
        amount_token,
    })
}

// ── Risk Controls ────────────────────────────────────────────────────

/// Check cooldown (uses compile-time defaults).
pub fn check_cooldown(
    last_trade_times: &HashMap<String, String>,
    direction: &str,
) -> Option<String> {
    check_cooldown_cfg(last_trade_times, direction, &GridConfig::default())
}

/// Check cooldown using user config.
pub fn check_cooldown_cfg(
    last_trade_times: &HashMap<String, String>,
    direction: &str,
    cfg: &GridConfig,
) -> Option<String> {
    let last_time_str = last_trade_times.get(direction)?;
    let last_time = chrono::DateTime::parse_from_rfc3339(last_time_str).ok()?;
    let elapsed = chrono::Utc::now()
        .signed_duration_since(last_time)
        .num_seconds() as u64;
    if elapsed < cfg.min_trade_interval {
        Some(format!(
            "Cooldown: {}s remaining for {} trade",
            cfg.min_trade_interval - elapsed,
            direction
        ))
    } else {
        None
    }
}

/// Check position limits (uses compile-time defaults).
pub fn check_position_limit(direction: &str, eth_pct: f64) -> Option<String> {
    check_position_limit_cfg(direction, eth_pct, &GridConfig::default())
}

/// Check position limits using user config.
pub fn check_position_limit_cfg(direction: &str, eth_pct: f64, cfg: &GridConfig) -> Option<String> {
    match direction {
        "BUY" if eth_pct > cfg.position_max_pct => Some(format!(
            "Position limit: ETH% {:.1}% > {:.0}% max, blocking BUY",
            eth_pct, cfg.position_max_pct
        )),
        "SELL" if eth_pct < cfg.position_min_pct => Some(format!(
            "Position limit: ETH% {:.1}% < {:.0}% min, blocking SELL",
            eth_pct, cfg.position_min_pct
        )),
        _ => None,
    }
}

/// Check repeat boundary: prevent trading the same crossing twice in a row.
pub fn check_repeat_boundary(
    last_trade: Option<&Trade>,
    direction: &str,
    from_level: u32,
    to_level: u32,
) -> Option<String> {
    let lt = last_trade?;
    if lt.success
        && lt.direction == direction
        && lt.grid_from == from_level
        && lt.grid_to == to_level
    {
        Some(format!(
            "Repeat guard: same crossing {}->{} {} as last trade",
            from_level, to_level, direction
        ))
    } else {
        None
    }
}

/// Check consecutive limit (uses compile-time defaults).
pub fn check_consecutive_limit(recent_trades: &[Trade], direction: &str) -> Option<String> {
    check_consecutive_limit_cfg(recent_trades, direction, &GridConfig::default())
}

/// Check consecutive limit using user config.
pub fn check_consecutive_limit_cfg(
    recent_trades: &[Trade],
    direction: &str,
    cfg: &GridConfig,
) -> Option<String> {
    let consecutive = recent_trades
        .iter()
        .rev()
        .take_while(|t| t.direction == direction && t.success)
        .count();
    if consecutive >= cfg.max_same_dir_trades {
        Some(format!(
            "Consecutive limit: {} consecutive {} trades (max {})",
            consecutive, direction, cfg.max_same_dir_trades
        ))
    } else {
        None
    }
}

// ── Grid Recalibration ───────────────────────────────────────────────

/// Check if grid needs recalibration (uses compile-time defaults).
pub fn needs_recalibration(
    grid: &Grid,
    grid_set_at: &str,
    current_price: f64,
    price_history: &[f64],
) -> bool {
    needs_recalibration_cfg(
        grid,
        grid_set_at,
        current_price,
        price_history,
        &GridConfig::default(),
    )
}

/// Check if grid needs recalibration using user config.
pub fn needs_recalibration_cfg(
    grid: &Grid,
    grid_set_at: &str,
    current_price: f64,
    price_history: &[f64],
    cfg: &GridConfig,
) -> bool {
    // 1. Price exits grid range by > 1 step
    if current_price < grid.range.0 - grid.step || current_price > grid.range.1 + grid.step {
        return true;
    }

    // 2. Grid age exceeds limit
    if let Ok(set_at) = chrono::DateTime::parse_from_rfc3339(grid_set_at) {
        let hours = chrono::Utc::now()
            .signed_duration_since(set_at)
            .num_minutes() as f64
            / 60.0;
        if hours > cfg.grid_recalibrate_hours {
            return true;
        }
    }

    // 3. Volatility changed significantly
    if !price_history.is_empty() {
        let current_vol = calc_volatility(price_history);
        let mean = price_history.iter().sum::<f64>() / price_history.len() as f64;
        let current_vol_pct = if mean > 0.0 {
            current_vol / mean * 100.0
        } else {
            0.0
        };
        if grid.vol_pct > 0.0 {
            let ratio = (current_vol_pct - grid.vol_pct).abs() / grid.vol_pct;
            if ratio > VOL_RECALIBRATE_RATIO {
                return true;
            }
        }
    }

    false
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calc_ema_empty() {
        assert_eq!(calc_ema(&[], 20), 0.0);
    }

    #[test]
    fn test_calc_ema_short() {
        let prices = vec![100.0, 110.0, 105.0];
        let ema = calc_ema(&prices, 20);
        assert!((ema - 105.0).abs() < 0.01);
    }

    #[test]
    fn test_calc_ema_exact() {
        let prices: Vec<f64> = (0..30).map(|i| 2000.0 + i as f64).collect();
        let ema = calc_ema(&prices, 20);
        assert!(ema > 2015.0 && ema < 2030.0, "ema={}", ema);
    }

    #[test]
    fn test_calc_volatility() {
        let prices = vec![100.0, 100.0, 100.0];
        assert_eq!(calc_volatility(&prices), 0.0);

        let prices2 = vec![100.0, 110.0, 90.0, 100.0];
        assert!(calc_volatility(&prices2) > 0.0);
    }

    #[test]
    fn test_calc_dynamic_grid() {
        let prices: Vec<f64> = (0..30).map(|i| 2000.0 + (i as f64 * 2.0)).collect();
        let grid = calc_dynamic_grid(2050.0, &prices);
        assert!(grid.center > 1990.0 && grid.center < 2060.0);
        assert!(grid.step >= STEP_FLOOR);
        assert!(grid.range.0 < grid.center);
        assert!(grid.range.1 > grid.center);
        assert_eq!(grid.levels, GRID_LEVELS);
    }

    #[test]
    fn test_price_to_level() {
        let grid = Grid {
            center: 2000.0,
            step: 50.0,
            levels: 6,
            range: (1850.0, 2150.0),
            vol_pct: 2.0,
        };
        assert_eq!(price_to_level(1850.0, &grid), 0);
        assert_eq!(price_to_level(1900.0, &grid), 1);
        assert_eq!(price_to_level(2000.0, &grid), 3);
        assert_eq!(price_to_level(2150.0, &grid), 6);
        assert_eq!(price_to_level(1700.0, &grid), 0);
        assert_eq!(price_to_level(2300.0, &grid), 6);
    }

    #[test]
    fn test_calc_trade_amount_buy() {
        let result = calc_trade_amount("BUY", 0.5, 1000.0, 2000.0);
        assert!(result.is_some());
        let ta = result.unwrap();
        let total = 0.5 * 2000.0 + 1000.0;
        assert!(ta.amount_usd <= total * MAX_TRADE_PCT + 0.01);
        assert!(ta.amount_usd <= 1000.0);
    }

    #[test]
    fn test_calc_trade_amount_sell() {
        let result = calc_trade_amount("SELL", 0.5, 1000.0, 2000.0);
        assert!(result.is_some());
        let ta = result.unwrap();
        assert!(ta.amount_token <= 0.5 - GAS_RESERVE_ETH + 0.001);
    }

    #[test]
    fn test_calc_trade_amount_below_minimum() {
        let result = calc_trade_amount("BUY", 0.001, 2.0, 2000.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_position_limit() {
        assert!(check_position_limit("BUY", 70.0).is_some());
        assert!(check_position_limit("BUY", 50.0).is_none());
        assert!(check_position_limit("SELL", 30.0).is_some());
        assert!(check_position_limit("SELL", 50.0).is_none());
    }

    #[test]
    fn test_check_repeat_boundary() {
        let trade = Trade {
            time: "2026-01-01T00:00:00Z".to_string(),
            direction: "BUY".to_string(),
            price: 2000.0,
            amount_usd: 100.0,
            tx: None,
            grid_from: 3,
            grid_to: 2,
            success: true,
            failure_reason: None,
        };
        assert!(check_repeat_boundary(Some(&trade), "BUY", 3, 2).is_some());
        assert!(check_repeat_boundary(Some(&trade), "BUY", 4, 3).is_none());
        assert!(check_repeat_boundary(None, "BUY", 3, 2).is_none());
    }

    #[test]
    fn test_check_consecutive_limit() {
        let trades: Vec<Trade> = (0..3)
            .map(|_| Trade {
                time: "2026-01-01T00:00:00Z".to_string(),
                direction: "BUY".to_string(),
                price: 2000.0,
                amount_usd: 100.0,
                tx: None,
                grid_from: 3,
                grid_to: 2,
                success: true,
                failure_reason: None,
            })
            .collect();
        assert!(check_consecutive_limit(&trades, "BUY").is_some());
        assert!(check_consecutive_limit(&trades, "SELL").is_none());
    }
}
