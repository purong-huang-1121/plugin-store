//! CLI commands for SOL Memepump Scanner strategy.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};

use crate::notifier::{Notifier, NotifyLevel};
use crate::output;
use crate::strategy::memepump_scanner::client::ScannerClient;
use crate::strategy::memepump_scanner::config::ScannerConfig;
use crate::strategy::memepump_scanner::engine;
use crate::strategy::memepump_scanner::state::ScannerState;

#[derive(Subcommand)]
pub enum ScannerCommand {
    /// Execute one scan cycle: scan -> filter -> signal -> trade -> monitor exits
    Tick,
    /// Start foreground daemon (tick every 10s)
    Start,
    /// Stop running daemon via PID file
    Stop,
    /// Show current positions, session stats, PnL overview
    Status,
    /// Detailed P&L report
    Report,
    /// Trade history
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
    /// Run full pipeline without trading -- output filter/signal results
    Analyze,
    /// Show current configuration
    Config,
    /// Set a configuration parameter
    Set {
        /// Parameter name
        key: String,
        /// New value
        value: String,
    },
}

pub async fn execute(cmd: ScannerCommand) -> Result<()> {
    match cmd {
        ScannerCommand::Tick => {
            let notifier = Notifier::from_env("\u{1f50d} Scanner");
            cmd_tick(&notifier).await
        }
        ScannerCommand::Start => cmd_start().await,
        ScannerCommand::Stop => cmd_stop().await,
        ScannerCommand::Status => cmd_status().await,
        ScannerCommand::Report => cmd_report().await,
        ScannerCommand::History { limit } => cmd_history(limit).await,
        ScannerCommand::Reset { force } => cmd_reset(force).await,
        ScannerCommand::Analyze => cmd_analyze().await,
        ScannerCommand::Config => cmd_config().await,
        ScannerCommand::Set { key, value } => cmd_set(&key, &value).await,
    }
}

// ── status ──────────────────────────────────────────────────────────

async fn cmd_status() -> Result<()> {
    let state = ScannerState::load()?;

    let bot_running = {
        let pid_path = ScannerState::pid_path();
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

    let positions: serde_json::Map<String, serde_json::Value> = state
        .positions
        .iter()
        .map(|(addr, pos)| {
            let pnl_pct = if pos.entry_price > 0.0 {
                (pos.peak_price - pos.entry_price) / pos.entry_price * 100.0
            } else {
                0.0
            };
            let age_min = if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&pos.entry_time) {
                chrono::Utc::now().signed_duration_since(t).num_seconds() as f64 / 60.0
            } else {
                0.0
            };
            (
                addr.clone(),
                json!({
                    "symbol": pos.symbol,
                    "pnl_pct": format!("{:.2}", pnl_pct),
                    "age_min": format!("{:.1}", age_min),
                    "entry_sol": pos.entry_sol,
                    "tier": pos.tier,
                    "launch": pos.launch,
                    "tp1_hit": pos.tp1_hit,
                    "sell_fail_count": pos.sell_fail_count,
                }),
            )
        })
        .collect();

    output::success(json!({
        "bot_running": bot_running,
        "stopped": state.stopped,
        "stop_reason": state.stop_reason,
        "positions_count": state.positions.len(),
        "positions": positions,
        "stats": {
            "total_buys": state.stats.total_buys,
            "total_sells": state.stats.total_sells,
            "successful_trades": state.stats.successful_trades,
            "failed_trades": state.stats.failed_trades,
            "session_pnl_sol": state.stats.session_pnl_sol,
            "consecutive_losses": state.stats.consecutive_losses,
            "cumulative_loss_sol": state.stats.cumulative_loss_sol,
        },
        "consecutive_errors": state.errors.consecutive_errors,
        "last_error": state.errors.last_error_msg,
        "paused_until": state.paused_until,
    }));
    Ok(())
}

// ── report ──────────────────────────────────────────────────────────

async fn cmd_report() -> Result<()> {
    let state = ScannerState::load()?;

    let total_trades = state.stats.total_buys + state.stats.total_sells;
    let win_rate = if state.stats.successful_trades > 0 {
        let total = state.stats.successful_trades + state.stats.failed_trades;
        if total > 0 {
            format!(
                "{:.1}%",
                state.stats.successful_trades as f64 / total as f64 * 100.0
            )
        } else {
            "N/A".to_string()
        }
    } else {
        "N/A".to_string()
    };

    output::success(json!({
        "total_trades": total_trades,
        "total_buys": state.stats.total_buys,
        "total_sells": state.stats.total_sells,
        "successful_trades": state.stats.successful_trades,
        "failed_trades": state.stats.failed_trades,
        "win_rate": win_rate,
        "total_invested_sol": state.stats.total_invested_sol,
        "total_returned_sol": state.stats.total_returned_sol,
        "session_pnl_sol": state.stats.session_pnl_sol,
        "consecutive_losses": state.stats.consecutive_losses,
        "cumulative_loss_sol": state.stats.cumulative_loss_sol,
        "positions_open": state.positions.len(),
        "signals_recorded": state.signals.len(),
    }));
    Ok(())
}

// ── history ─────────────────────────────────────────────────────────

async fn cmd_history(limit: usize) -> Result<()> {
    let state = ScannerState::load()?;
    let limit = limit.min(engine::MAX_TRADES);
    let trades: Vec<_> = state.trades.iter().rev().take(limit).collect();
    output::success(json!({ "trades": trades, "total": state.trades.len() }));
    Ok(())
}

// ── reset ───────────────────────────────────────────────────────────

async fn cmd_reset(force: bool) -> Result<()> {
    if !force {
        bail!("This will delete all scanner state. Use --force to confirm.");
    }
    ScannerState::reset()?;
    output::success(json!({ "message": "Scanner state reset" }));
    Ok(())
}

// ── stop ────────────────────────────────────────────────────────────

async fn cmd_stop() -> Result<()> {
    let pid_path = ScannerState::pid_path();
    if !pid_path.exists() {
        bail!("No running scanner found (PID file missing)");
    }
    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    if pid <= 0 {
        let _ = std::fs::remove_file(&pid_path);
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
    output::success(json!({ "message": format!("Stopped scanner (PID {})", pid) }));
    Ok(())
}

// ── tick ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
async fn cmd_tick(notifier: &Notifier) -> Result<()> {
    let cfg = ScannerConfig::load()?;
    let mut state = ScannerState::load()?;
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Session risk check
    if let Some(reason) = engine::check_session_risk(
        state.stats.consecutive_losses,
        state.stats.cumulative_loss_sol,
        state.paused_until.as_deref(),
        &now,
    ) {
        notifier
            .notify(NotifyLevel::Warning, &format!("Session paused: {reason}"))
            .await;
        output::success(json!({ "action": "paused", "reason": reason }));
        return Ok(());
    }

    // 2. Circuit breaker check
    if let Some(reason) = engine::check_circuit_breaker(
        state.errors.consecutive_errors,
        state.errors.last_error_time.as_deref(),
        &now,
    ) {
        notifier
            .notify(
                NotifyLevel::Error,
                &format!("Circuit breaker active: {reason}"),
            )
            .await;
        output::success(json!({ "action": "circuit_breaker", "reason": reason }));
        return Ok(());
    }

    // 3. Create client
    let client = ScannerClient::new()?;

    let mut scanned = 0u32;
    let mut passed_filter = 0u32;
    let mut signals_found = 0u32;
    let mut trades_executed = 0u32;
    let mut exits_triggered = 0u32;

    // Compute current SOL used from open positions
    let sol_used: f64 = state.positions.values().map(|p| p.entry_sol).sum();

    // ── SCAN PHASE ──
    let can_buy = sol_used < cfg.max_sol && state.positions.len() < cfg.max_positions;

    if can_buy {
        // Build server-side filter params
        let params = build_filter_params(&cfg);

        let tokens = match client.get_memepump_list(&params).await {
            Ok(t) => t,
            Err(e) => {
                state.errors.consecutive_errors += 1;
                state.errors.last_error_time = Some(now.clone());
                state.errors.last_error_msg = Some(format!("{e:#}"));
                state.save()?;
                output::error(&format!("tokenList failed: {e:#}"));
                return Ok(());
            }
        };
        scanned = tokens.len() as u32;

        for token_val in &tokens {
            let addr = token_addr(token_val);
            if addr.is_empty() {
                continue;
            }

            let token = parse_token_data(token_val);

            // Layer 2: Client-side filter
            if engine::classify_token_with(
                &token,
                cfg.cf_min_bs_ratio,
                cfg.cf_min_vol_mc_pct,
                cfg.cf_max_top10,
            )
            .is_none()
            {
                continue;
            }
            passed_filter += 1;

            // Rate limit
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;

            // Fetch candles for signal detection
            let candles_val = match client.get_candles(&addr, 6).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            let candles = match &candles_val {
                Value::Array(arr) => arr.clone(),
                _ => candles_val.as_array().cloned().unwrap_or_default(),
            };

            let last_vol = candles
                .first()
                .map(|c| engine::safe_float(&c["volume"], 0.0))
                .unwrap_or(0.0);
            let launch = engine::classify_launch(last_vol);

            // Signal A: TX acceleration
            let current_tx = token_tx_1m(token_val);
            let prev_tx = if state.prev_tx.contains(&addr) {
                current_tx.saturating_sub(1).max(1)
            } else {
                0
            };
            let elapsed_secs = engine::TICK_INTERVAL_SECS as u32;
            let (sig_a, sig_a_ratio) =
                engine::check_signal_a(current_tx, elapsed_secs, prev_tx, launch);

            // Signal B: Volume spike
            let volumes: Vec<f64> = candles
                .iter()
                .skip(1)
                .take(5)
                .map(|c| engine::safe_float(&c["volume"], 0.0))
                .collect();
            let current_vol = candles
                .first()
                .map(|c| engine::safe_float(&c["volume"], 0.0))
                .unwrap_or(0.0);
            let (sig_b, sig_b_ratio) = engine::check_signal_b(current_vol, &volumes, launch);

            // Signal C: Buy/sell ratio
            let sig_c = engine::check_signal_c(token.buy_tx_1h, token.sell_tx_1h);

            // Combine signals
            let tier = match engine::detect_signal(sig_a, sig_b, sig_c) {
                Some(t) => t,
                None => {
                    state.prev_tx.insert(addr);
                    continue;
                }
            };
            signals_found += 1;

            // Rate limit before deep safety
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;

            // Layer 3: Deep safety check
            let dev_info = client.get_dev_info(&addr).await.unwrap_or(Value::Null);
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;
            let bundle_info = client.get_bundle_info(&addr).await.unwrap_or(Value::Null);

            let safety = engine::deep_safety_check_with(
                engine::safe_u32(&dev_info["rugPullCount"], 0),
                engine::safe_u32(&dev_info["totalLaunched"], 0),
                engine::safe_float(&dev_info["devHoldPercent"], 0.0),
                engine::safe_float(&bundle_info["bundlerAthPercent"], 0.0),
                engine::safe_u32(&bundle_info["bundlerCount"], 0),
                cfg.ds_max_dev_hold,
                cfg.ds_max_bundler_ath,
                cfg.ds_max_bundler_count,
            );

            let safety_str = match safety {
                engine::SafetyVerdict::Safe => None,
                engine::SafetyVerdict::Unsafe(r) => Some(r.as_str().to_string()),
            };

            if safety != engine::SafetyVerdict::Safe {
                let reason = safety_str.as_deref().unwrap_or("unsafe");
                notifier
                    .notify(
                        NotifyLevel::Warning,
                        &format!(
                            "Signal skipped (safety)\n{} MC ${:.0}\nReason: {}",
                            token.symbol, token.market_cap, reason
                        ),
                    )
                    .await;
                state.push_signal(crate::strategy::memepump_scanner::state::SignalRecord {
                    time: now.clone(),
                    token_address: addr.clone(),
                    symbol: token.symbol.clone(),
                    tier,
                    launch,
                    sig_a_ratio,
                    sig_b_ratio,
                    market_cap: token.market_cap,
                    acted: false,
                    skip_reason: safety_str,
                });
                state.prev_tx.insert(addr);
                continue;
            }

            // Skip if already holding
            if state.positions.contains_key(&addr) {
                state.push_signal(crate::strategy::memepump_scanner::state::SignalRecord {
                    time: now.clone(),
                    token_address: addr.clone(),
                    symbol: token.symbol.clone(),
                    tier,
                    launch,
                    sig_a_ratio,
                    sig_b_ratio,
                    market_cap: token.market_cap,
                    acted: false,
                    skip_reason: Some("already_holding".to_string()),
                });
                state.prev_tx.insert(addr);
                continue;
            }

            // Collision guard: check ranking_sniper positions
            let sniper_state =
                crate::strategy::ranking_sniper::state::SniperState::load().unwrap_or_default();
            if sniper_state.positions.contains_key(&addr) {
                state.push_signal(crate::strategy::memepump_scanner::state::SignalRecord {
                    time: now.clone(),
                    token_address: addr.clone(),
                    symbol: token.symbol.clone(),
                    tier,
                    launch,
                    sig_a_ratio,
                    sig_b_ratio,
                    market_cap: token.market_cap,
                    acted: false,
                    skip_reason: Some("in_ranking_sniper".to_string()),
                });
                state.prev_tx.insert(addr);
                continue;
            }

            // Position sizing
            let sol_amount = cfg.position_size(tier);
            let current_sol_used: f64 = state.positions.values().map(|p| p.entry_sol).sum();
            if current_sol_used + sol_amount > cfg.max_sol {
                state.push_signal(crate::strategy::memepump_scanner::state::SignalRecord {
                    time: now.clone(),
                    token_address: addr.clone(),
                    symbol: token.symbol.clone(),
                    tier,
                    launch,
                    sig_a_ratio,
                    sig_b_ratio,
                    market_cap: token.market_cap,
                    acted: false,
                    skip_reason: Some("budget_exceeded".to_string()),
                });
                state.prev_tx.insert(addr);
                continue;
            }

            // Execute buy
            let slippage_val = cfg.slippage(tier);
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;

            match client.buy_token(&addr, sol_amount, slippage_val).await {
                Ok(result) => {
                    let entry_price = token_price(token_val);
                    let breakeven = engine::calc_breakeven_pct(sol_amount);

                    state.positions.insert(
                        addr.clone(),
                        crate::strategy::memepump_scanner::state::Position {
                            token_address: addr.clone(),
                            symbol: token.symbol.clone(),
                            tier,
                            launch,
                            entry_price,
                            entry_sol: sol_amount,
                            token_amount_raw: format!("{}", result.amount_out as u64),
                            entry_time: now.clone(),
                            peak_price: entry_price,
                            tp1_hit: false,
                            breakeven_pct: breakeven,
                            sell_fail_count: 0,
                        },
                    );

                    state.stats.total_buys += 1;
                    state.stats.successful_trades += 1;
                    state.stats.total_invested_sol += sol_amount;
                    state.errors.consecutive_errors = 0;

                    state.push_signal(crate::strategy::memepump_scanner::state::SignalRecord {
                        time: now.clone(),
                        token_address: addr.clone(),
                        symbol: token.symbol.clone(),
                        tier,
                        launch,
                        sig_a_ratio,
                        sig_b_ratio,
                        market_cap: token.market_cap,
                        acted: true,
                        skip_reason: None,
                    });

                    state.push_trade(crate::strategy::memepump_scanner::state::Trade {
                        time: now.clone(),
                        token_address: addr.clone(),
                        symbol: token.symbol.clone(),
                        direction: "BUY".to_string(),
                        sol_amount,
                        price: entry_price,
                        tier,
                        launch,
                        tx_hash: result.tx_hash.clone(),
                        success: true,
                        exit_reason: None,
                        pnl_sol: None,
                    });

                    trades_executed += 1;
                    notifier
                        .notify(
                            NotifyLevel::Success,
                            &format!(
                                "BUY {} ({:?})\n{sol_amount} SOL @ MC ${:.0}\ntx: {}",
                                token.symbol,
                                tier,
                                token.market_cap,
                                result.tx_hash.as_deref().unwrap_or("pending")
                            ),
                        )
                        .await;
                }
                Err(e) => {
                    state.errors.consecutive_errors += 1;
                    state.errors.last_error_time = Some(now.clone());
                    state.errors.last_error_msg = Some(format!("{e:#}"));
                    state.stats.total_buys += 1;
                    state.stats.failed_trades += 1;

                    state.push_signal(crate::strategy::memepump_scanner::state::SignalRecord {
                        time: now.clone(),
                        token_address: addr.clone(),
                        symbol: token.symbol.clone(),
                        tier,
                        launch,
                        sig_a_ratio,
                        sig_b_ratio,
                        market_cap: token.market_cap,
                        acted: false,
                        skip_reason: Some(format!("buy_failed: {e:#}")),
                    });

                    notifier
                        .notify(
                            NotifyLevel::Error,
                            &format!("BUY failed {}: {e:#}", token.symbol),
                        )
                        .await;
                }
            }

            state.prev_tx.insert(addr);
        }

        // Trim prev_tx to prevent unbounded growth
        state.trim_prev_tx();
    }

    // ── MONITOR PHASE ──
    let position_addrs: Vec<String> = state.positions.keys().cloned().collect();
    for addr in &position_addrs {
        tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;

        let price_info = match client.get_price_info(addr).await {
            Ok(p) => p,
            Err(_) => continue,
        };

        let current_price = engine::safe_float(&price_info["price"], 0.0);
        if current_price <= 0.0 {
            continue;
        }

        // Extract position data and update peak price. The mutable borrow ends
        // after this block so we can call state methods later without conflict.
        let (
            entry_price,
            peak_price,
            tp1_hit,
            entry_time,
            entry_sol,
            tier,
            launch,
            breakeven_pct,
            sell_fail_count,
            token_amount_raw,
            symbol,
        ) = {
            let pos = match state.positions.get_mut(addr) {
                Some(p) => p,
                None => continue,
            };
            if current_price > pos.peak_price {
                pos.peak_price = current_price;
            }
            (
                pos.entry_price,
                pos.peak_price,
                pos.tp1_hit,
                pos.entry_time.clone(),
                pos.entry_sol,
                pos.tier,
                pos.launch,
                pos.breakeven_pct,
                pos.sell_fail_count,
                pos.token_amount_raw.clone(),
                pos.symbol.clone(),
            )
        };

        // Skip stuck positions (too many sell failures)
        if sell_fail_count >= engine::STUCK_MAX_FAILS {
            continue;
        }

        let pnl_pct = if entry_price > 0.0 {
            (current_price - entry_price) / entry_price * 100.0
        } else {
            0.0
        };

        let age_min = if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&entry_time) {
            chrono::Utc::now().signed_duration_since(t).num_seconds() as f64 / 60.0
        } else {
            0.0
        };

        let exit = engine::check_exit(
            pnl_pct,
            age_min,
            peak_price,
            current_price,
            tp1_hit,
            tier,
            launch,
            breakeven_pct,
            &cfg.exit_params(),
        );

        if let Some(action) = exit {
            let sell_pct = engine::exit_sell_pct(action);

            // Calculate sell amount from raw token amount
            let total_tokens: u64 = token_amount_raw.parse().unwrap_or(0);
            let sell_tokens = (total_tokens as f64 * sell_pct) as u64;
            let sell_amount_raw = sell_tokens.to_string();

            let slippage_val = cfg.slippage(tier);
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;

            match client
                .sell_token(addr, &sell_amount_raw, slippage_val)
                .await
            {
                Ok(result) => {
                    let sol_out = result.amount_out;
                    let pnl_sol = sol_out - entry_sol * sell_pct;

                    // Update position or remove if fully exited
                    if sell_pct >= 0.99 {
                        state.positions.remove(addr);
                        if pnl_sol < 0.0 {
                            state.record_loss(pnl_sol.abs());
                        } else {
                            state.record_win();
                        }
                    } else {
                        // Partial exit (e.g. TP1)
                        if let Some(pos) = state.positions.get_mut(addr) {
                            let remaining_tokens =
                                total_tokens.saturating_sub(sell_tokens).to_string();
                            pos.token_amount_raw = remaining_tokens;
                            if matches!(action, engine::ExitAction::TakeProfit1 { .. }) {
                                pos.tp1_hit = true;
                            }
                        }
                    }

                    state.stats.total_sells += 1;
                    state.stats.total_returned_sol += sol_out;
                    state.stats.session_pnl_sol += pnl_sol;

                    state.push_trade(crate::strategy::memepump_scanner::state::Trade {
                        time: now.clone(),
                        token_address: addr.clone(),
                        symbol: symbol.clone(),
                        direction: "SELL".to_string(),
                        sol_amount: sol_out,
                        price: current_price,
                        tier,
                        launch,
                        tx_hash: result.tx_hash.clone(),
                        success: true,
                        exit_reason: Some(action.as_str().to_string()),
                        pnl_sol: Some(pnl_sol),
                    });

                    exits_triggered += 1;

                    notifier
                        .notify(
                            NotifyLevel::Success,
                            &format!(
                                "SELL {} ({})\nPnL: {pnl_pct:.1}%\ntx: {}",
                                symbol,
                                action.as_str(),
                                result.tx_hash.as_deref().unwrap_or("pending")
                            ),
                        )
                        .await;
                }
                Err(_e) => {
                    if let Some(pos) = state.positions.get_mut(addr) {
                        pos.sell_fail_count += 1;
                        if pos.sell_fail_count >= engine::STUCK_MAX_FAILS {
                            let sol_in = pos.entry_sol;
                            let sym = pos.symbol.clone();
                            let fails = pos.sell_fail_count;
                            state.stats.cumulative_loss_sol += sol_in;
                            notifier
                                .notify(
                                    NotifyLevel::Error,
                                    &format!(
                                        "STUCK {} -- {} sell failures, marking -100%",
                                        sym, fails
                                    ),
                                )
                                .await;
                        }
                    }

                    state.stats.failed_trades += 1;
                }
            }
        }
    }

    // Save state
    state.save()?;

    let final_sol_used: f64 = state.positions.values().map(|p| p.entry_sol).sum();

    // Send tick summary notification when something interesting happened
    if trades_executed > 0 || exits_triggered > 0 || signals_found > 0 {
        notifier
            .notify(
                NotifyLevel::Info,
                &format!(
                    "Tick summary\nScanned: {} | Filtered: {} | Signals: {}\nBuys: {} | Exits: {}\nPositions: {} | SOL used: {:.3}\nPnL: {:.3} SOL",
                    scanned, passed_filter, signals_found,
                    trades_executed, exits_triggered,
                    state.positions.len(), final_sol_used,
                    state.stats.session_pnl_sol
                ),
            )
            .await;
    }

    output::success(json!({
        "cycle_time": now,
        "scanned": scanned,
        "passed_filter": passed_filter,
        "signals_found": signals_found,
        "trades_executed": trades_executed,
        "positions_monitored": position_addrs.len(),
        "exits_triggered": exits_triggered,
        "sol_used": final_sol_used,
        "session_pnl_sol": state.stats.session_pnl_sol,
    }));
    Ok(())
}

// ── start ────────────────────────────────────────────────────────────

async fn cmd_start() -> Result<()> {
    let cfg = ScannerConfig::load()?;
    let pid_path = ScannerState::pid_path();

    // Check if already running
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
        let pid: i32 = pid_str.trim().parse().unwrap_or(0);
        #[cfg(unix)]
        if pid > 0 && unsafe { libc::kill(pid, 0) } == 0 {
            bail!(
                "Scanner already running (PID {}). Use 'scanner stop' first.",
                pid
            );
        }
    }

    // Validate credentials
    let _ = ScannerClient::new()?;

    // Show config summary before starting
    let is_custom = ScannerConfig::config_path().exists();
    let log_path = ScannerConfig::log_path();
    output::success(json!({
        "message": format!("Scanner started (PID {})", std::process::id()),
        "files": {
            "config": ScannerConfig::config_path().to_string_lossy(),
            "state": ScannerState::state_path().to_string_lossy(),
            "log": log_path.to_string_lossy(),
            "pid": ScannerState::pid_path().to_string_lossy(),
        },
        "is_custom": is_custom,
        "parameters": {
            "scan_filters": {
                "stage": cfg.stage,
                "min_mc": cfg.tf_min_mc,
                "max_mc": cfg.tf_max_mc,
                "min_holders": cfg.tf_min_holders,
                "max_dev_hold": cfg.tf_max_dev_hold,
                "max_bundler": cfg.tf_max_bundler,
                "max_sniper": cfg.tf_max_sniper,
                "max_insider": cfg.tf_max_insider,
                "max_top10": cfg.tf_max_top10,
                "max_fresh": cfg.tf_max_fresh,
                "min_tx": cfg.tf_min_tx,
                "min_buy_tx": cfg.tf_min_buy_tx,
                "min_age": cfg.tf_min_age,
                "max_age": cfg.tf_max_age,
                "min_vol": cfg.tf_min_vol,
            },
            "client_filters": {
                "min_bs_ratio": cfg.cf_min_bs_ratio,
                "min_vol_mc_pct": cfg.cf_min_vol_mc_pct,
                "max_top10": cfg.cf_max_top10,
            },
            "deep_safety": {
                "max_dev_hold": cfg.ds_max_dev_hold,
                "max_bundler_ath": cfg.ds_max_bundler_ath,
                "max_bundler_count": cfg.ds_max_bundler_count,
            },
            "position": {
                "scalp_sol": cfg.scalp_sol,
                "minimum_sol": cfg.minimum_sol,
                "max_sol": cfg.max_sol,
                "max_positions": cfg.max_positions,
                "slippage_scalp": cfg.slippage_scalp,
                "slippage_minimum": cfg.slippage_minimum,
            },
            "exit": {
                "tp1_pct": cfg.tp1_pct,
                "tp2_pct": cfg.tp2_pct,
                "sl_scalp": cfg.sl_scalp,
                "sl_hot": cfg.sl_hot,
                "sl_quiet": cfg.sl_quiet,
                "trailing_pct": cfg.trailing_pct,
                "max_hold_min": cfg.max_hold_min,
            },
            "session_risk": {
                "max_consec_loss": cfg.max_consec_loss,
                "pause_loss_sol": cfg.pause_loss_sol,
                "stop_loss_sol": cfg.stop_loss_sol,
            },
            "tick_interval_secs": cfg.tick_interval_secs,
        }
    }));

    // Write PID file
    let dir = pid_path.parent().context("no parent dir")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let notifier = Notifier::from_env("\u{1f50d} Scanner");

    write_log(
        &log_path,
        &format!("Scanner started (PID {})", std::process::id()),
    );
    notifier
        .notify(
            NotifyLevel::Info,
            &format!(
                "Scanner started (PID {})\nStage: {} | Tick: {}s\nMax SOL: {} | Max positions: {}\nScalp: {} SOL | Min: {} SOL",
                std::process::id(),
                cfg.stage,
                cfg.tick_interval_secs,
                cfg.max_sol,
                cfg.max_positions,
                cfg.scalp_sol,
                cfg.minimum_sol,
            ),
        )
        .await;

    // Signal handlers
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    });

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

    let tick_secs = cfg.tick_interval_secs;

    // Main loop
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        if let Err(e) = cmd_tick(&notifier).await {
            write_log(&log_path, &format!("Tick error: {e:#}"));
            notifier
                .notify(NotifyLevel::Error, &format!("Tick error: {e:#}"))
                .await;
        }
        // Sleep in small increments to check shutdown flag
        for _ in 0..(tick_secs / 5).max(1) {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    write_log(&log_path, "Scanner stopped");
    let final_state = ScannerState::load().unwrap_or_default();
    notifier
        .notify(
            NotifyLevel::Info,
            &format!(
                "Scanner stopped\nBuys: {} | Sells: {}\nPositions: {}\nPnL: {:.3} SOL",
                final_state.stats.total_buys,
                final_state.stats.total_sells,
                final_state.positions.len(),
                final_state.stats.session_pnl_sol,
            ),
        )
        .await;
    output::success(json!({ "message": "Scanner stopped" }));
    Ok(())
}

// ── analyze ──────────────────────────────────────────────────────────

async fn cmd_analyze() -> Result<()> {
    let cfg = ScannerConfig::load()?;
    let state = ScannerState::load()?;
    let client = ScannerClient::new_read_only()?;

    // Build the same server-side filter params as cmd_tick
    let params = build_filter_params(&cfg);

    let tokens = client.get_memepump_list(&params).await?;
    let scanned = tokens.len();

    let mut layer2_passed = Vec::new();
    let mut layer2_rejected = Vec::new();
    let mut signals = Vec::new();
    let mut layer3_results = Vec::new();
    let mut would_trade = Vec::new();

    for token_val in &tokens {
        let addr = token_addr(token_val);
        let token = parse_token_data(token_val);
        let symbol = &token.symbol;

        let bs_ratio = if token.sell_tx_1h > 0 {
            token.buy_tx_1h as f64 / token.sell_tx_1h as f64
        } else {
            0.0
        };
        let vol_mc = if token.market_cap > 0.0 {
            token.volume_1h / token.market_cap * 100.0
        } else {
            0.0
        };

        match engine::classify_token_with(
            &token,
            cfg.cf_min_bs_ratio,
            cfg.cf_min_vol_mc_pct,
            cfg.cf_max_top10,
        ) {
            None => {
                let reason = if bs_ratio < cfg.cf_min_bs_ratio {
                    format!("B/S={bs_ratio:.2}<{}", cfg.cf_min_bs_ratio)
                } else if vol_mc < cfg.cf_min_vol_mc_pct {
                    format!("Vol/MC={vol_mc:.1}%<{}%", cfg.cf_min_vol_mc_pct)
                } else if token.top10_pct > cfg.cf_max_top10 {
                    format!("Top10={:.1}%>{}%", token.top10_pct, cfg.cf_max_top10)
                } else {
                    "unknown".to_string()
                };
                layer2_rejected.push(json!({
                    "symbol": symbol, "address": addr,
                    "reason": reason, "bs": format!("{bs_ratio:.2}"), "vol_mc": format!("{vol_mc:.1}"),
                }));
                continue;
            }
            Some(_) => {
                layer2_passed.push(json!({ "symbol": symbol, "address": addr }));
            }
        }

        // Signal detection
        tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;
        let candles_val = client.get_candles(&addr, 6).await.unwrap_or(Value::Null);
        let candles = match &candles_val {
            Value::Array(arr) => arr.clone(),
            _ => candles_val.as_array().cloned().unwrap_or_default(),
        };

        let last_vol = candles
            .first()
            .map(|c| engine::safe_float(&c["volume"], 0.0))
            .unwrap_or(0.0);
        let launch = engine::classify_launch(last_vol);

        let current_tx = token_tx_1m(token_val);
        // In analyze mode, use prev_tx set to estimate -- if token is known, assume similar tx count
        let prev_tx = if state.prev_tx.contains(&addr) {
            current_tx.saturating_sub(1).max(1)
        } else {
            0
        };
        let elapsed_secs = engine::TICK_INTERVAL_SECS as u32;
        let (sig_a, sig_a_ratio) =
            engine::check_signal_a(current_tx, elapsed_secs, prev_tx, launch);

        let volumes: Vec<f64> = candles
            .iter()
            .skip(1)
            .take(5)
            .map(|c| engine::safe_float(&c["volume"], 0.0))
            .collect();
        let current_vol = candles
            .first()
            .map(|c| engine::safe_float(&c["volume"], 0.0))
            .unwrap_or(0.0);
        let (sig_b, sig_b_ratio) = engine::check_signal_b(current_vol, &volumes, launch);
        let sig_c = engine::check_signal_c(token.buy_tx_1h, token.sell_tx_1h);

        let launch_str = match launch {
            engine::LaunchType::Hot => "HOT",
            engine::LaunchType::Quiet => "QUIET",
        };

        if let Some(tier) = engine::detect_signal(sig_a, sig_b, sig_c) {
            let tier_str = match tier {
                engine::SignalTier::Scalp => "SCALP",
                engine::SignalTier::Minimum => "MINIMUM",
            };
            signals.push(json!({
                "symbol": symbol, "address": addr,
                "tier": tier_str, "launch": launch_str,
                "sig_a": sig_a, "sig_a_ratio": format!("{sig_a_ratio:.2}"),
                "sig_b": sig_b, "sig_b_ratio": format!("{sig_b_ratio:.2}"),
                "sig_c": sig_c,
            }));

            // Deep safety check
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;
            let dev_info = client.get_dev_info(&addr).await.unwrap_or(Value::Null);
            tokio::time::sleep(std::time::Duration::from_millis(engine::API_DELAY_MS)).await;
            let bundle_info = client.get_bundle_info(&addr).await.unwrap_or(Value::Null);

            let rug = engine::safe_u32(&dev_info["rugPullCount"], 0);
            let launched = engine::safe_u32(&dev_info["totalLaunched"], 0);
            let dev_hold = engine::safe_float(&dev_info["devHoldPercent"], 0.0);
            let bundler_ath = engine::safe_float(&bundle_info["bundlerAthPercent"], 0.0);
            let bundler_count = engine::safe_u32(&bundle_info["bundlerCount"], 0);

            let safety = engine::deep_safety_check_with(
                rug,
                launched,
                dev_hold,
                bundler_ath,
                bundler_count,
                cfg.ds_max_dev_hold,
                cfg.ds_max_bundler_ath,
                cfg.ds_max_bundler_count,
            );

            let verdict = match safety {
                engine::SafetyVerdict::Safe => "SAFE",
                engine::SafetyVerdict::Unsafe(ref r) => r.as_str(),
            };

            layer3_results.push(json!({
                "symbol": symbol, "address": addr,
                "verdict": verdict,
                "dev_rug": rug, "dev_launched": launched, "dev_hold": format!("{dev_hold:.1}"),
                "bundler_ath": format!("{bundler_ath:.1}"), "bundler_count": bundler_count,
            }));

            if safety == engine::SafetyVerdict::Safe {
                would_trade.push(json!({
                    "symbol": symbol, "address": addr,
                    "tier": tier_str, "sol": engine::position_size(tier),
                }));
            }
        }
    }

    output::success(json!({
        "scanned": scanned,
        "layer2_passed": layer2_passed.len(),
        "layer2_rejected_count": layer2_rejected.len(),
        "layer2_rejected": layer2_rejected,
        "signals": signals,
        "layer3_results": layer3_results,
        "would_trade": would_trade,
    }));
    Ok(())
}

// ── config / set ─────────────────────────────────────────────────────

async fn cmd_config() -> Result<()> {
    let cfg = ScannerConfig::load()?;
    output::success(json!({
        "files": {
            "config": ScannerConfig::config_path().to_string_lossy(),
            "state": ScannerState::state_path().to_string_lossy(),
            "log": ScannerConfig::log_path().to_string_lossy(),
            "pid": ScannerState::pid_path().to_string_lossy(),
        },
        "scan": {
            "stage": cfg.stage,
        },
        "filters": {
            "tf_min_mc": cfg.tf_min_mc,
            "tf_max_mc": cfg.tf_max_mc,
            "tf_min_holders": cfg.tf_min_holders,
            "tf_max_dev_hold": cfg.tf_max_dev_hold,
            "tf_max_bundler": cfg.tf_max_bundler,
            "tf_max_sniper": cfg.tf_max_sniper,
            "tf_max_insider": cfg.tf_max_insider,
            "tf_max_top10": cfg.tf_max_top10,
            "tf_max_fresh": cfg.tf_max_fresh,
            "tf_min_tx": cfg.tf_min_tx,
            "tf_min_buy_tx": cfg.tf_min_buy_tx,
            "tf_min_age": cfg.tf_min_age,
            "tf_max_age": cfg.tf_max_age,
            "tf_min_vol": cfg.tf_min_vol,
        },
        "client_filters": {
            "cf_min_bs_ratio": cfg.cf_min_bs_ratio,
            "cf_min_vol_mc_pct": cfg.cf_min_vol_mc_pct,
            "cf_max_top10": cfg.cf_max_top10,
        },
        "deep_safety": {
            "ds_max_dev_hold": cfg.ds_max_dev_hold,
            "ds_max_bundler_ath": cfg.ds_max_bundler_ath,
            "ds_max_bundler_count": cfg.ds_max_bundler_count,
        },
        "position": {
            "scalp_sol": cfg.scalp_sol,
            "minimum_sol": cfg.minimum_sol,
            "max_sol": cfg.max_sol,
            "max_positions": cfg.max_positions,
            "slippage_scalp": cfg.slippage_scalp,
            "slippage_minimum": cfg.slippage_minimum,
        },
        "exit": {
            "tp1_pct": cfg.tp1_pct,
            "tp2_pct": cfg.tp2_pct,
            "sl_scalp": cfg.sl_scalp,
            "sl_hot": cfg.sl_hot,
            "sl_quiet": cfg.sl_quiet,
            "trailing_pct": cfg.trailing_pct,
            "max_hold_min": cfg.max_hold_min,
        },
        "session_risk": {
            "max_consec_loss": cfg.max_consec_loss,
            "pause_loss_sol": cfg.pause_loss_sol,
            "stop_loss_sol": cfg.stop_loss_sol,
        },
        "tick_interval_secs": cfg.tick_interval_secs,
    }));
    Ok(())
}

async fn cmd_set(key: &str, value: &str) -> Result<()> {
    let mut cfg = ScannerConfig::load()?;

    match key {
        "stage" => cfg.stage = value.to_string(),
        "tf_min_mc" => cfg.tf_min_mc = value.parse().context("invalid u64")?,
        "tf_max_mc" => cfg.tf_max_mc = value.parse().context("invalid u64")?,
        "tf_min_holders" => cfg.tf_min_holders = value.parse().context("invalid u32")?,
        "tf_max_dev_hold" => cfg.tf_max_dev_hold = value.parse().context("invalid u32")?,
        "tf_max_bundler" => cfg.tf_max_bundler = value.parse().context("invalid u32")?,
        "tf_max_sniper" => cfg.tf_max_sniper = value.parse().context("invalid u32")?,
        "tf_max_insider" => cfg.tf_max_insider = value.parse().context("invalid u32")?,
        "tf_max_top10" => cfg.tf_max_top10 = value.parse().context("invalid u32")?,
        "tf_max_fresh" => cfg.tf_max_fresh = value.parse().context("invalid u32")?,
        "tf_min_tx" => cfg.tf_min_tx = value.parse().context("invalid u32")?,
        "tf_min_buy_tx" => cfg.tf_min_buy_tx = value.parse().context("invalid u32")?,
        "tf_min_age" => cfg.tf_min_age = value.parse().context("invalid u32")?,
        "tf_max_age" => cfg.tf_max_age = value.parse().context("invalid u32")?,
        "tf_min_vol" => cfg.tf_min_vol = value.parse().context("invalid u64")?,
        "cf_min_bs_ratio" => cfg.cf_min_bs_ratio = value.parse().context("invalid f64")?,
        "cf_min_vol_mc_pct" => cfg.cf_min_vol_mc_pct = value.parse().context("invalid f64")?,
        "cf_max_top10" => cfg.cf_max_top10 = value.parse().context("invalid f64")?,
        "ds_max_dev_hold" => cfg.ds_max_dev_hold = value.parse().context("invalid f64")?,
        "ds_max_bundler_ath" => cfg.ds_max_bundler_ath = value.parse().context("invalid f64")?,
        "ds_max_bundler_count" => {
            cfg.ds_max_bundler_count = value.parse().context("invalid u32")?
        }
        "scalp_sol" => cfg.scalp_sol = value.parse().context("invalid f64")?,
        "minimum_sol" => cfg.minimum_sol = value.parse().context("invalid f64")?,
        "max_sol" => cfg.max_sol = value.parse().context("invalid f64")?,
        "max_positions" => cfg.max_positions = value.parse().context("invalid usize")?,
        "slippage_scalp" => cfg.slippage_scalp = value.parse().context("invalid u32")?,
        "slippage_minimum" => cfg.slippage_minimum = value.parse().context("invalid u32")?,
        "tp1_pct" => cfg.tp1_pct = value.parse().context("invalid f64")?,
        "tp2_pct" => cfg.tp2_pct = value.parse().context("invalid f64")?,
        "sl_scalp" => cfg.sl_scalp = value.parse().context("invalid f64")?,
        "sl_hot" => cfg.sl_hot = value.parse().context("invalid f64")?,
        "sl_quiet" => cfg.sl_quiet = value.parse().context("invalid f64")?,
        "trailing_pct" => cfg.trailing_pct = value.parse().context("invalid f64")?,
        "max_hold_min" => cfg.max_hold_min = value.parse().context("invalid u64")?,
        "max_consec_loss" => cfg.max_consec_loss = value.parse().context("invalid u32")?,
        "pause_loss_sol" => cfg.pause_loss_sol = value.parse().context("invalid f64")?,
        "stop_loss_sol" => cfg.stop_loss_sol = value.parse().context("invalid f64")?,
        "tick_interval_secs" => cfg.tick_interval_secs = value.parse().context("invalid u64")?,
        _ => bail!("Unknown config key '{key}'. Use 'scanner config' to see available keys."),
    }

    cfg.save()?;
    output::success(json!({
        "updated": key,
        "value": value,
    }));
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────

/// Append a timestamped line to the log file.
fn write_log(path: &std::path::Path, msg: &str) {
    use std::io::Write;
    let line = format!("[{}] {}\n", chrono::Utc::now().to_rfc3339(), msg);
    // Also write to stderr for interactive use
    eprint!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Build server-side filter params for memepump tokenList API.
fn build_filter_params(cfg: &ScannerConfig) -> Value {
    json!({
        "chainIndex": engine::CHAIN_INDEX,
        "stage": cfg.stage,
        "minMarketCap": cfg.tf_min_mc.to_string(),
        "maxMarketCap": cfg.tf_max_mc.to_string(),
        "minTotalHolders": cfg.tf_min_holders.to_string(),
        "maxDevHoldingsPercent": cfg.tf_max_dev_hold.to_string(),
        "maxBundlersPercent": cfg.tf_max_bundler.to_string(),
        "maxSnipersPercent": cfg.tf_max_sniper.to_string(),
        "maxInsidersPercent": cfg.tf_max_insider.to_string(),
        "maxTop10HoldingsPercent": cfg.tf_max_top10.to_string(),
        "maxFreshWalletsPercent": cfg.tf_max_fresh.to_string(),
        "minTxCount1h": cfg.tf_min_tx.to_string(),
        "minBuyTxCount1h": cfg.tf_min_buy_tx.to_string(),
        "minTokenAge": cfg.tf_min_age.to_string(),
        "maxTokenAge": cfg.tf_max_age.to_string(),
        "minVolumeUsd1h": cfg.tf_min_vol.to_string(),
    })
}

/// Extract token address from API response item.
fn token_addr(v: &Value) -> String {
    v["tokenAddress"]
        .as_str()
        .or_else(|| v["tokenContractAddress"].as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse a TokenData from the memepump tokenList API response item.
/// Fields are nested: `market.*` for trading data, `tags.*` for holder analytics.
fn parse_token_data(v: &Value) -> engine::TokenData {
    let market = &v["market"];
    let tags = &v["tags"];
    engine::TokenData {
        token_address: token_addr(v),
        symbol: v["symbol"].as_str().unwrap_or("???").to_string(),
        name: v["name"].as_str().unwrap_or("").to_string(),
        market_cap: engine::safe_float(&market["marketCapUsd"], 0.0),
        volume_1h: engine::safe_float(&market["volumeUsd1h"], 0.0),
        buy_tx_1h: engine::safe_u32(&market["buyTxCount1h"], 0),
        sell_tx_1h: engine::safe_u32(&market["sellTxCount1h"], 0),
        holders: engine::safe_u32(&tags["totalHolders"], 0),
        top10_pct: engine::safe_float(&tags["top10HoldingsPercent"], 0.0),
        dev_hold_pct: engine::safe_float(&tags["devHoldingsPercent"], 0.0),
        bundler_pct: engine::safe_float(&tags["bundlersPercent"], 0.0),
        sniper_pct: engine::safe_float(&tags["snipersPercent"], 0.0),
        insider_pct: engine::safe_float(&tags["insidersPercent"], 0.0),
        fresh_wallet_pct: engine::safe_float(&tags["freshWalletsPercent"], 0.0),
        created_timestamp: engine::safe_u32(&v["createdTimestamp"], 0) as u64,
    }
}

/// Extract 1m tx count from response item (market data).
fn token_tx_1m(v: &Value) -> u32 {
    engine::safe_u32(&v["market"]["txCount1m"], 0)
}

/// Extract price from response item (market data).
fn token_price(v: &Value) -> f64 {
    engine::safe_float(
        &v["market"]["priceUsd"],
        engine::safe_float(&v["market"]["price"], 0.0),
    )
}
