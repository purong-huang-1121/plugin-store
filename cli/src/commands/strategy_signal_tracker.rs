//! CLI commands for Signal Tracker strategy.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::notifier::{Notifier, NotifyLevel};
use crate::output;
use crate::strategy::signal_tracker::client::SignalClient;
use crate::strategy::signal_tracker::config::SignalTrackerConfig;
use crate::strategy::signal_tracker::engine::{self, Position, Trade};
use crate::strategy::signal_tracker::state::SignalTrackerState;

#[derive(Subcommand)]
pub enum SignalTrackerCommand {
    /// Execute one tick: fetch signals, check exits, open new positions
    Tick {
        /// Simulate without executing swaps
        #[arg(long)]
        dry_run: bool,
    },
    /// Start the bot in foreground (tick every 20 seconds)
    Start {
        /// Simulate without executing swaps
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop a running bot via PID file
    Stop,
    /// Show current state, positions, and PnL
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
    /// Market analysis (current signals, wallet types)
    Analyze,
    /// Show all strategy parameters and file paths
    Config,
    /// Set a config parameter (e.g. signal-tracker set max_positions 8)
    Set {
        /// Parameter name
        key: String,
        /// New value
        value: String,
    },
}

pub async fn execute(cmd: SignalTrackerCommand) -> Result<()> {
    let notifier = Notifier::from_env("📡 Signal Tracker");
    match cmd {
        SignalTrackerCommand::Tick { dry_run } => cmd_tick(dry_run, &notifier).await,
        SignalTrackerCommand::Start { dry_run } => cmd_start(dry_run, &notifier).await,
        SignalTrackerCommand::Stop => cmd_stop().await,
        SignalTrackerCommand::Status => cmd_status().await,
        SignalTrackerCommand::Report => cmd_report().await,
        SignalTrackerCommand::History { limit } => cmd_history(limit).await,
        SignalTrackerCommand::Reset { force } => cmd_reset(force).await,
        SignalTrackerCommand::Analyze => cmd_analyze().await,
        SignalTrackerCommand::Config => cmd_config().await,
        SignalTrackerCommand::Set { key, value } => cmd_set(&key, &value).await,
    }
}

// ── tick ──────────────────────────────────────────────────────────────

async fn cmd_tick(dry_run: bool, notifier: &Notifier) -> Result<()> {
    let mut state = SignalTrackerState::load()?;
    state.dry_run = dry_run;

    // Circuit breaker
    if let Some(reason) = state.check_circuit_breaker() {
        notifier
            .notify(NotifyLevel::Error, &format!("Circuit breaker: {reason}"))
            .await;
        output::error(&reason);
        return Ok(());
    }

    // Check if previously stopped
    if state.stopped {
        let reason = state
            .stop_reason
            .clone()
            .unwrap_or_else(|| "previously stopped".to_string());
        output::error(&format!(
            "Bot stopped: {reason}. Use 'signal-tracker reset --force' to restart."
        ));
        return Ok(());
    }

    // Check if paused
    if state.is_paused() {
        let until = state.paused_until.unwrap_or(0);
        let remaining = until - chrono::Utc::now().timestamp();
        output::error(&format!("Bot paused. Resuming in {remaining}s."));
        return Ok(());
    }

    // Session risk check
    if let Some((reason, pause_secs)) = engine::check_session_risk(
        state.stats.consecutive_losses,
        state.stats.cumulative_loss_sol,
    ) {
        notifier.notify(NotifyLevel::Warning, &reason).await;
        log_to_file(&reason);
        if pause_secs == u64::MAX {
            state.stopped = true;
            state.stop_reason = Some(reason.clone());
            state.save()?;
            output::error(&format!(
                "Bot permanently stopped: {reason}. Use 'signal-tracker reset --force' to restart."
            ));
        } else {
            let until = chrono::Utc::now().timestamp() + pause_secs as i64;
            state.paused_until = Some(until);
            state.save()?;
            output::error(&format!("Bot paused: {reason}. Resuming in {pause_secs}s."));
        }
        return Ok(());
    }

    // Create client
    let client = if dry_run {
        SignalClient::new_read_only()?
    } else {
        SignalClient::new()?
    };

    let now = chrono::Utc::now();
    let now_ts = now.timestamp();
    let now_str = now.to_rfc3339();

    let mut actions = Vec::new();

    // ── Check exits for existing positions ──
    let position_addrs: Vec<String> = state.positions.keys().cloned().collect();
    for addr in position_addrs {
        let price_info = match client.fetch_price_info(&addr).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "[signal-tracker] failed to fetch price-info for {}: {e:#}",
                    addr
                );
                continue;
            }
        };

        let price = engine::safe_float(&price_info["price"], 0.0);
        if price <= 0.0 {
            continue;
        }

        let liq = engine::safe_float(&price_info["liquidity"], 0.0);
        let mc = engine::safe_float(&price_info["marketCap"], 0.0);

        let exit_signal = {
            let pos = state.positions.get_mut(&addr).unwrap();
            engine::check_exits(pos, price, liq, mc, now_ts)
        };

        if let Some(signal) = exit_signal {
            let pos = state.positions.get(&addr).unwrap().clone();
            let pnl_pct = if pos.buy_price > 0.0 {
                (price - pos.buy_price) / pos.buy_price * 100.0
            } else {
                0.0
            };
            let net_pnl_pct = pnl_pct - pos.breakeven_pct;
            let sell_fraction = signal.sell_pct;
            let is_full_exit = (sell_fraction - 1.0).abs() < 0.01;

            // Estimate SOL return for this sell
            let sell_sol = pos.buy_amount_sol * sell_fraction * (1.0 + pnl_pct / 100.0);
            let pnl_sol = sell_sol - pos.buy_amount_sol * sell_fraction;

            // Execute sell
            let tx_hash = if !dry_run {
                let amount_raw = if is_full_exit {
                    "0".to_string()
                } else {
                    // Estimate raw token amount for partial sell — use "0" as fallback
                    "0".to_string()
                };
                match client.sell_token(&addr, &amount_raw).await {
                    Ok(sr) => sr.tx_hash.unwrap_or_default(),
                    Err(e) => {
                        eprintln!("[signal-tracker] sell failed for {}: {e:#}", pos.symbol);
                        state.errors.consecutive_errors += 1;
                        state.errors.last_error_time = Some(now_str.clone());
                        state.errors.last_error_msg = Some(format!("{e:#}"));
                        actions.push(json!({
                            "action": "exit_failed",
                            "symbol": pos.symbol,
                            "label": pos.label,
                            "tier": pos.tier,
                            "reason": signal.reason,
                            "error": format!("{e:#}"),
                        }));
                        continue;
                    }
                }
            } else {
                "DRY_RUN".to_string()
            };

            // Update stats
            state.stats.total_returned_sol += sell_sol;
            state.stats.session_pnl_sol += pnl_sol;
            state.stats.total_sells += 1;
            state.stats.successful_trades += 1;

            if pnl_sol >= 0.0 {
                state.record_win();
            } else {
                state.record_loss(pnl_sol.abs());
            }

            state.push_trade(Trade {
                time: now_str.clone(),
                symbol: pos.symbol.clone(),
                token_address: addr.clone(),
                label: pos.label.clone(),
                tier: pos.tier.clone(),
                action: "SELL".to_string(),
                price,
                amount_sol: sell_sol,
                entry_mc: Some(pos.entry_mc),
                exit_mc: Some(mc),
                exit_reason: Some(signal.reason.clone()),
                pnl_pct: Some(pnl_pct),
                net_pnl_pct: Some(net_pnl_pct),
                pnl_sol: Some(pnl_sol),
                tx_hash: tx_hash.clone(),
            });

            {
                let level = if pnl_sol >= 0.0 {
                    NotifyLevel::Success
                } else {
                    NotifyLevel::Warning
                };
                let msg = format!(
                    "SELL {} [{}/{}]\n{}\nPnL: {pnl_pct:+.1}% (net {net_pnl_pct:+.1}%) = {pnl_sol:+.4} SOL\ntx: {tx_hash}",
                    pos.symbol, pos.label, pos.tier, signal.reason
                );
                log_to_file(&msg);
                notifier.notify(level, &msg).await;
            }

            actions.push(json!({
                "action": "exit",
                "symbol": pos.symbol,
                "label": pos.label,
                "tier": pos.tier,
                "reason": signal.reason,
                "sell_pct": sell_fraction,
                "pnl_pct": format!("{pnl_pct:+.1}%"),
                "net_pnl_pct": format!("{net_pnl_pct:+.1}%"),
                "pnl_sol": format!("{pnl_sol:+.4}"),
                "tx_hash": tx_hash,
            }));

            if is_full_exit {
                state.positions.remove(&addr);
            } else {
                // Partial sell — update TP tier was already advanced inside check_exits
                // Position stays open
            }
        }
    }

    // ── Fetch signals ──
    let signals = match client.fetch_signals().await {
        Ok(s) => s,
        Err(e) => {
            state.errors.consecutive_errors += 1;
            state.errors.last_error_time = Some(now_str.clone());
            state.errors.last_error_msg = Some(format!("{e:#}"));
            state.save()?;
            bail!("Failed to fetch signals: {e:#}");
        }
    };

    // ── Scan for new entries ──
    for signal in &signals {
        let addr = signal["token"]["tokenAddress"]
            .as_str()
            .or_else(|| signal["token"]["tokenContractAddress"].as_str())
            .unwrap_or("")
            .to_string();

        if addr.is_empty() {
            continue;
        }

        let symbol = signal["token"]["symbol"]
            .as_str()
            .or_else(|| signal["token"]["name"].as_str())
            .unwrap_or("?")
            .to_string();

        let wallet_type = signal["walletType"].as_str().unwrap_or("?");
        let label = engine::wallet_type_label(wallet_type).to_string();

        // Skip if already known or holding
        if state.known_tokens.contains(&addr) || state.positions.contains_key(&addr) {
            continue;
        }

        state.known_tokens.insert(addr.clone());
        state.trim_known_tokens();

        // Position limit
        if state.positions.len() >= engine::MAX_POSITIONS {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": format!("max positions ({}) reached", engine::MAX_POSITIONS),
            }));
            continue;
        }

        // Signal pre-filter
        let (prefilter_passed, prefilter_reasons) = engine::run_signal_prefilter(signal);
        if !prefilter_passed {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": format!("prefilter: {}", prefilter_reasons.iter().take(3).cloned().collect::<Vec<_>>().join("; ")),
            }));
            continue;
        }

        // Determine position tier from wallet count
        let wallet_count = engine::safe_int(&signal["triggerWalletCount"], 0) as u32;
        let (tier, sol_amount) = engine::calc_position_tier(wallet_count);

        // SOL balance check
        let sol_balance = match client.fetch_sol_balance().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[signal-tracker] balance fetch error: {e:#}");
                0.0
            }
        };
        if sol_balance < sol_amount + engine::GAS_RESERVE_SOL {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "tier": tier,
                "reason": format!("insufficient SOL balance ({sol_balance:.4} < {}+{})", sol_amount, engine::GAS_RESERVE_SOL),
            }));
            continue;
        }

        // Fetch price info for safety checks
        let price_info = match client.fetch_price_info(&addr).await {
            Ok(p) => p,
            Err(e) => {
                actions.push(json!({
                    "action": "skip",
                    "symbol": symbol,
                    "label": label,
                    "reason": format!("failed to fetch price-info: {e:#}"),
                }));
                continue;
            }
        };

        // Safety checks from price-info
        let (safety_passed, safety_reasons) = engine::run_safety_checks(&price_info);
        if !safety_passed {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": format!("safety: {}", safety_reasons.iter().take(3).cloned().collect::<Vec<_>>().join("; ")),
            }));
            continue;
        }

        // Fetch 1m candles and check pump
        let candles = client.fetch_candles_1m(&addr).await.unwrap_or_default();
        if let Some(pump_reason) = engine::check_k1_pump(&candles) {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": format!("k1_pump: {pump_reason}"),
            }));
            continue;
        }

        // Fetch dev info and bundle info
        let dev_info = client.fetch_dev_info(&addr).await.unwrap_or_default();
        let bundle_info = client.fetch_bundle_info(&addr).await.unwrap_or_default();

        let (dev_passed, dev_reasons) = engine::run_dev_bundler_checks(&dev_info, &bundle_info);
        if !dev_passed {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": format!("dev_bundler: {}", dev_reasons.iter().take(3).cloned().collect::<Vec<_>>().join("; ")),
            }));
            continue;
        }

        // Fetch quote for honeypot check
        let quote = client
            .fetch_quote(&addr, sol_amount)
            .await
            .unwrap_or_default();
        if let Some(hp_reason) = engine::check_honeypot(&quote) {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": format!("honeypot: {hp_reason}"),
            }));
            continue;
        }

        // Get current price and MC
        let price = engine::safe_float(&price_info["price"], 0.0);
        if price <= 0.0 {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "label": label,
                "reason": "invalid price (zero)",
            }));
            continue;
        }
        let entry_mc = engine::safe_float(&price_info["marketCap"], 0.0);
        let breakeven_pct = engine::calc_breakeven(sol_amount);

        // Execute buy
        let tx_hash = if !dry_run {
            match client.buy_token(&addr, sol_amount).await {
                Ok(sr) => sr.tx_hash.unwrap_or_default(),
                Err(e) => {
                    state.errors.consecutive_errors += 1;
                    state.errors.last_error_time = Some(now_str.clone());
                    state.errors.last_error_msg = Some(format!("{e:#}"));
                    state.stats.failed_trades += 1;
                    notifier
                        .notify(NotifyLevel::Error, &format!("BUY FAILED {symbol}: {e:#}"))
                        .await;
                    log_to_file(&format!(
                        "BUY_FAILED {} [{}/{}]: {e:#}",
                        symbol, label, tier
                    ));
                    actions.push(json!({
                        "action": "buy_failed",
                        "symbol": symbol,
                        "label": label,
                        "tier": tier,
                        "error": format!("{e:#}"),
                    }));
                    continue;
                }
            }
        } else {
            "DRY_RUN".to_string()
        };

        // Record position
        state.positions.insert(
            addr.clone(),
            Position {
                token_address: addr.clone(),
                symbol: symbol.clone(),
                label: label.clone(),
                tier: tier.to_string(),
                buy_price: price,
                buy_amount_sol: sol_amount,
                buy_time: now_str.clone(),
                breakeven_pct,
                peak_price: price,
                peak_pnl_pct: 0.0,
                trailing_active: false,
                tp_tier: 0,
                entry_mc,
                tx_hash: tx_hash.clone(),
            },
        );

        state.stats.total_invested_sol += sol_amount;
        state.stats.total_buys += 1;
        state.stats.successful_trades += 1;
        state.errors.consecutive_errors = 0;

        state.push_trade(Trade {
            time: now_str.clone(),
            symbol: symbol.clone(),
            token_address: addr.clone(),
            label: label.clone(),
            tier: tier.to_string(),
            action: "BUY".to_string(),
            price,
            amount_sol: sol_amount,
            entry_mc: Some(entry_mc),
            exit_mc: None,
            exit_reason: None,
            pnl_pct: None,
            net_pnl_pct: None,
            pnl_sol: None,
            tx_hash: tx_hash.clone(),
        });

        {
            let msg = format!(
                "BUY {} [{}/{}]\n{sol_amount} SOL @ ${price:.10}\nMC: ${entry_mc:.0} | Wallets: {wallet_count}\ntx: {tx_hash}",
                symbol, label, tier
            );
            log_to_file(&msg);
            notifier.notify(NotifyLevel::Success, &msg).await;
        }

        actions.push(json!({
            "action": "buy",
            "symbol": symbol,
            "label": label,
            "tier": tier,
            "price": price,
            "amount_sol": sol_amount,
            "breakeven_pct": format!("{breakeven_pct:.2}%"),
            "entry_mc": entry_mc,
            "wallet_count": wallet_count,
            "tx_hash": tx_hash,
        }));
    }

    state.save()?;

    output::success(json!({
        "tick_time": now_str,
        "positions": state.positions.len(),
        "signals_seen": signals.len(),
        "session_pnl_sol": state.stats.session_pnl_sol,
        "consecutive_losses": state.stats.consecutive_losses,
        "cumulative_loss_sol": state.stats.cumulative_loss_sol,
        "actions": actions,
        "dry_run": dry_run,
    }));
    Ok(())
}

// ── start ─────────────────────────────────────────────────────────────

async fn cmd_config() -> Result<()> {
    let cfg = SignalTrackerConfig::load()?;
    let path = SignalTrackerConfig::config_path();
    let is_custom = path.exists();

    output::success(json!({
        "config_file": path.to_string_lossy(),
        "is_custom": is_custom,
        "log_file": SignalTrackerConfig::log_path().to_string_lossy(),
        "state_file": SignalTrackerState::state_path().to_string_lossy(),
        "pid_file": SignalTrackerState::pid_path().to_string_lossy(),
        "parameters": {
            "signal_filter": {
                "signal_labels": cfg.signal_labels,
                "min_wallet_count": cfg.min_wallet_count,
                "max_sell_ratio": cfg.max_sell_ratio,
            },
            "safety": {
                "min_mcap": cfg.min_mcap,
                "min_liquidity": cfg.min_liquidity,
                "min_holders": cfg.min_holders,
                "min_liq_mc_ratio": cfg.min_liq_mc_ratio,
                "max_top10_holder_pct": cfg.max_top10_holder_pct,
                "min_lp_burn": cfg.min_lp_burn,
                "min_holder_density": cfg.min_holder_density,
                "max_k1_pump_pct": cfg.max_k1_pump_pct,
            },
            "dev_bundler": {
                "dev_max_launched": cfg.dev_max_launched,
                "dev_max_hold_pct": cfg.dev_max_hold_pct,
                "bundle_max_ath_pct": cfg.bundle_max_ath_pct,
                "bundle_max_count": cfg.bundle_max_count,
            },
            "position": {
                "position_high_sol": cfg.position_high_sol,
                "position_mid_sol": cfg.position_mid_sol,
                "position_low_sol": cfg.position_low_sol,
                "wallet_high_threshold": cfg.wallet_high_threshold,
                "wallet_mid_threshold": cfg.wallet_mid_threshold,
                "max_positions": cfg.max_positions,
                "slippage_pct": cfg.slippage_pct,
                "gas_reserve_sol": cfg.gas_reserve_sol,
            },
            "cost": {
                "fixed_cost_sol": cfg.fixed_cost_sol,
                "cost_per_leg_pct": cfg.cost_per_leg_pct,
                "breakeven_high": format!("{:.1}%", cfg.calc_breakeven(cfg.position_high_sol)),
                "breakeven_mid": format!("{:.1}%", cfg.calc_breakeven(cfg.position_mid_sol)),
                "breakeven_low": format!("{:.1}%", cfg.calc_breakeven(cfg.position_low_sol)),
            },
            "take_profit": {
                "tp1_pct": cfg.tp1_pct,
                "tp1_sell": cfg.tp1_sell,
                "tp2_pct": cfg.tp2_pct,
                "tp2_sell": cfg.tp2_sell,
                "tp3_pct": cfg.tp3_pct,
                "tp3_sell": cfg.tp3_sell,
                "trail_activate_pct": cfg.trail_activate_pct,
                "trail_distance_pct": cfg.trail_distance_pct,
            },
            "stop_loss": {
                "sl_multiplier": cfg.sl_multiplier,
                "liq_emergency": cfg.liq_emergency,
                "time_stop_hours": cfg.time_stop_hours,
            },
            "session_risk": {
                "max_consec_loss": cfg.max_consec_loss,
                "pause_consec_sec": cfg.pause_consec_sec,
                "session_loss_limit_sol": cfg.session_loss_limit_sol,
                "session_loss_pause_sec": cfg.session_loss_pause_sec,
                "session_stop_sol": cfg.session_stop_sol,
            },
            "tick_interval_secs": cfg.tick_interval_secs,
            "max_consecutive_errors": cfg.max_consecutive_errors,
            "cooldown_after_errors": cfg.cooldown_after_errors,
        }
    }));
    Ok(())
}

async fn cmd_set(key: &str, value: &str) -> Result<()> {
    let mut cfg = SignalTrackerConfig::load()?;

    match key {
        "signal_labels" => cfg.signal_labels = value.to_string(),
        "min_wallet_count" => cfg.min_wallet_count = value.parse().context("invalid u32")?,
        "max_sell_ratio" => cfg.max_sell_ratio = value.parse().context("invalid f64")?,
        "min_mcap" => cfg.min_mcap = value.parse().context("invalid f64")?,
        "min_liquidity" => cfg.min_liquidity = value.parse().context("invalid f64")?,
        "min_holders" => cfg.min_holders = value.parse().context("invalid i64")?,
        "min_liq_mc_ratio" => cfg.min_liq_mc_ratio = value.parse().context("invalid f64")?,
        "max_top10_holder_pct" => {
            cfg.max_top10_holder_pct = value.parse().context("invalid f64")?
        }
        "min_lp_burn" => cfg.min_lp_burn = value.parse().context("invalid f64")?,
        "min_holder_density" => cfg.min_holder_density = value.parse().context("invalid f64")?,
        "max_k1_pump_pct" => cfg.max_k1_pump_pct = value.parse().context("invalid f64")?,
        "dev_max_launched" => cfg.dev_max_launched = value.parse().context("invalid i64")?,
        "dev_max_hold_pct" => cfg.dev_max_hold_pct = value.parse().context("invalid f64")?,
        "bundle_max_ath_pct" => cfg.bundle_max_ath_pct = value.parse().context("invalid f64")?,
        "bundle_max_count" => cfg.bundle_max_count = value.parse().context("invalid i64")?,
        "position_high_sol" => cfg.position_high_sol = value.parse().context("invalid f64")?,
        "position_mid_sol" => cfg.position_mid_sol = value.parse().context("invalid f64")?,
        "position_low_sol" => cfg.position_low_sol = value.parse().context("invalid f64")?,
        "wallet_high_threshold" => {
            cfg.wallet_high_threshold = value.parse().context("invalid u32")?
        }
        "wallet_mid_threshold" => {
            cfg.wallet_mid_threshold = value.parse().context("invalid u32")?
        }
        "max_positions" => cfg.max_positions = value.parse().context("invalid usize")?,
        "slippage_pct" => cfg.slippage_pct = value.to_string(),
        "gas_reserve_sol" => cfg.gas_reserve_sol = value.parse().context("invalid f64")?,
        "fixed_cost_sol" => cfg.fixed_cost_sol = value.parse().context("invalid f64")?,
        "cost_per_leg_pct" => cfg.cost_per_leg_pct = value.parse().context("invalid f64")?,
        "tp1_pct" => cfg.tp1_pct = value.parse().context("invalid f64")?,
        "tp1_sell" => cfg.tp1_sell = value.parse().context("invalid f64")?,
        "tp2_pct" => cfg.tp2_pct = value.parse().context("invalid f64")?,
        "tp2_sell" => cfg.tp2_sell = value.parse().context("invalid f64")?,
        "tp3_pct" => cfg.tp3_pct = value.parse().context("invalid f64")?,
        "tp3_sell" => cfg.tp3_sell = value.parse().context("invalid f64")?,
        "trail_activate_pct" => cfg.trail_activate_pct = value.parse().context("invalid f64")?,
        "trail_distance_pct" => cfg.trail_distance_pct = value.parse().context("invalid f64")?,
        "sl_multiplier" => cfg.sl_multiplier = value.parse().context("invalid f64")?,
        "liq_emergency" => cfg.liq_emergency = value.parse().context("invalid f64")?,
        "time_stop_hours" => cfg.time_stop_hours = value.parse().context("invalid f64")?,
        "max_consec_loss" => cfg.max_consec_loss = value.parse().context("invalid u32")?,
        "pause_consec_sec" => cfg.pause_consec_sec = value.parse().context("invalid u64")?,
        "session_loss_limit_sol" => {
            cfg.session_loss_limit_sol = value.parse().context("invalid f64")?
        }
        "session_loss_pause_sec" => {
            cfg.session_loss_pause_sec = value.parse().context("invalid u64")?
        }
        "session_stop_sol" => cfg.session_stop_sol = value.parse().context("invalid f64")?,
        "tick_interval_secs" => cfg.tick_interval_secs = value.parse().context("invalid u64")?,
        "max_consecutive_errors" => {
            cfg.max_consecutive_errors = value.parse().context("invalid u32")?
        }
        "cooldown_after_errors" => {
            cfg.cooldown_after_errors = value.parse().context("invalid u64")?
        }
        _ => bail!(
            "Unknown parameter '{}'. Use 'signal-tracker config' to see available parameters.",
            key
        ),
    }

    cfg.save()?;
    output::success(json!({
        "message": format!("Set {} = {}", key, value),
        "config_file": SignalTrackerConfig::config_path().to_string_lossy(),
    }));
    Ok(())
}

/// Append a log line to the log file.
fn log_to_file(msg: &str) {
    let path = SignalTrackerConfig::log_path();
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

async fn cmd_start(dry_run: bool, notifier: &Notifier) -> Result<()> {
    // Show config before starting
    let cfg = SignalTrackerConfig::load()?;
    eprintln!("Signal Tracker v3.0 — Current Parameters:");
    eprintln!("  Config: {}", SignalTrackerConfig::config_path().display());
    eprintln!("  Log:    {}", SignalTrackerConfig::log_path().display());
    eprintln!("  State:  {}", SignalTrackerState::state_path().display());
    eprintln!("  Mode:   {}", if dry_run { "DRY RUN" } else { "LIVE" });
    eprintln!("  Tick:   {}s", cfg.tick_interval_secs);
    eprintln!(
        "  Positions: max {} | high={} mid={} low={} SOL",
        cfg.max_positions, cfg.position_high_sol, cfg.position_mid_sol, cfg.position_low_sol
    );
    eprintln!(
        "  Safety: MC>=${} Liq>=${} Top10<={:.0}% LP>={:.0}%",
        cfg.min_mcap, cfg.min_liquidity, cfg.max_top10_holder_pct, cfg.min_lp_burn
    );
    eprintln!(
        "  TP: +{}%/+{}%/+{}% | SL: {:.0}% | Trail: +{}%/-{}%",
        cfg.tp1_pct,
        cfg.tp2_pct,
        cfg.tp3_pct,
        (cfg.sl_multiplier - 1.0) * 100.0,
        cfg.trail_activate_pct,
        cfg.trail_distance_pct
    );
    eprintln!(
        "  Session: {} consec loss → pause {}s | {} SOL → stop",
        cfg.max_consec_loss, cfg.pause_consec_sec, cfg.session_stop_sol
    );

    log_to_file(&format!(
        "START mode={} dry_run={dry_run}",
        if dry_run { "DRY" } else { "LIVE" }
    ));
    eprintln!("---");
    eprintln!(
        "Mode: {} | Tick interval: {}s",
        if dry_run { "DRY RUN" } else { "LIVE" },
        cfg.tick_interval_secs
    );

    notifier
        .notify(
            NotifyLevel::Info,
            &format!(
                "Starting ({} mode)\nPositions: max {} | {}/{}/{} SOL\nTP: +{}%/+{}%/+{}% | SL: {:.0}%",
                if dry_run { "DRY RUN" } else { "LIVE" },
                cfg.max_positions,
                cfg.position_high_sol, cfg.position_mid_sol, cfg.position_low_sol,
                cfg.tp1_pct, cfg.tp2_pct, cfg.tp3_pct,
                (cfg.sl_multiplier - 1.0) * 100.0
            ),
        )
        .await;

    let pid_path = SignalTrackerState::pid_path();

    // Check if already running
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
        let pid: i32 = pid_str.trim().parse().unwrap_or(0);
        #[cfg(unix)]
        if pid > 0 && unsafe { libc::kill(pid, 0) } == 0 {
            bail!(
                "Signal tracker already running (PID {}). Use 'signal-tracker stop' first.",
                pid
            );
        }
    }

    // Validate credentials before starting
    if dry_run {
        let _ = SignalClient::new_read_only()?;
    } else {
        let _ = SignalClient::new()?;
    }

    // Write PID file
    let dir = pid_path.parent().context("no parent dir")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    eprintln!(
        "[{}] Signal tracker started (PID {}) dry_run={dry_run}",
        chrono::Utc::now().to_rfc3339(),
        std::process::id()
    );

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

    // Main loop
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        if let Err(e) = cmd_tick(dry_run, notifier).await {
            eprintln!("[{}] Tick error: {:#}", chrono::Utc::now().to_rfc3339(), e);
        }

        // Sleep in small increments to check shutdown
        for _ in 0..(engine::TICK_INTERVAL_SECS / 2) {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    notifier.notify(NotifyLevel::Info, "Bot stopped").await;
    log_to_file("STOP");
    eprintln!(
        "[{}] Signal tracker stopped",
        chrono::Utc::now().to_rfc3339()
    );
    output::success(json!({ "message": "Signal tracker stopped" }));
    Ok(())
}

// ── stop ──────────────────────────────────────────────────────────────

async fn cmd_stop() -> Result<()> {
    let pid_path = SignalTrackerState::pid_path();
    if !pid_path.exists() {
        bail!("No running bot found (PID file missing)");
    }
    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    if pid <= 0 {
        let _ = std::fs::remove_file(&pid_path);
        bail!("Invalid PID in file");
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    let _ = std::fs::remove_file(&pid_path);
    output::success(json!({
        "message": format!("Sent SIGTERM to PID {pid}"),
        "pid": pid,
    }));
    Ok(())
}

// ── status ────────────────────────────────────────────────────────────

async fn cmd_status() -> Result<()> {
    let state = SignalTrackerState::load()?;

    let pid_path = SignalTrackerState::pid_path();
    let bot_running = if pid_path.exists() {
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
    };

    let positions: Vec<_> = state
        .positions
        .values()
        .map(|p| {
            json!({
                "symbol": p.symbol,
                "token_address": p.token_address,
                "label": p.label,
                "tier": p.tier,
                "buy_price": p.buy_price,
                "buy_amount_sol": p.buy_amount_sol,
                "buy_time": p.buy_time,
                "breakeven_pct": p.breakeven_pct,
                "peak_pnl_pct": p.peak_pnl_pct,
                "trailing_active": p.trailing_active,
                "tp_tier": p.tp_tier,
                "entry_mc": p.entry_mc,
            })
        })
        .collect();

    let paused = state.is_paused();
    let paused_remaining_secs = if paused {
        state
            .paused_until
            .map(|u| u - chrono::Utc::now().timestamp())
            .unwrap_or(0)
    } else {
        0
    };

    output::success(json!({
        "bot_running": bot_running,
        "stopped": state.stopped,
        "stop_reason": state.stop_reason,
        "paused": paused,
        "paused_remaining_secs": paused_remaining_secs,
        "positions": positions,
        "position_count": state.positions.len(),
        "max_positions": engine::MAX_POSITIONS,
        "session_pnl_sol": state.stats.session_pnl_sol,
        "consecutive_losses": state.stats.consecutive_losses,
        "cumulative_loss_sol": state.stats.cumulative_loss_sol,
        "known_tokens_count": state.known_tokens.len(),
        "consecutive_errors": state.errors.consecutive_errors,
        "last_error": state.errors.last_error_msg,
        "dry_run": state.dry_run,
    }));
    Ok(())
}

// ── report ────────────────────────────────────────────────────────────

async fn cmd_report() -> Result<()> {
    let state = SignalTrackerState::load()?;

    let buys: Vec<_> = state.trades.iter().filter(|t| t.action == "BUY").collect();
    let sells: Vec<_> = state.trades.iter().filter(|t| t.action == "SELL").collect();

    let total_pnl_sol: f64 = sells.iter().filter_map(|t| t.pnl_sol).sum();
    let win_count = sells
        .iter()
        .filter(|t| t.net_pnl_pct.unwrap_or(-1.0) > 0.0)
        .count();
    let loss_count = sells
        .iter()
        .filter(|t| t.net_pnl_pct.unwrap_or(-1.0) <= 0.0)
        .count();
    let win_rate = if !sells.is_empty() {
        win_count as f64 / sells.len() as f64 * 100.0
    } else {
        0.0
    };

    let total_invested_sol = state.stats.total_invested_sol;
    let total_returned_sol = state.stats.total_returned_sol;
    let gross_pnl_sol = total_returned_sol - total_invested_sol;

    // Label breakdown
    let mut smart_money_sells = 0usize;
    let mut kol_sells = 0usize;
    let mut whale_sells = 0usize;
    for t in &sells {
        match t.label.as_str() {
            "SmartMoney" => smart_money_sells += 1,
            "KOL" => kol_sells += 1,
            "Whale" => whale_sells += 1,
            _ => {}
        }
    }

    output::success(json!({
        "total_buys": state.stats.total_buys,
        "total_sells": state.stats.total_sells,
        "successful_trades": state.stats.successful_trades,
        "failed_trades": state.stats.failed_trades,
        "total_invested_sol": total_invested_sol,
        "total_returned_sol": total_returned_sol,
        "gross_pnl_sol": gross_pnl_sol,
        "total_pnl_sol": total_pnl_sol,
        "session_pnl_sol": state.stats.session_pnl_sol,
        "win_count": win_count,
        "loss_count": loss_count,
        "win_rate": format!("{win_rate:.1}%"),
        "label_breakdown": {
            "SmartMoney": smart_money_sells,
            "KOL": kol_sells,
            "Whale": whale_sells,
        },
        "current_positions": state.positions.len(),
        "known_tokens_scanned": state.known_tokens.len(),
        "buy_history": buys.len(),
        "sell_history": sells.len(),
        "consecutive_losses": state.stats.consecutive_losses,
        "cumulative_loss_sol": state.stats.cumulative_loss_sol,
    }));
    Ok(())
}

// ── history ───────────────────────────────────────────────────────────

async fn cmd_history(limit: usize) -> Result<()> {
    let state = SignalTrackerState::load()?;
    let trades: Vec<_> = state.trades.iter().rev().take(limit).collect();

    output::success(json!({
        "trades": trades,
        "total": state.trades.len(),
        "showing": trades.len(),
    }));
    Ok(())
}

// ── reset ─────────────────────────────────────────────────────────────

async fn cmd_reset(force: bool) -> Result<()> {
    if !force {
        bail!("This will delete all signal tracker data. Use --force to confirm.");
    }
    SignalTrackerState::reset()?;
    output::success(json!({ "message": "Signal tracker state reset" }));
    Ok(())
}

// ── analyze ───────────────────────────────────────────────────────────

async fn cmd_analyze() -> Result<()> {
    let client = SignalClient::new_read_only()?;
    let signals = client.fetch_signals().await?;

    let mut signal_entries = Vec::new();
    for signal in &signals {
        let addr = signal["token"]["tokenAddress"]
            .as_str()
            .or_else(|| signal["token"]["tokenContractAddress"].as_str())
            .unwrap_or("");
        let symbol = signal["token"]["symbol"]
            .as_str()
            .or_else(|| signal["token"]["name"].as_str())
            .unwrap_or("?");
        let wallet_type = signal["walletType"].as_str().unwrap_or("?");
        let label = engine::wallet_type_label(wallet_type);
        let wallet_count = engine::safe_int(&signal["triggerWalletCount"], 0);
        let sold_ratio = engine::safe_float(&signal["soldRatioPercent"], 0.0);
        let mc = engine::safe_float(&signal["token"]["marketCapUsd"], 0.0);
        let holders = engine::safe_int(&signal["token"]["holders"], 0);

        let (tier, sol_amount) = engine::calc_position_tier(wallet_count as u32);

        let (prefilter_passed, prefilter_reasons) = engine::run_signal_prefilter(signal);

        signal_entries.push(json!({
            "symbol": symbol,
            "address": addr,
            "label": label,
            "wallet_type_label": engine::wallet_type_label(wallet_type),
            "wallet_count": wallet_count,
            "sold_ratio_pct": format!("{sold_ratio:.1}%"),
            "market_cap_usd": mc,
            "holders": holders,
            "tier": tier,
            "position_sol": sol_amount,
            "prefilter_passed": prefilter_passed,
            "prefilter_failures": prefilter_reasons,
        }));
    }

    let state = SignalTrackerState::load()?;

    let passed_count = signal_entries
        .iter()
        .filter(|s| s["prefilter_passed"].as_bool().unwrap_or(false))
        .count();

    output::success(json!({
        "signals_total": signals.len(),
        "signals_passed_prefilter": passed_count,
        "signals": signal_entries,
        "known_tokens_count": state.known_tokens.len(),
        "active_positions": state.positions.len(),
        "session_pnl_sol": state.stats.session_pnl_sol,
        "consecutive_losses": state.stats.consecutive_losses,
    }));
    Ok(())
}
