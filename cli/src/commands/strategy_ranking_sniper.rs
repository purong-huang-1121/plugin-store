//! CLI commands for SOL Ranking Sniper strategy.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::json;
use std::collections::HashSet;
use std::io::Write;

use crate::notifier::{Notifier, NotifyLevel};
use crate::output;
use crate::strategy::ranking_sniper::client::SniperClient;
use crate::strategy::ranking_sniper::config::SniperConfig;
use crate::strategy::ranking_sniper::engine::{self, Position, Trade};
use crate::strategy::ranking_sniper::state::SniperState;

#[derive(Subcommand)]
pub enum RankingSniperCommand {
    /// Execute one tick: fetch ranking, check exits, scan new entries
    Tick {
        /// SOL budget (overrides config)
        #[arg(long)]
        budget: Option<f64>,
        /// SOL per trade (overrides config)
        #[arg(long)]
        per_trade: Option<f64>,
        /// Simulate without executing swaps
        #[arg(long)]
        dry_run: bool,
        /// Max market cap filter (overrides config)
        #[arg(long)]
        max_market_cap: Option<f64>,
        /// Min change % filter (overrides config)
        #[arg(long)]
        min_change: Option<f64>,
        /// Min holders filter (overrides config)
        #[arg(long)]
        min_holders: Option<i64>,
    },
    /// Start the bot in foreground (tick every 10 seconds)
    Start {
        /// SOL budget (overrides config)
        #[arg(long)]
        budget: Option<f64>,
        /// SOL per trade (overrides config)
        #[arg(long)]
        per_trade: Option<f64>,
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
    /// Market analysis (current ranking, top tokens)
    Analyze,
    /// Show wallet SOL balance
    Balance,
    /// Show all configurable parameters and their current values
    Config,
    /// Force-sell all open positions immediately
    SellAll,
    /// Sell a specific token by address (raw amount)
    Sell {
        /// Token contract address
        token: String,
        /// Raw token amount to sell
        #[arg(long)]
        amount: String,
    },
}

pub async fn execute(cmd: RankingSniperCommand) -> Result<()> {
    match cmd {
        RankingSniperCommand::Tick {
            budget,
            per_trade,
            dry_run,
            max_market_cap,
            min_change,
            min_holders,
        } => {
            let notifier = make_notifier();
            let mut cfg = SniperConfig::load()?;
            if let Some(v) = budget { cfg.budget_sol = v; }
            if let Some(v) = per_trade { cfg.per_trade_sol = v; }
            if let Some(v) = max_market_cap { cfg.max_market_cap = v; }
            if let Some(v) = min_change { cfg.min_change_pct = v; }
            if let Some(v) = min_holders { cfg.min_holders = v; }
            cmd_tick_with_config(cfg, dry_run, &notifier).await
        }
        RankingSniperCommand::Start {
            budget,
            per_trade,
            dry_run,
        } => {
            let cfg = SniperConfig::load()?;
            cmd_start(budget.unwrap_or(cfg.budget_sol), per_trade.unwrap_or(cfg.per_trade_sol), dry_run).await
        }
        RankingSniperCommand::Stop => cmd_stop().await,
        RankingSniperCommand::Status => cmd_status().await,
        RankingSniperCommand::Report => cmd_report().await,
        RankingSniperCommand::History { limit } => cmd_history(limit).await,
        RankingSniperCommand::Reset { force } => cmd_reset(force).await,
        RankingSniperCommand::Analyze => cmd_analyze().await,
        RankingSniperCommand::Balance => cmd_balance().await,
        RankingSniperCommand::Config => cmd_config().await,
        RankingSniperCommand::SellAll => cmd_sell_all().await,
        RankingSniperCommand::Sell { token, amount } => cmd_sell(&token, &amount).await,
    }
}

/// Create a Notifier using config file credentials (priority) or env vars.
fn make_notifier() -> Notifier {
    let cfg = SniperConfig::load().unwrap_or_default();
    let token = cfg
        .telegram_bot_token
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
    let chat_id = cfg
        .telegram_chat_id
        .or_else(|| std::env::var("TELEGRAM_CHAT_ID").ok());
    Notifier::new(token, chat_id, "\u{1f3af} Ranking Sniper")
}

// ── balance ───────────────────────────────────────────────────────────

async fn cmd_balance() -> Result<()> {
    let client = SniperClient::new_read_only()?;
    let wallet = crate::onchainos::get_sol_address().unwrap_or_default();
    let sol = client.fetch_sol_balance().await?;
    let hint = if sol < 0.1 {
        format!("余额不足，建议充值至少 0.1 SOL（当前 {:.4} SOL）", sol)
    } else {
        format!("{:.4} SOL 可用", sol)
    };
    output::success(serde_json::json!({
        "wallet": wallet,
        "sol_balance": sol,
        "sufficient": sol >= 0.1,
        "hint": hint,
    }));
    Ok(())
}

// ── config ────────────────────────────────────────────────────────────

fn build_config_json(cfg: &SniperConfig) -> serde_json::Value {
    json!({
        "资金管理": {
            "budget_sol": { "值": cfg.budget_sol, "说明": "策略总预算 (SOL)" },
            "per_trade_sol": { "值": cfg.per_trade_sol, "说明": "单笔买入金额 (SOL)" },
            "max_positions": { "值": cfg.max_positions, "说明": "最大同时持仓数" },
            "gas_reserve_sol": { "值": cfg.gas_reserve_sol, "说明": "Gas 预留 (SOL)" },
            "min_wallet_balance": { "值": cfg.min_wallet_balance, "说明": "最低钱包余额 (SOL)" },
            "daily_loss_limit_pct": { "值": cfg.daily_loss_limit_pct, "说明": "日亏损上限 (%)" },
            "dry_run": { "值": cfg.dry_run, "说明": "模拟模式" },
        },
        "交易参数": {
            "slippage_pct": { "值": cfg.slippage_pct, "说明": "DEX 滑点 (%)", "生产建议": "3-5" },
            "score_buy_threshold": { "值": cfg.score_buy_threshold, "说明": "Momentum Score 买入阈值 (0-125)", "生产建议": 40 },
            "tick_interval_secs": { "值": cfg.tick_interval_secs, "说明": "轮询间隔 (秒)" },
            "cooldown_minutes": { "值": cfg.cooldown_minutes, "说明": "冷却期 (分钟)" },
            "top_n": { "值": cfg.top_n, "说明": "排行榜 Top N" },
        },
        "一级_Slot_Guard": {
            "min_change_pct": { "值": cfg.min_change_pct, "说明": "涨幅下限 (%)", "生产建议": 15.0 },
            "max_change_pct": { "值": cfg.max_change_pct, "说明": "涨幅上限 (%)" },
            "min_liquidity": { "值": cfg.min_liquidity, "说明": "最低流动性 ($)", "生产建议": 5000.0 },
            "min_market_cap": { "值": cfg.min_market_cap, "说明": "市值下限 ($)", "生产建议": 5000.0 },
            "max_market_cap": { "值": cfg.max_market_cap, "说明": "市值上限 ($)", "生产建议": 10_000_000.0 },
            "min_holders": { "值": cfg.min_holders, "说明": "最低持有者数", "生产建议": 30 },
            "min_buy_ratio": { "值": cfg.min_buy_ratio, "说明": "最低买入比 (0-1)", "生产建议": 0.55 },
            "min_traders": { "值": cfg.min_traders, "说明": "最低独立交易者数", "生产建议": 20 },
        },
        "二级_Advanced_Safety": {
            "max_risk_level": { "值": cfg.max_risk_level, "说明": "最大风控等级", "生产建议": 1 },
            "max_top10_hold": { "值": cfg.max_top10_hold, "说明": "Top10 持仓 (%)", "生产建议": 50.0 },
            "max_dev_hold": { "值": cfg.max_dev_hold, "说明": "Dev 持仓 (%)", "生产建议": 20.0 },
            "max_bundler_hold": { "值": cfg.max_bundler_hold, "说明": "Bundler 持仓 (%)", "生产建议": 15.0 },
            "min_lp_burn": { "值": cfg.min_lp_burn, "说明": "LP 销毁 (%)", "生产建议": 80.0 },
            "max_dev_rug_count": { "值": cfg.max_dev_rug_count, "说明": "Dev Rug 历史数", "生产建议": 10 },
            "max_sniper_hold": { "值": cfg.max_sniper_hold, "说明": "狙击手持仓 (%)", "生产建议": 20.0 },
            "block_internal": { "值": cfg.block_internal, "说明": "拒绝 PumpFun 内盘", "生产建议": true },
        },
        "三级_Holder_Risk_Scan": {
            "max_suspicious_hold": { "值": cfg.max_suspicious_hold, "说明": "可疑持仓 (%)", "生产建议": 10.0 },
            "max_suspicious_count": { "值": cfg.max_suspicious_count, "说明": "可疑地址数", "生产建议": 5 },
            "block_phishing": { "值": cfg.block_phishing, "说明": "拒绝钓鱼地址", "生产建议": true },
        },
        "退出系统": {
            "hard_stop_pct": { "值": cfg.hard_stop_pct, "说明": "硬止损 (%)" },
            "fast_stop_time_secs": { "值": cfg.fast_stop_time_secs, "说明": "快速止损窗口 (秒)" },
            "fast_stop_pct": { "值": cfg.fast_stop_pct, "说明": "快速止损 (%)" },
            "trailing_activate_pct": { "值": cfg.trailing_activate_pct, "说明": "追踪止损激活 (%)" },
            "trailing_drawdown_pct": { "值": cfg.trailing_drawdown_pct, "说明": "追踪止损回撤 (%)" },
            "time_stop_secs": { "值": cfg.time_stop_secs, "说明": "时间止损 (秒)", "生产建议": 21600 },
            "tp_levels": { "值": cfg.tp_levels, "说明": "梯度止盈 [TP1%, TP2%, TP3%]" },
        },
        "熔断": {
            "max_consecutive_errors": { "值": cfg.max_consecutive_errors, "说明": "连续错误触发熔断数" },
            "cooldown_after_errors": { "值": cfg.cooldown_after_errors, "说明": "熔断后冷却时间 (秒)" },
        },
        "日志": {
            "log_file": { "值": cfg.log_path().display().to_string(), "说明": "日志文件路径" },
        },
        "Telegram通知": {
            "telegram_bot_token": { "值": cfg.telegram_bot_token.as_deref().unwrap_or("未配置"), "说明": "Telegram Bot Token (或环境变量 TELEGRAM_BOT_TOKEN)" },
            "telegram_chat_id": { "值": cfg.telegram_chat_id.as_deref().unwrap_or("未配置"), "说明": "Telegram Chat ID (或环境变量 TELEGRAM_CHAT_ID)" },
        },
    })
}

async fn cmd_config() -> Result<()> {
    let cfg = SniperConfig::load()?;
    cfg.print_summary();

    let config_json = build_config_json(&cfg);
    output::success(json!({
        "config": config_json,
        "config_file": SniperConfig::config_path().display().to_string(),
        "log_file": cfg.log_path().display().to_string(),
    }));
    Ok(())
}

/// Append a line to the log file (best-effort, never fails).
fn log_to_file(log_path: &std::path::Path, msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let _ = writeln!(f, "[{now}] {msg}");
    }
}

/// Save defaults to config file if it doesn't exist yet.
fn ensure_config_file() -> Result<SniperConfig> {
    let cfg = SniperConfig::load()?;
    if !SniperConfig::config_path().exists() {
        cfg.save()?;
        eprintln!(
            "[config] 已生成默认配置文件: {}",
            SniperConfig::config_path().display()
        );
    }
    Ok(cfg)
}

// ── tick ──────────────────────────────────────────────────────────────

async fn cmd_tick(budget: f64, per_trade: f64, dry_run: bool, notifier: &Notifier) -> Result<()> {
    let mut cfg = SniperConfig::load()?;
    cfg.budget_sol = budget;
    cfg.per_trade_sol = per_trade;
    cmd_tick_with_config(cfg, dry_run, notifier).await
}

async fn cmd_tick_with_config(cfg: SniperConfig, dry_run: bool, notifier: &Notifier) -> Result<()> {
    let budget = cfg.budget_sol;
    let per_trade = cfg.per_trade_sol;
    let mut state = SniperState::load()?;

    state.config.budget_sol = budget;
    state.config.per_trade_sol = per_trade;
    state.config.dry_run = dry_run;
    if state.remaining_budget_sol <= 0.0 {
        state.remaining_budget_sol = cfg.budget_sol;
    }
    state.maybe_reset_daily();

    // Circuit breaker
    if let Some(reason) = state.check_circuit_breaker(&cfg) {
        notifier
            .notify(NotifyLevel::Error, &format!("Circuit breaker: {reason}"))
            .await;
        output::error(&reason);
        return Ok(());
    }

    // Check daily loss
    if let Some(reason) =
        engine::check_daily_loss(state.stats.daily_pnl_sol, budget, cfg.daily_loss_limit_pct)
    {
        state.stopped = true;
        state.stop_reason = Some(reason.clone());
        state.save()?;
        notifier
            .notify(NotifyLevel::Error, &format!("Daily loss limit: {reason}"))
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
            "Bot stopped: {reason}. Use 'ranking-sniper reset --force' to restart."
        ));
        return Ok(());
    }

    // Create client
    let client = if dry_run {
        SniperClient::new_read_only()?
    } else {
        SniperClient::new()?
    };

    // Fetch ranking
    let ranking = match client.fetch_ranking(cfg.top_n).await {
        Ok(r) => r,
        Err(e) => {
            state.errors.consecutive_errors += 1;
            state.errors.last_error_time = Some(chrono::Utc::now().to_rfc3339());
            state.errors.last_error_msg = Some(format!("{e:#}"));
            state.save()?;
            bail!("Failed to fetch ranking: {e:#}");
        }
    };

    if ranking.is_empty() {
        state.save()?;
        output::success(json!({ "action": "no_ranking_data" }));
        return Ok(());
    }

    let current_ranking_addrs: HashSet<String> = ranking
        .iter()
        .filter_map(|t| t["tokenContractAddress"].as_str().map(|s| s.to_string()))
        .collect();

    let now = chrono::Utc::now();
    let now_ts = now.timestamp();
    let now_str = now.to_rfc3339();

    let mut actions = Vec::new();

    // ── Check exits for existing positions ──
    let position_addrs: Vec<String> = state.positions.keys().cloned().collect();
    for addr in position_addrs {
        let price = match client.fetch_price(&addr).await {
            Ok(p) => p,
            Err(_) => continue,
        };

        let exit_signal = {
            let pos = state.positions.get_mut(&addr).unwrap();
            engine::check_exits(pos, price, &current_ranking_addrs, now_ts, &cfg)
        };

        if let Some(signal) = exit_signal {
            let pos = state.positions.get(&addr).unwrap().clone();
            let pnl_pct = if pos.buy_price > 0.0 {
                (price - pos.buy_price) / pos.buy_price * 100.0
            } else {
                0.0
            };

            // Execute sell
            let tx_hash = if !dry_run {
                let sell_amount = if pos.amount_raw.is_empty() {
                    // Fallback: estimate from buy amount and price
                    let estimated = (pos.buy_amount_sol * 1e9 / pos.buy_price) as u64;
                    format!("{}", estimated)
                } else {
                    pos.amount_raw.clone()
                };
                match client.sell_token(&addr, &sell_amount).await {
                    Ok(sr) => sr.tx_hash.unwrap_or_default(),
                    Err(e) => {
                        eprintln!("[sniper] sell failed for {}: {e:#}", pos.symbol);
                        notifier
                            .notify(
                                NotifyLevel::Error,
                                &format!(
                                    "Sell failed: {}\nReason: {}\nError: {e:#}",
                                    pos.symbol, signal.reason
                                ),
                            )
                            .await;
                        actions.push(json!({
                            "action": "exit_failed",
                            "symbol": pos.symbol,
                            "reason": signal.reason,
                            "error": format!("{e:#}"),
                        }));
                        continue;
                    }
                }
            } else {
                "DRY_RUN".to_string()
            };

            let estimated_return = pos.buy_amount_sol * (1.0 + pnl_pct / 100.0);
            state.remaining_budget_sol += estimated_return;
            state.stats.total_returned_sol += estimated_return;
            state.stats.daily_pnl_sol += estimated_return - pos.buy_amount_sol;
            state.stats.total_sells += 1;
            state.stats.successful_trades += 1;

            state.push_trade(Trade {
                time: now_str.clone(),
                symbol: pos.symbol.clone(),
                token_address: addr.clone(),
                action: "SELL".to_string(),
                price,
                amount_sol: estimated_return,
                score: None,
                exit_reason: Some(signal.reason.clone()),
                pnl_pct: Some(pnl_pct),
                pnl_sol: Some(estimated_return - pos.buy_amount_sol),
                tx_hash: tx_hash.clone(),
            });

            let pnl_sol = estimated_return - pos.buy_amount_sol;
            notifier
                .notify(
                    if pnl_sol >= 0.0 { NotifyLevel::Success } else { NotifyLevel::Warning },
                    &format!(
                        "Exit: {} ({})\nReason: {}\nPnL: {pnl_pct:+.1}% ({pnl_sol:+.4} SOL)\nTx: {}",
                        pos.symbol, signal.exit_type, signal.reason,
                        if tx_hash == "DRY_RUN" { "DRY_RUN".to_string() } else {
                            format!("{}...{}", &tx_hash[..8.min(tx_hash.len())], &tx_hash[tx_hash.len().saturating_sub(6)..])
                        }
                    ),
                )
                .await;

            actions.push(json!({
                "action": "exit",
                "symbol": pos.symbol,
                "reason": signal.reason,
                "exit_type": signal.exit_type,
                "pnl_pct": format!("{pnl_pct:+.1}%"),
                "pnl_sol": format!("{pnl_sol:+.4}"),
                "tx_hash": tx_hash,
            }));

            state.positions.remove(&addr);
        }
    }

    // ── Scan for new entries ──
    for token in &ranking {
        let addr = token["tokenContractAddress"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let symbol = token["tokenSymbol"].as_str().unwrap_or("?").to_string();

        if addr.is_empty() {
            continue;
        }

        // Skip if already known or holding
        if state.known_tokens.contains(&addr) || state.positions.contains_key(&addr) {
            continue;
        }

        state.known_tokens.insert(addr.clone());

        // Budget check
        if state.remaining_budget_sol < per_trade + cfg.gas_reserve_sol {
            continue;
        }

        // Position limit
        if state.positions.len() >= cfg.max_positions {
            continue;
        }

        // Safety checks
        let adv_info = match client.fetch_advanced_info(&addr).await {
            Ok(info) => info,
            Err(_) => {
                actions.push(json!({
                    "action": "skip",
                    "symbol": symbol,
                    "reason": "failed to fetch advanced info",
                }));
                continue;
            }
        };

        // Slot guard (ranking-data checks)
        let daily_loss_exceeded = engine::check_daily_loss(
            state.stats.daily_pnl_sol,
            state.config.budget_sol,
            cfg.daily_loss_limit_pct,
        )
        .is_some();
        let is_holding = state.positions.contains_key(&addr);
        let cooldown_active = state.is_cooldown_active(&addr, &cfg);
        let (slot_passed, slot_reasons) = engine::run_slot_guard(
            token,
            state.positions.len(),
            is_holding,
            daily_loss_exceeded,
            cooldown_active,
            &cfg,
        );
        if !slot_passed {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "reason": format!("slot_guard: {}", slot_reasons.iter().take(3).cloned().collect::<Vec<_>>().join("; ")),
            }));
            continue;
        }

        // Advanced safety (advanced-info API checks)
        let (adv_passed, adv_reasons) = engine::run_advanced_safety(&adv_info, &cfg);
        if !adv_passed {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "reason": format!("advanced_safety: {}", adv_reasons.iter().take(3).cloned().collect::<Vec<_>>().join("; ")),
            }));
            continue;
        }

        // Holder risk scan
        let suspicious_data = client
            .fetch_holder_risk(&addr, "6")
            .await
            .unwrap_or_default();
        let phishing_data = client
            .fetch_holder_risk(&addr, "8")
            .await
            .unwrap_or_default();
        let suspicious_val = serde_json::Value::Array(suspicious_data.clone());
        let phishing_val = serde_json::Value::Array(phishing_data);
        let (holder_passed, holder_reasons) =
            engine::run_holder_risk_scan(&suspicious_val, &phishing_val, &cfg);
        if !holder_passed {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "reason": format!("holder_risk: {}", holder_reasons.iter().take(3).cloned().collect::<Vec<_>>().join("; ")),
            }));
            continue;
        }

        // Count active suspicious holders for momentum score
        let suspicious_active_count = suspicious_data
            .iter()
            .filter(|h| engine::safe_float(&h["holdPercent"], 0.0) > 0.0)
            .count();

        // Momentum Score
        let score = engine::calc_momentum_score(token, &adv_info, suspicious_active_count);
        if score < cfg.score_buy_threshold {
            actions.push(json!({
                "action": "skip",
                "symbol": symbol,
                "reason": format!("score {score}/125 < threshold {}", cfg.score_buy_threshold),
            }));
            continue;
        }

        // Get current price
        let price = match client.fetch_price(&addr).await {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Execute buy
        let (tx_hash, amount_raw) = if !dry_run {
            match client.buy_token(&addr, per_trade).await {
                Ok(sr) => {
                    let raw = if sr.amount_out > 0.0 {
                        format!("{}", sr.amount_out as u64)
                    } else {
                        String::new()
                    };
                    (sr.tx_hash.unwrap_or_default(), raw)
                }
                Err(e) => {
                    state.errors.consecutive_errors += 1;
                    state.errors.last_error_time = Some(now_str.clone());
                    state.errors.last_error_msg = Some(format!("{e:#}"));
                    state.stats.failed_trades += 1;
                    notifier
                        .notify(
                            NotifyLevel::Error,
                            &format!("Buy failed: {symbol}\nError: {e:#}"),
                        )
                        .await;
                    actions.push(json!({
                        "action": "buy_failed",
                        "symbol": symbol,
                        "error": format!("{e:#}"),
                    }));
                    continue;
                }
            }
        } else {
            ("DRY_RUN".to_string(), String::new())
        };

        // Record position
        state.positions.insert(
            addr.clone(),
            Position {
                token_address: addr.clone(),
                symbol: symbol.clone(),
                buy_price: price,
                buy_amount_sol: per_trade,
                buy_time: now_str.clone(),
                peak_pnl_pct: 0.0,
                trailing_active: false,
                tp_sold: vec![],
                tx_hash: tx_hash.clone(),
                amount_raw: amount_raw.clone(),
            },
        );

        state.remaining_budget_sol -= per_trade;
        state.stats.total_invested_sol += per_trade;
        state.stats.total_buys += 1;
        state.stats.successful_trades += 1;
        state.errors.consecutive_errors = 0;

        state.push_trade(Trade {
            time: now_str.clone(),
            symbol: symbol.clone(),
            token_address: addr,
            action: "BUY".to_string(),
            price,
            amount_sol: per_trade,
            score: Some(score),
            exit_reason: None,
            pnl_pct: None,
            pnl_sol: None,
            tx_hash: tx_hash.clone(),
        });

        notifier
            .notify(
                NotifyLevel::Success,
                &format!(
                    "Buy: {symbol}\nPrice: ${price:.10}\nAmount: {per_trade} SOL\nScore: {score}/125\nTx: {}",
                    if tx_hash == "DRY_RUN" { "DRY_RUN".to_string() } else {
                        format!("{}...{}", &tx_hash[..8.min(tx_hash.len())], &tx_hash[tx_hash.len().saturating_sub(6)..])
                    }
                ),
            )
            .await;

        actions.push(json!({
            "action": "buy",
            "symbol": symbol,
            "price": price,
            "amount_sol": per_trade,
            "score": score,
            "tx_hash": tx_hash,
        }));
    }

    state.save()?;

    output::success(json!({
        "tick_time": now_str,
        "positions": state.positions.len(),
        "remaining_budget_sol": state.remaining_budget_sol,
        "daily_pnl_sol": state.stats.daily_pnl_sol,
        "actions": actions,
        "dry_run": dry_run,
    }));
    Ok(())
}

// ── start ─────────────────────────────────────────────────────────────

async fn cmd_start(budget: f64, per_trade: f64, dry_run: bool) -> Result<()> {
    let pid_path = SniperState::pid_path();

    // Check if already running
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
        let pid: i32 = pid_str.trim().parse().unwrap_or(0);
        #[cfg(unix)]
        if pid > 0 && unsafe { libc::kill(pid, 0) } == 0 {
            bail!(
                "Ranking sniper already running (PID {}). Use 'ranking-sniper stop' first.",
                pid
            );
        }
    }

    // Load config from file, apply CLI overrides
    let mut cfg = ensure_config_file()?;
    cfg.budget_sol = budget;
    cfg.per_trade_sol = per_trade;
    cfg.dry_run = dry_run;

    // Validate credentials before starting
    if cfg.dry_run {
        let _ = SniperClient::new_read_only()?;
    } else {
        let _ = SniperClient::new()?;
    }

    // Write PID file
    let dir = pid_path.parent().context("no parent dir")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Setup log file
    let log_path = cfg.log_path();
    if let Some(log_dir) = log_path.parent() {
        std::fs::create_dir_all(log_dir)?;
    }
    eprintln!("[log] 日志文件: {}", log_path.display());

    // Print parameter summary before starting
    cfg.print_summary();

    let notifier = make_notifier();

    eprintln!(
        "[{}] Ranking sniper started (PID {}) budget={budget} per_trade={per_trade} dry_run={dry_run}",
        chrono::Utc::now().to_rfc3339(),
        std::process::id()
    );
    notifier
        .notify(
            NotifyLevel::Info,
            &format!(
                "Bot started (PID {})\nBudget: {budget} SOL | Per trade: {per_trade} SOL\nDry run: {dry_run}\nTick interval: {}s",
                std::process::id(),
                cfg.tick_interval_secs
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

    // Main loop
    let log_file = cfg.log_path();
    log_to_file(
        &log_file,
        &format!(
            "Sniper started (PID {}) budget={budget} per_trade={per_trade} dry_run={dry_run}",
            std::process::id()
        ),
    );

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        match cmd_tick(budget, per_trade, dry_run, &notifier).await {
            Ok(()) => {
                log_to_file(&log_file, "tick OK");
            }
            Err(e) => {
                let msg = format!("Tick error: {:#}", e);
                eprintln!("[{}] {}", chrono::Utc::now().to_rfc3339(), msg);
                log_to_file(&log_file, &msg);
            }
        }

        // Sleep in small increments to check shutdown
        for _ in 0..(cfg.tick_interval_secs / 2) {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    log_to_file(&log_file, "Sniper stopped");
    eprintln!(
        "[{}] Ranking sniper stopped",
        chrono::Utc::now().to_rfc3339()
    );
    notifier.notify(NotifyLevel::Info, "Bot stopped").await;
    output::success(json!({ "message": "Ranking sniper stopped" }));
    Ok(())
}

// ── stop ──────────────────────────────────────────────────────────────

async fn cmd_stop() -> Result<()> {
    let pid_path = SniperState::pid_path();
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
    let cfg = SniperConfig::load()?;
    let state = SniperState::load()?;

    let pid_path = SniperState::pid_path();
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
                "buy_price": p.buy_price,
                "buy_amount_sol": p.buy_amount_sol,
                "buy_time": p.buy_time,
                "peak_pnl_pct": p.peak_pnl_pct,
                "trailing_active": p.trailing_active,
                "tp_sold": p.tp_sold,
            })
        })
        .collect();

    output::success(json!({
        "bot_running": bot_running,
        "stopped": state.stopped,
        "stop_reason": state.stop_reason,
        "positions": positions,
        "position_count": state.positions.len(),
        "max_positions": cfg.max_positions,
        "remaining_budget_sol": state.remaining_budget_sol,
        "config": {
            "budget_sol": state.config.budget_sol,
            "per_trade_sol": state.config.per_trade_sol,
            "dry_run": state.config.dry_run,
        },
        "daily_pnl_sol": state.stats.daily_pnl_sol,
        "known_tokens_count": state.known_tokens.len(),
        "consecutive_errors": state.errors.consecutive_errors,
        "last_error": state.errors.last_error_msg,
    }));
    Ok(())
}

// ── report ────────────────────────────────────────────────────────────

async fn cmd_report() -> Result<()> {
    let state = SniperState::load()?;

    let buys: Vec<_> = state.trades.iter().filter(|t| t.action == "BUY").collect();
    let sells: Vec<_> = state.trades.iter().filter(|t| t.action == "SELL").collect();

    let total_pnl_sol: f64 = sells.iter().filter_map(|t| t.pnl_sol).sum();
    let win_count = sells
        .iter()
        .filter(|t| t.pnl_pct.unwrap_or(0.0) > 0.0)
        .count();
    let loss_count = sells
        .iter()
        .filter(|t| t.pnl_pct.unwrap_or(0.0) <= 0.0)
        .count();
    let win_rate = if !sells.is_empty() {
        win_count as f64 / sells.len() as f64 * 100.0
    } else {
        0.0
    };

    output::success(json!({
        "total_buys": state.stats.total_buys,
        "total_sells": state.stats.total_sells,
        "successful_trades": state.stats.successful_trades,
        "failed_trades": state.stats.failed_trades,
        "total_invested_sol": state.stats.total_invested_sol,
        "total_returned_sol": state.stats.total_returned_sol,
        "total_pnl_sol": total_pnl_sol,
        "daily_pnl_sol": state.stats.daily_pnl_sol,
        "win_count": win_count,
        "loss_count": loss_count,
        "win_rate": format!("{win_rate:.1}%"),
        "current_positions": state.positions.len(),
        "known_tokens_scanned": state.known_tokens.len(),
        "buy_history": buys.len(),
        "sell_history": sells.len(),
    }));
    Ok(())
}

// ── history ───────────────────────────────────────────────────────────

async fn cmd_history(limit: usize) -> Result<()> {
    let state = SniperState::load()?;
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
        bail!("This will delete all ranking sniper data. Use --force to confirm.");
    }
    SniperState::reset()?;
    output::success(json!({ "message": "Ranking sniper state reset" }));
    Ok(())
}

// ── analyze ───────────────────────────────────────────────────────────

async fn cmd_analyze() -> Result<()> {
    let cfg = SniperConfig::load()?;
    let client = SniperClient::new_read_only()?;
    let ranking = client.fetch_ranking(cfg.top_n).await?;

    let mut tokens = Vec::new();
    for token in &ranking {
        let addr = token["tokenContractAddress"].as_str().unwrap_or("");
        let symbol = token["tokenSymbol"].as_str().unwrap_or("?");
        let change = engine::safe_float(&token["change"], 0.0);
        let mc = engine::safe_float(&token["marketCap"], 0.0);
        let vol = engine::safe_float(&token["volume"], 0.0);
        let holders = engine::safe_int(&token["holders"], 0);

        tokens.push(json!({
            "symbol": symbol,
            "address": addr,
            "change_24h": format!("{change:+.1}%"),
            "market_cap": mc,
            "volume": vol,
            "holders": holders,
        }));
    }

    let state = SniperState::load()?;

    output::success(json!({
        "ranking_count": ranking.len(),
        "top_tokens": tokens,
        "known_tokens_count": state.known_tokens.len(),
        "active_positions": state.positions.len(),
    }));
    Ok(())
}

// ── sell-all ────────────────────────────────────────────────────────

async fn cmd_sell_all() -> Result<()> {
    let client = SniperClient::new()?;
    let mut state = SniperState::load()?;

    let positions: Vec<_> = state.positions.values().cloned().collect();
    if positions.is_empty() {
        output::success(json!({ "message": "No open positions to sell" }));
        return Ok(());
    }

    eprintln!("[sell-all] Selling {} positions...", positions.len());
    let mut results = Vec::new();

    for pos in &positions {
        eprintln!(
            "[sell-all] Selling {} ({})...",
            pos.symbol, pos.token_address
        );
        // Try full amount first, then halve repeatedly on insufficient liquidity
        let full_amount: u128 = pos.amount_raw.parse().unwrap_or(0);
        let mut try_amount = full_amount;
        let mut _sell_ok = false;

        for attempt in 0..4 {
            if try_amount == 0 {
                break;
            }
            let amount_raw = try_amount.to_string();
            if attempt > 0 {
                eprintln!("[sell-all]   retry {}: amount_raw={}", attempt, amount_raw);
            }
            match client.sell_token(&pos.token_address, &amount_raw).await {
                Ok(sell_result) => {
                    let tx_hash = sell_result.tx_hash.clone().unwrap_or_default();
                    let sol_out = sell_result.amount_out / 1e9;
                    eprintln!(
                        "[sell-all] {} sold: tx={} sol_out={:.6}",
                        pos.symbol, tx_hash, sol_out
                    );
                    state.trades.push(Trade {
                        time: chrono::Utc::now().to_rfc3339(),
                        symbol: pos.symbol.clone(),
                        token_address: pos.token_address.clone(),
                        action: "SELL".to_string(),
                        price: 0.0,
                        amount_sol: sol_out,
                        score: None,
                        exit_reason: Some("manual sell-all".to_string()),
                        pnl_pct: None,
                        pnl_sol: None,
                        tx_hash: tx_hash.clone(),
                    });
                    results.push(json!({
                        "symbol": pos.symbol,
                        "token": pos.token_address,
                        "status": "sold",
                        "tx_hash": tx_hash,
                        "sol_out": sol_out,
                    }));
                    _sell_ok = true;
                    break;
                }
                Err(e) => {
                    let err_str = format!("{:#}", e);
                    if err_str.contains("Insufficient liquidity") && attempt < 3 {
                        try_amount /= 2;
                        eprintln!(
                            "[sell-all]   {} insufficient liquidity, halving to {}",
                            pos.symbol, try_amount
                        );
                        continue;
                    }
                    eprintln!("[sell-all] {} sell FAILED: {}", pos.symbol, err_str);
                    results.push(json!({
                        "symbol": pos.symbol,
                        "token": pos.token_address,
                        "status": "failed",
                        "error": err_str,
                    }));
                    break;
                }
            }
        }
        // Small delay between sells
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // Remove sold positions from state
    for r in &results {
        if r["status"].as_str() == Some("sold") {
            if let Some(addr) = r["token"].as_str() {
                state.positions.remove(addr);
            }
        }
    }
    state.save()?;

    output::success(json!({
        "sold": results.iter().filter(|r| r["status"] == "sold").count(),
        "failed": results.iter().filter(|r| r["status"] == "failed").count(),
        "results": results,
    }));
    Ok(())
}

// ── sell ─────────────────────────────────────────────────────────────

async fn cmd_sell(token_addr: &str, amount_raw: &str) -> Result<()> {
    let client = SniperClient::new()?;
    eprintln!(
        "[sell] Selling token {} amount_raw={}...",
        token_addr, amount_raw
    );
    let result = client.sell_token(token_addr, amount_raw).await?;
    let tx_hash = result.tx_hash.clone().unwrap_or_default();
    let sol_out = result.amount_out / 1e9;
    output::success(json!({
        "token": token_addr,
        "tx_hash": tx_hash,
        "sol_out": sol_out,
        "amount_out_lamports": result.amount_out,
    }));
    Ok(())
}
