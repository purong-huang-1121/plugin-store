use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::notifier::{Notifier, NotifyLevel};
use crate::output;
use crate::strategy::grid::client::GridClient;
use crate::strategy::grid::config::GridConfig;
use crate::strategy::grid::engine;
use crate::strategy::grid::state::{BalanceSnapshot, FailedTrade, GridState};

#[derive(Subcommand)]
pub enum GridCommand {
    /// Execute one grid tick: fetch price, detect crossing, trade if needed
    Tick,
    /// Start the bot in foreground (tick interval configurable, default 60s)
    Start,
    /// Stop a running bot via PID file
    Stop,
    /// Show current grid state, position, and PnL overview
    Status,
    /// Detailed PnL report
    Report,
    /// Show trade history
    History {
        /// Number of trades to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Reset state (clear all data)
    Reset {
        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },
    /// Retry the last failed trade
    Retry,
    /// Market analysis (volatility, trend, grid utilization)
    Analyze,
    /// Record a manual deposit or withdrawal
    Deposit {
        /// Amount in USD (negative for withdrawal)
        #[arg(long)]
        amount: f64,
        /// Optional note
        #[arg(long)]
        note: Option<String>,
    },
    /// Show current bot configuration
    Config,
    /// Set a config parameter: plugin-store grid set --key tick_interval_secs --value 120
    Set {
        /// Parameter name (e.g. grid_levels, tick_interval_secs, max_trade_pct)
        #[arg(long)]
        key: String,
        /// New value
        #[arg(long)]
        value: String,
    },
}

pub async fn execute(cmd: GridCommand) -> Result<()> {
    match cmd {
        GridCommand::Tick => {
            let notifier = Notifier::from_env("\u{1f4ca} Grid Bot");
            cmd_tick(&notifier).await
        }
        GridCommand::Start => cmd_start().await,
        GridCommand::Stop => cmd_stop().await,
        GridCommand::Status => cmd_status().await,
        GridCommand::Report => cmd_report().await,
        GridCommand::History { limit } => cmd_history(limit).await,
        GridCommand::Reset { force } => cmd_reset(force).await,
        GridCommand::Retry => cmd_retry().await,
        GridCommand::Analyze => cmd_analyze().await,
        GridCommand::Deposit { amount, note } => cmd_deposit(amount, note).await,
        GridCommand::Config => cmd_config().await,
        GridCommand::Set { key, value } => cmd_set(&key, &value).await,
    }
}

// ── tick ──────────────────────────────────────────────────────────────

async fn cmd_tick(notifier: &Notifier) -> Result<()> {
    let cfg = GridConfig::load()?;
    let mut state = GridState::load()?;

    // 1. Circuit breaker check
    if let Some(reason) = state.check_circuit_breaker_cfg(&cfg) {
        notifier
            .notify(
                NotifyLevel::Error,
                &format!("Circuit breaker active: {reason}"),
            )
            .await;
        output::error(&reason);
        return Ok(());
    }

    // 2. Create client
    let client = GridClient::new()?;

    // 3. Fetch price
    let price = client.get_eth_price().await?;
    state.push_price(price);

    // 4. Get balances
    let (eth_bal, usdc_bal) = client.get_balances().await?;
    let total_usd = eth_bal * price + usdc_bal;
    let eth_pct = if total_usd > 0.0 {
        (eth_bal * price) / total_usd * 100.0
    } else {
        0.0
    };

    // 5. Detect deposits/withdrawals
    if let Some(diff) = state.detect_balance_change(eth_bal, usdc_bal, price) {
        state.record_deposit(diff, Some("Auto-detected balance change".to_string()));
    }

    // Update balance snapshot
    let now = chrono::Utc::now().to_rfc3339();
    state.last_balances = Some(BalanceSnapshot {
        eth: eth_bal,
        usdc: usdc_bal,
        total_usd,
        timestamp: now.clone(),
    });

    // Set initial portfolio if first tick
    if state.stats.initial_portfolio_usd.is_none() {
        state.stats.initial_portfolio_usd = Some(total_usd);
    }

    // 6. Grid calibration
    let need_calibration = match (&state.grid, &state.grid_set_at) {
        (Some(grid), Some(set_at)) => {
            engine::needs_recalibration_cfg(grid, set_at, price, &state.price_history, &cfg)
        }
        _ => true,
    };

    if need_calibration {
        let grid = engine::calc_dynamic_grid_cfg(price, &state.price_history, &cfg);
        let level = engine::price_to_level(price, &grid);
        state.grid = Some(grid.clone());
        state.grid_set_at = Some(now);
        state.current_level = Some(level);
        state.save()?;
        notifier
            .notify(
                NotifyLevel::Info,
                &format!(
                    "Grid recalibrated\nPrice: ${:.2}\nCenter: ${:.2} | Step: ${:.2}\nRange: ${:.2} — ${:.2}\nLevel: {}/{}",
                    price, grid.center, grid.step, grid.range.0, grid.range.1, level, grid.levels
                ),
            )
            .await;
        output::success(json!({
            "action": "grid_calibrated",
            "price": price,
            "grid": grid,
            "level": level,
            "balances": { "eth": eth_bal, "usdc": usdc_bal, "total_usd": total_usd },
        }));
        return Ok(());
    }

    // 7. Detect grid crossing
    let grid = state.grid.as_ref().context("grid not calibrated")?;
    let new_level = engine::price_to_level(price, grid);
    let current_level = state.current_level.unwrap_or(new_level);

    if new_level == current_level {
        state.last_blocked_reason = None;
        state.save()?;
        output::success(json!({
            "action": "no_crossing",
            "price": price,
            "level": new_level,
            "grid_range": grid.range,
            "eth_pct": format!("{:.1}", eth_pct),
        }));
        return Ok(());
    }

    // 8. Grid crossing detected!
    let direction = if new_level < current_level {
        "BUY"
    } else {
        "SELL"
    };

    // 9. Risk checks
    let risk_checks: Vec<Option<String>> = vec![
        engine::check_cooldown_cfg(&state.last_trade_times, direction, &cfg),
        engine::check_position_limit_cfg(direction, eth_pct, &cfg),
        engine::check_repeat_boundary(state.trades.last(), direction, current_level, new_level),
        engine::check_consecutive_limit_cfg(&state.trades, direction, &cfg),
    ];
    if let Some(reason) = risk_checks.into_iter().flatten().next() {
        // Only send TG notification if this is a NEW block reason (avoid spam)
        let is_new_reason = state.last_blocked_reason.as_ref() != Some(&reason);
        if is_new_reason {
            notifier
                .notify(
                    NotifyLevel::Warning,
                    &format!(
                        "Trade blocked: {reason}\nDirection: {direction} | Price: ${price:.2} | Level: {current_level}→{new_level}"
                    ),
                )
                .await;
        }
        state.last_blocked_reason = Some(reason.clone());
        state.save()?;
        output::success(json!({ "action": "blocked", "reason": reason, "price": price }));
        return Ok(());
    }

    // 10. Calculate trade amount
    let trade_amount =
        match engine::calc_trade_amount_cfg(direction, eth_bal, usdc_bal, price, &cfg) {
            Some(ta) => ta,
            None => {
                state.save()?;
                output::success(json!({
                    "action": "skipped",
                    "reason": "Trade amount below minimum",
                    "price": price,
                    "direction": direction,
                }));
                return Ok(());
            }
        };

    // 11. Execute swap
    let raw_amount = match direction {
        "BUY" => {
            // USDC amount in 6 decimals
            let units = trade_amount.amount_token * 1_000_000.0;
            if units < 0.0 || units > u64::MAX as f64 {
                bail!("USDC amount out of range: {}", trade_amount.amount_token);
            }
            alloy::primitives::U256::from(units as u64)
        }
        "SELL" => {
            // ETH amount in wei (18 decimals)
            let wei = trade_amount.amount_token * 1e18;
            if wei < 0.0 || wei > u128::MAX as f64 {
                bail!("ETH amount out of range: {}", trade_amount.amount_token);
            }
            alloy::primitives::U256::from(wei as u128)
        }
        _ => unreachable!(),
    };

    let swap_result = client.execute_swap(direction, raw_amount, price).await;

    match swap_result {
        Ok(ref sr) if sr.failure.is_none() => {
            // Success!
            let trade = engine::Trade {
                time: now.clone(),
                direction: direction.to_string(),
                price,
                amount_usd: trade_amount.amount_usd,
                tx: sr.tx_hash.clone(),
                grid_from: current_level,
                grid_to: new_level,
                success: true,
                failure_reason: None,
            };

            state.stats.total_trades += 1;
            state.stats.successful_trades += 1;
            match direction {
                "BUY" => state.stats.total_buy_usd += trade_amount.amount_usd,
                "SELL" => {
                    state.stats.total_sell_usd += trade_amount.amount_usd;
                    if state.stats.total_buy_usd > 0.0 {
                        state.stats.grid_profit += trade_amount.amount_usd * 0.01;
                    }
                }
                _ => {}
            }

            // Update level ONLY on success
            state.last_blocked_reason = None;
            state.current_level = Some(new_level);
            state
                .last_trade_times
                .insert(direction.to_string(), now.clone());
            state.push_trade(trade);
            state.errors.consecutive_errors = 0;
            state.last_failed_trade = None;
            state.save()?;

            let tx_short = sr
                .tx_hash
                .as_deref()
                .map(|h| format!("{}…{}", &h[..10], &h[h.len() - 6..]))
                .unwrap_or_else(|| "—".into());
            notifier
                .notify(
                    NotifyLevel::Success,
                    &format!(
                        "Trade executed\n{direction} ${:.2} @ ${price:.2}\nLevel: {current_level} → {new_level}\nTx: {tx_short}",
                        trade_amount.amount_usd
                    ),
                )
                .await;

            output::success(json!({
                "action": "trade_executed",
                "direction": direction,
                "price": price,
                "amount_usd": format!("{:.2}", trade_amount.amount_usd),
                "tx_hash": sr.tx_hash,
                "grid_from": current_level,
                "grid_to": new_level,
                "balances": { "eth": eth_bal, "usdc": usdc_bal },
            }));
        }
        Ok(ref sr) => {
            let failure = sr.failure.as_ref().context("swap failed but no failure details")?;
            record_failure(
                &mut state,
                direction,
                &trade_amount,
                price,
                current_level,
                new_level,
                &failure.reason,
                &now,
                sr.tx_hash.clone(),
            );
            state.save()?;
            notifier
                .notify(
                    NotifyLevel::Error,
                    &format!(
                        "Trade failed\n{direction} ${:.2} @ ${price:.2}\nReason: {}\nRetriable: {}",
                        trade_amount.amount_usd, failure.reason, failure.retriable
                    ),
                )
                .await;
            output::success(json!({
                "action": "trade_failed",
                "direction": direction,
                "reason": failure.reason,
                "detail": failure.detail,
                "retriable": failure.retriable,
                "tx_hash": sr.tx_hash,
            }));
        }
        Err(e) => {
            let err_msg = format!("{:#}", e);
            record_failure(
                &mut state,
                direction,
                &trade_amount,
                price,
                current_level,
                new_level,
                &err_msg,
                &now,
                None,
            );
            state.save()?;
            notifier
                .notify(
                    NotifyLevel::Error,
                    &format!(
                        "Trade failed\n{direction} ${:.2} @ ${price:.2}\nError: {err_msg}",
                        trade_amount.amount_usd
                    ),
                )
                .await;
            output::success(json!({
                "action": "trade_failed",
                "direction": direction,
                "reason": err_msg,
                "retriable": true,
            }));
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_failure(
    state: &mut GridState,
    direction: &str,
    trade_amount: &engine::TradeAmount,
    price: f64,
    from_level: u32,
    to_level: u32,
    reason: &str,
    now: &str,
    tx_hash: Option<String>,
) {
    state.stats.total_trades += 1;
    state.stats.failed_trades += 1;
    state.errors.consecutive_errors += 1;
    state.errors.last_error_time = Some(now.to_string());
    state.errors.last_error_msg = Some(reason.to_string());

    let trade = engine::Trade {
        time: now.to_string(),
        direction: direction.to_string(),
        price,
        amount_usd: trade_amount.amount_usd,
        tx: tx_hash,
        grid_from: from_level,
        grid_to: to_level,
        success: false,
        failure_reason: Some(reason.to_string()),
    };
    state.push_trade(trade);

    state.last_failed_trade = Some(FailedTrade {
        direction: direction.to_string(),
        amount_usd: trade_amount.amount_usd,
        price,
        grid_from: from_level,
        grid_to: to_level,
        reason: reason.to_string(),
        timestamp: now.to_string(),
    });
}

// ── start ─────────────────────────────────────────────────────────────

async fn cmd_start() -> Result<()> {
    let pid_path = GridState::pid_path();

    // Check if already running
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
        let pid: i32 = pid_str.trim().parse().unwrap_or(0);
        #[cfg(unix)]
        if pid > 0 && unsafe { libc::kill(pid, 0) } == 0 {
            bail!(
                "Grid bot already running (PID {}). Use 'grid stop' first.",
                pid
            );
        }
    }

    // Validate credentials before starting loop
    let _ = GridClient::new()?;

    // Write PID file
    let dir = pid_path.parent().context("no parent dir")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let cfg = GridConfig::load()?;
    let notifier = Notifier::from_env("\u{1f4ca} Grid Bot");

    eprintln!(
        "[{}] Grid bot started (PID {})",
        chrono::Utc::now().to_rfc3339(),
        std::process::id()
    );
    notifier
        .notify(
            NotifyLevel::Info,
            &format!(
                "Bot started (PID {})\nTick interval: {}s",
                std::process::id(),
                cfg.tick_interval_secs
            ),
        )
        .await;

    // Signal handler for graceful shutdown (SIGINT + SIGTERM)
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    // SIGINT (Ctrl+C)
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    });

    // SIGTERM (from `grid stop`)
    #[cfg(unix)]
    {
        let r2 = running.clone();
        tokio::spawn(async move {
            let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
            sig.recv().await;
            r2.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }

    // Main loop
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        if let Err(e) = cmd_tick(&notifier).await {
            eprintln!("[{}] Tick error: {:#}", chrono::Utc::now().to_rfc3339(), e);
        }
        // Sleep in small increments to check shutdown flag
        for _ in 0..(cfg.tick_interval_secs / 5) {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    // Cleanup — send notification before exiting
    let _ = std::fs::remove_file(&pid_path);
    eprintln!("[{}] Grid bot stopped", chrono::Utc::now().to_rfc3339());
    notifier.notify(NotifyLevel::Info, "Bot stopped").await;
    output::success(json!({ "message": "Grid bot stopped" }));
    Ok(())
}

// ── stop ──────────────────────────────────────────────────────────────

async fn cmd_stop() -> Result<()> {
    let pid_path = GridState::pid_path();
    if !pid_path.exists() {
        bail!("No running bot found (PID file missing)");
    }
    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    if pid == 0 {
        bail!("Invalid PID in file");
    }

    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid, libc::SIGTERM) };
        if result != 0 {
            let _ = std::fs::remove_file(&pid_path);
            bail!("Process {} not found (already stopped?)", pid);
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    output::success(json!({ "message": format!("Stopped grid bot (PID {})", pid) }));
    Ok(())
}

// ── status ────────────────────────────────────────────────────────────

async fn cmd_status() -> Result<()> {
    let state = GridState::load()?;

    let bot_running = {
        let pid_path = GridState::pid_path();
        if pid_path.exists() {
            let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
            let pid: i32 = pid_str.trim().parse().unwrap_or(0);
            #[cfg(unix)]
            {
                pid > 0 && unsafe { libc::kill(pid, 0) } == 0
            }
            #[cfg(not(unix))]
            {
                pid > 0
            }
        } else {
            false
        }
    };

    let last_trade = state.trades.last();
    let last_price = state.price_history.last().copied();

    let mut result = json!({
        "grid": state.grid,
        "current_level": state.current_level,
        "last_price": last_price,
        "balances": state.last_balances,
        "last_trade": last_trade,
        "bot_running": bot_running,
        "consecutive_errors": state.errors.consecutive_errors,
        "price_history_count": state.price_history.len(),
        "trade_count": state.trades.len(),
    });

    if let Some(initial) = state.stats.initial_portfolio_usd {
        if let Some(ref balances) = state.last_balances {
            let current = balances.total_usd;
            let deposits = state.stats.total_deposits_usd;
            let pnl = current - initial - deposits;
            result["pnl"] = json!({
                "total": format!("{:.2}", pnl),
                "grid_profit": format!("{:.2}", state.stats.grid_profit),
                "initial_portfolio_usd": format!("{:.2}", initial),
                "total_deposits_usd": format!("{:.2}", deposits),
            });
        }
    }

    if let (Some(ref bal), Some(price)) = (&state.last_balances, last_price) {
        if bal.total_usd > 0.0 {
            let eth_pct = (bal.eth * price) / bal.total_usd * 100.0;
            result["eth_pct"] = json!(format!("{:.1}", eth_pct));
        }
    }

    output::success(result);
    Ok(())
}

// ── report ────────────────────────────────────────────────────────────

async fn cmd_report() -> Result<()> {
    let state = GridState::load()?;
    let s = &state.stats;

    let success_rate = if s.total_trades > 0 {
        s.successful_trades as f64 / s.total_trades as f64 * 100.0
    } else {
        0.0
    };

    let buy_count = state
        .trades
        .iter()
        .filter(|t| t.direction == "BUY" && t.success)
        .count();
    let sell_count = state
        .trades
        .iter()
        .filter(|t| t.direction == "SELL" && t.success)
        .count();

    let grid_age_hours = state.grid_set_at.as_ref().and_then(|t| {
        chrono::DateTime::parse_from_rfc3339(t)
            .ok()
            .map(|dt| chrono::Utc::now().signed_duration_since(dt).num_minutes() as f64 / 60.0)
    });

    let mut result = json!({
        "total_trades": s.total_trades,
        "successful_trades": s.successful_trades,
        "failed_trades": s.failed_trades,
        "success_rate": format!("{:.1}%", success_rate),
        "buy_count": buy_count,
        "sell_count": sell_count,
        "total_buy_usd": format!("{:.2}", s.total_buy_usd),
        "total_sell_usd": format!("{:.2}", s.total_sell_usd),
        "grid_profit": format!("{:.2}", s.grid_profit),
        "total_deposits_usd": format!("{:.2}", s.total_deposits_usd),
        "deposit_history": s.deposit_history,
        "price_history_count": state.price_history.len(),
    });

    if let Some(initial) = s.initial_portfolio_usd {
        result["initial_portfolio_usd"] = json!(format!("{:.2}", initial));
        if let Some(ref bal) = state.last_balances {
            let pnl = bal.total_usd - initial - s.total_deposits_usd;
            result["current_portfolio_usd"] = json!(format!("{:.2}", bal.total_usd));
            result["total_pnl"] = json!(format!("{:.2}", pnl));
        }
    }

    if let Some(hours) = grid_age_hours {
        result["grid_age_hours"] = json!(format!("{:.1}", hours));
    }

    output::success(result);
    Ok(())
}

// ── history ───────────────────────────────────────────────────────────

async fn cmd_history(limit: usize) -> Result<()> {
    let state = GridState::load()?;
    let trades: Vec<_> = state.trades.iter().rev().take(limit).collect();
    output::success(json!({ "trades": trades, "total": state.trades.len() }));
    Ok(())
}

// ── reset ─────────────────────────────────────────────────────────────

async fn cmd_reset(force: bool) -> Result<()> {
    if !force {
        bail!("This will delete all grid state. Use --force to confirm.");
    }
    GridState::reset()?;
    output::success(json!({ "message": "Grid state reset successfully" }));
    Ok(())
}

// ── deposit ───────────────────────────────────────────────────────────

async fn cmd_deposit(amount: f64, note: Option<String>) -> Result<()> {
    let mut state = GridState::load()?;
    state.record_deposit(amount, note.clone());
    state.save()?;
    output::success(json!({
        "action": if amount >= 0.0 { "deposit" } else { "withdrawal" },
        "amount_usd": amount,
        "note": note,
        "total_deposits_usd": state.stats.total_deposits_usd,
    }));
    Ok(())
}

// ── retry ─────────────────────────────────────────────────────────────

async fn cmd_retry() -> Result<()> {
    let mut state = GridState::load()?;
    let failed = match &state.last_failed_trade {
        Some(f) => f.clone(),
        None => {
            bail!("No failed trade to retry");
        }
    };

    let client = GridClient::new()?;

    // Re-fetch price to ensure it's still reasonable
    let price = client.get_eth_price().await?;
    let price_diff_pct = ((price - failed.price) / failed.price * 100.0).abs();
    if price_diff_pct > 5.0 {
        bail!(
            "Price changed {:.1}% since failure (was ${:.2}, now ${:.2}). Run 'grid tick' instead.",
            price_diff_pct,
            failed.price,
            price
        );
    }

    let raw_amount = match failed.direction.as_str() {
        "BUY" => alloy::primitives::U256::from((failed.amount_usd * 1_000_000.0) as u64),
        "SELL" => {
            let eth_amount = failed.amount_usd / price;
            alloy::primitives::U256::from((eth_amount * 1e18) as u128)
        }
        _ => bail!("invalid direction in failed trade"),
    };

    let swap_result = client
        .execute_swap(&failed.direction, raw_amount, price)
        .await?;

    if let Some(ref f) = swap_result.failure {
        bail!("Retry failed: {}", f.reason);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let trade = engine::Trade {
        time: now.clone(),
        direction: failed.direction.clone(),
        price,
        amount_usd: failed.amount_usd,
        tx: swap_result.tx_hash.clone(),
        grid_from: failed.grid_from,
        grid_to: failed.grid_to,
        success: true,
        failure_reason: None,
    };

    state.stats.total_trades += 1;
    state.stats.successful_trades += 1;
    match failed.direction.as_str() {
        "BUY" => state.stats.total_buy_usd += failed.amount_usd,
        "SELL" => state.stats.total_sell_usd += failed.amount_usd,
        _ => {}
    }
    state.current_level = Some(failed.grid_to);
    state.last_trade_times.insert(failed.direction.clone(), now);
    state.push_trade(trade);
    state.errors.consecutive_errors = 0;
    state.last_failed_trade = None;
    state.save()?;

    output::success(json!({
        "action": "retry_success",
        "direction": failed.direction,
        "amount_usd": format!("{:.2}", failed.amount_usd),
        "tx_hash": swap_result.tx_hash,
    }));
    Ok(())
}

// ── analyze ───────────────────────────────────────────────────────────

async fn cmd_analyze() -> Result<()> {
    let state = GridState::load()?;
    let client = GridClient::new()?;

    let price = client.get_eth_price().await?;
    let ema = if state.price_history.len() >= engine::EMA_PERIOD {
        engine::calc_ema(&state.price_history, engine::EMA_PERIOD)
    } else if !state.price_history.is_empty() {
        state.price_history.iter().sum::<f64>() / state.price_history.len() as f64
    } else {
        price
    };

    let vol = engine::calc_volatility(&state.price_history);
    let mean = if state.price_history.is_empty() {
        price
    } else {
        state.price_history.iter().sum::<f64>() / state.price_history.len() as f64
    };
    let vol_pct = if mean > 0.0 { vol / mean * 100.0 } else { 0.0 };

    let price_vs_ema = if ema > 0.0 {
        (price - ema) / ema * 100.0
    } else {
        0.0
    };
    let trend = if price_vs_ema > 1.0 {
        "bullish"
    } else if price_vs_ema < -1.0 {
        "bearish"
    } else if price_vs_ema > 0.0 {
        "slightly_bullish"
    } else {
        "slightly_bearish"
    };

    let mut result = json!({
        "current_price": price,
        "ema_20": format!("{:.2}", ema),
        "price_vs_ema": format!("{:+.2}%", price_vs_ema),
        "volatility_pct": format!("{:.2}", vol_pct),
        "trend": trend,
        "price_history_count": state.price_history.len(),
    });

    if let Some(ref grid) = state.grid {
        let level = engine::price_to_level(price, grid);
        result["grid"] = json!({
            "center": format!("{:.2}", grid.center),
            "step": format!("{:.2}", grid.step),
            "utilization": format!("level {} of {}", level, grid.levels),
            "distance_to_low": format!("${:.2}", price - grid.range.0),
            "distance_to_high": format!("${:.2}", grid.range.1 - price),
        });
    }

    if let Some(ref set_at) = state.grid_set_at {
        if let Ok(t) = chrono::DateTime::parse_from_rfc3339(set_at) {
            let hours = chrono::Utc::now().signed_duration_since(t).num_minutes() as f64 / 60.0;
            result["grid_age_hours"] = json!(format!("{:.1}", hours));
        }
    }

    output::success(result);
    Ok(())
}

// ── config ───────────────────────────────────────────────────────────

async fn cmd_config() -> Result<()> {
    let cfg = GridConfig::load()?;
    let path = GridConfig::config_path();
    let is_custom = path.exists();

    output::success(json!({
        "config_file": path.to_string_lossy(),
        "is_custom": is_custom,
        "parameters": {
            "grid_levels": cfg.grid_levels,
            "tick_interval_secs": cfg.tick_interval_secs,
            "max_trade_pct": cfg.max_trade_pct,
            "min_trade_usd": cfg.min_trade_usd,
            "slippage_pct": cfg.slippage_pct,
            "ema_period": cfg.ema_period,
            "volatility_multiplier": cfg.volatility_multiplier,
            "step_min_pct": cfg.step_min_pct,
            "step_max_pct": cfg.step_max_pct,
            "step_floor": cfg.step_floor,
            "grid_recalibrate_hours": cfg.grid_recalibrate_hours,
            "min_trade_interval": cfg.min_trade_interval,
            "max_same_dir_trades": cfg.max_same_dir_trades,
            "position_max_pct": cfg.position_max_pct,
            "position_min_pct": cfg.position_min_pct,
            "gas_reserve_eth": cfg.gas_reserve_eth,
            "max_consecutive_errors": cfg.max_consecutive_errors,
            "cooldown_after_errors": cfg.cooldown_after_errors,
        }
    }));
    Ok(())
}

// ── set ──────────────────────────────────────────────────────────────

async fn cmd_set(key: &str, value: &str) -> Result<()> {
    let mut cfg = GridConfig::load()?;

    match key {
        "grid_levels" => cfg.grid_levels = value.parse().context("invalid u32")?,
        "tick_interval_secs" => cfg.tick_interval_secs = value.parse().context("invalid u64")?,
        "max_trade_pct" => cfg.max_trade_pct = value.parse().context("invalid f64")?,
        "min_trade_usd" => cfg.min_trade_usd = value.parse().context("invalid f64")?,
        "slippage_pct" => cfg.slippage_pct = value.to_string(),
        "ema_period" => cfg.ema_period = value.parse().context("invalid usize")?,
        "volatility_multiplier" => {
            cfg.volatility_multiplier = value.parse().context("invalid f64")?
        }
        "step_min_pct" => cfg.step_min_pct = value.parse().context("invalid f64")?,
        "step_max_pct" => cfg.step_max_pct = value.parse().context("invalid f64")?,
        "step_floor" => cfg.step_floor = value.parse().context("invalid f64")?,
        "grid_recalibrate_hours" => {
            cfg.grid_recalibrate_hours = value.parse().context("invalid f64")?
        }
        "min_trade_interval" => cfg.min_trade_interval = value.parse().context("invalid u64")?,
        "max_same_dir_trades" => {
            cfg.max_same_dir_trades = value.parse().context("invalid usize")?
        }
        "position_max_pct" => cfg.position_max_pct = value.parse().context("invalid f64")?,
        "position_min_pct" => cfg.position_min_pct = value.parse().context("invalid f64")?,
        "gas_reserve_eth" => cfg.gas_reserve_eth = value.parse().context("invalid f64")?,
        "max_consecutive_errors" => {
            cfg.max_consecutive_errors = value.parse().context("invalid u32")?
        }
        "cooldown_after_errors" => {
            cfg.cooldown_after_errors = value.parse().context("invalid u64")?
        }
        _ => bail!(
            "Unknown parameter '{}'. Use 'plugin-store grid config' to see available parameters.",
            key
        ),
    }

    cfg.save()?;
    output::success(json!({
        "message": format!("Set {} = {}", key, value),
        "config_file": GridConfig::config_path().to_string_lossy(),
    }));
    Ok(())
}
