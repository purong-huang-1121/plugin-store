//! Background daemon for automated yield rebalancing.
//!
//! Manages PID file, signal handling, periodic yield checks, safety monitoring,
//! and automated rebalance execution. Follows the same patterns as the grid bot.

use alloy::primitives::U256;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::chains::{self, AutoRebalanceConfig};
use super::config::{log_to_file, AutoRebalanceConfig as UserConfig};
use super::engine::{self, Decision, EngineConfig};
use super::executor;
use super::notifier::{Notifier, NotifyLevel};
use super::safety_monitor::SafetyMonitor;
use super::state::{DaemonConfig, PositionState, RebalanceRecord, StateData};
use super::yield_monitor::{self, Protocol};
use crate::dapp::aave::client::AaveClient;
use crate::dapp::compound::CompoundClient;
use crate::dapp::morpho::client::MorphoClient;
use crate::dapp::morpho::vault::MorphoVaultClient;
use crate::output;

/// How often safety checks run between main cycles (seconds).
const SAFETY_CHECK_INTERVAL_SECS: u64 = 900;

// ── PID management ──────────────────────────────────────────────────

/// Path to the daemon PID file.
pub fn pid_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".plugin-store")
        .join("auto-rebalance-daemon.pid")
}

/// Check if the daemon is already running. Returns the PID if alive.
pub fn check_running() -> Option<i32> {
    let path = pid_path();
    if !path.exists() {
        return None;
    }
    let pid_str = std::fs::read_to_string(&path).unwrap_or_default();
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    if pid <= 0 {
        return None;
    }
    #[cfg(unix)]
    {
        if unsafe { libc::kill(pid, 0) } == 0 {
            return Some(pid);
        }
        None
    }
    #[cfg(not(unix))]
    {
        Some(pid)
    }
}

// ── Start ───────────────────────────────────────────────────────────

/// Start the yield rebalancing daemon.
pub async fn start(
    interval_secs: u64,
    min_spread: f64,
    max_break_even: u64,
    telegram_token: Option<String>,
    telegram_chat_id: Option<String>,
    chain: &str,
) -> Result<()> {
    let chain_config = chains::get_config(chain)?;

    // Check not already running
    if let Some(pid) = check_running() {
        bail!(
            "Auto-rebalance daemon already running (PID {}). Use 'auto-rebalance auto stop' first.",
            pid
        );
    }

    // Write PID file
    let path = pid_path();
    let dir = path.parent().context("no parent dir")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, std::process::id().to_string())?;

    // Signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        r.store(false, Ordering::SeqCst);
    });

    #[cfg(unix)]
    {
        let r = running.clone();
        tokio::spawn(async move {
            let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
            sig.recv().await;
            r.store(false, Ordering::SeqCst);
        });
    }

    // Create notifier (prefer args, fall back to env)
    let notifier = if telegram_token.is_some() || telegram_chat_id.is_some() {
        Notifier::new(telegram_token, telegram_chat_id, "🤖 Auto-Rebalancer")
    } else {
        Notifier::from_env("🤖 Auto-Rebalancer")
    };

    // Engine config
    let engine_config = EngineConfig {
        min_yield_spread: min_spread,
        max_break_even_days: max_break_even as u32,
        ..Default::default()
    };

    // Load state and save daemon config
    let mut state = StateData::load().unwrap_or_default();
    state.config = DaemonConfig {
        interval_minutes: interval_secs / 60,
        min_spread,
        max_break_even_days: max_break_even,
    };

    // Clear TVL history when switching chains to prevent cross-chain comparisons
    // from triggering false emergency withdrawals.
    let current_chain = chain_config.chain_name.to_string();
    if state.chain.as_deref() != Some(&current_chain) {
        if state.chain.is_some() {
            eprintln!(
                "[WARN] Chain changed from {:?} to {}, clearing TVL history",
                state.chain, current_chain
            );
        }
        state.tvl_history.clear();
        state.chain = Some(current_chain);
    }
    let _ = state.save();

    // Initialize safety monitor with persisted TVL history and configurable alert threshold
    let tvl_alert_threshold = UserConfig::load()
        .map(|c| c.tvl_alert_threshold)
        .unwrap_or(20.0);
    let mut safety_monitor = SafetyMonitor::with_alert_threshold(tvl_alert_threshold);
    safety_monitor.load_tvl_history(&state);

    // Detect initial protocol from on-chain balances
    let mut current_protocol = detect_current_protocol(chain_config).await;

    notifier
        .notify(
            NotifyLevel::Info,
            &format!(
                "Daemon started (PID {}, chain {}, interval {}s, min spread {:.2}%, max break-even {}d)\nCurrent protocol: {}",
                std::process::id(),
                chain_config.chain_name,
                interval_secs,
                min_spread,
                max_break_even,
                current_protocol
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
        )
        .await;

    let start_msg = format!(
        "Auto-rebalance daemon started (PID {}, chain {}, interval {}s, min spread {:.2}%, max break-even {}d)",
        std::process::id(),
        chain_config.chain_name,
        interval_secs,
        min_spread,
        max_break_even,
    );
    eprintln!("[{}] {}", chrono::Utc::now().to_rfc3339(), start_msg);
    log_to_file(&start_msg);

    // ── Main loop ───────────────────────────────────────────────────

    while running.load(Ordering::SeqCst) {
        // Run one main cycle
        match run_cycle(
            &mut safety_monitor,
            &notifier,
            &engine_config,
            &mut state,
            &mut current_protocol,
            chain_config,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                let msg = format!("Cycle error: {e:#}");
                eprintln!("[{}] {}", chrono::Utc::now().to_rfc3339(), msg);
                log_to_file(&msg);
                notifier.notify(NotifyLevel::Warning, &msg).await;
            }
        }

        // Save state after each cycle
        state.tvl_history = safety_monitor.get_tvl_history();
        state.last_check_timestamp = chrono::Utc::now().timestamp() as u64;
        let _ = state.save();

        // Sleep in 5-second increments, running safety checks every SAFETY_CHECK_INTERVAL_SECS
        let mut elapsed = 0u64;
        let mut since_safety_check = 0u64;

        while elapsed < interval_secs && running.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            elapsed += 5;
            since_safety_check += 5;

            // Periodic safety check between main cycles
            if since_safety_check >= SAFETY_CHECK_INTERVAL_SECS {
                since_safety_check = 0;
                if let Some(proto) = current_protocol {
                    let health = safety_monitor.check_all_protocols(None).await;
                    if let Some(h) = health.iter().find(|h| h.protocol == proto) {
                        if !h.is_healthy {
                            let msg = format!(
                                "EMERGENCY: {} unhealthy during safety check — {}",
                                proto,
                                h.alerts.join(", ")
                            );
                            log_to_file(&msg);
                            notifier.notify(NotifyLevel::Error, &msg).await;

                            // Execute emergency withdrawal
                            match executor::emergency_withdraw_on(proto, chain_config).await {
                                Ok(result) => {
                                    let msg = format!(
                                        "Emergency withdrawal from {}: {} USDC, tx {}",
                                        result.protocol, result.amount_usdc, result.tx_hash
                                    );
                                    notifier.notify(NotifyLevel::Warning, &msg).await;
                                    current_protocol = None;
                                    state.current_position = None;
                                    let _ = state.save();
                                }
                                Err(e) => {
                                    notifier
                                        .notify(
                                            NotifyLevel::Error,
                                            &format!("Emergency withdrawal FAILED: {e:#}"),
                                        )
                                        .await;
                                }
                            }
                            break; // Exit sleep loop, run next cycle immediately
                        }
                    }
                }
            }
        }
    }

    // ── Shutdown ─────────────────────────────────────────────────────

    notifier
        .notify(NotifyLevel::Info, "Daemon shutting down")
        .await;

    state.tvl_history = safety_monitor.get_tvl_history();
    state.last_check_timestamp = chrono::Utc::now().timestamp() as u64;
    let _ = state.save();

    let _ = std::fs::remove_file(pid_path());
    eprintln!(
        "[{}] Auto-rebalance daemon stopped",
        chrono::Utc::now().to_rfc3339()
    );
    log_to_file("Auto-rebalance daemon stopped");
    output::success(json!({ "message": "Auto-rebalance daemon stopped" }));
    Ok(())
}

// ── Stop ────────────────────────────────────────────────────────────

/// Stop a running daemon by sending SIGTERM.
pub async fn stop() -> Result<()> {
    let path = pid_path();
    if !path.exists() {
        bail!("No running daemon found (PID file missing)");
    }
    let pid_str = std::fs::read_to_string(&path)?;
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    if pid == 0 {
        bail!("Invalid PID in file");
    }

    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid, libc::SIGTERM) };
        if result != 0 {
            let _ = std::fs::remove_file(&path);
            bail!("Process {} not found (already stopped?)", pid);
        }
    }

    // Wait up to 5 seconds for the process to exit
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        #[cfg(unix)]
        {
            if unsafe { libc::kill(pid, 0) } != 0 {
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&path);
    output::success(json!({ "message": format!("Stopped Auto-rebalance daemon (PID {})", pid) }));
    Ok(())
}

// ── Status ──────────────────────────────────────────────────────────

/// Show daemon status and recent activity.
pub async fn status() -> Result<()> {
    let running = check_running();
    let state = StateData::load().unwrap_or_default();

    let recent_rebalances: Vec<&RebalanceRecord> =
        state.rebalance_history.iter().rev().take(5).collect();

    output::success(json!({
        "daemon_running": running.is_some(),
        "pid": running,
        "config": {
            "interval_minutes": state.config.interval_minutes,
            "min_spread": state.config.min_spread,
            "max_break_even_days": state.config.max_break_even_days,
        },
        "current_position": state.current_position,
        "last_check_timestamp": state.last_check_timestamp,
        "total_rebalances": state.rebalance_history.len(),
        "recent_rebalances": recent_rebalances,
    }));
    Ok(())
}

// ── Main cycle ──────────────────────────────────────────────────────

/// Run one main cycle: fetch yields, check health, detect capital, decide, execute.
async fn run_cycle(
    safety_monitor: &mut SafetyMonitor,
    notifier: &Notifier,
    engine_config: &EngineConfig,
    state: &mut StateData,
    current_protocol: &mut Option<Protocol>,
    chain_config: &'static AutoRebalanceConfig,
) -> Result<()> {
    // 1. Fetch yields
    let yields = yield_monitor::fetch_all_yields_for(chain_config).await?;

    // 2. Check protocol health
    let health = safety_monitor.check_all_protocols(Some(&yields)).await;

    // 3. Check gas and estimate real gas cost (matches TS profit-calculator.ts)
    let gas_spiking = SafetyMonitor::is_gas_spiking_on(chain_config)
        .await
        .unwrap_or(false);
    let gas_cost_usd = if gas_spiking {
        999.0
    } else {
        estimate_gas_cost_usd(chain_config).await.unwrap_or(0.03)
    };

    // 4. Detect capital (protocol balance or wallet USDC for initial deposit)
    let capital_usd = match *current_protocol {
        Some(proto) => detect_capital(proto, chain_config).await,
        None => detect_wallet_usdc(chain_config).await,
    };

    // 5. Frequency guard — enforce cooldown since last rebalance
    let last_rebalance_ts = state
        .rebalance_history
        .last()
        .map(|r| r.timestamp)
        .unwrap_or(0);
    let now_ts = chrono::Utc::now().timestamp() as u64;
    let since_last = now_ts.saturating_sub(last_rebalance_ts);
    let cooldown_active =
        last_rebalance_ts > 0 && since_last < engine_config.min_rebalance_interval_secs;

    // 6. Decide
    let decision = if cooldown_active {
        let remaining = engine_config.min_rebalance_interval_secs - since_last;
        engine::Decision::Hold {
            reason: format!(
                "Rebalance cooldown: {}h {}m remaining (24h minimum between rebalances)",
                remaining / 3600,
                (remaining % 3600) / 60,
            ),
        }
    } else {
        engine::decide_with_safety(
            &yields,
            *current_protocol,
            capital_usd,
            gas_cost_usd,
            engine_config,
            &health,
            gas_spiking,
        )
    };

    let cycle_msg = format!(
        "Cycle: chain={}, protocol={}, capital=${:.2}, decision={:?}",
        chain_config.chain_name,
        current_protocol
            .map(|p| p.to_string())
            .unwrap_or_else(|| "none".into()),
        capital_usd,
        decision,
    );
    eprintln!("[{}] {}", chrono::Utc::now().to_rfc3339(), cycle_msg);
    log_to_file(&cycle_msg);

    // 6. Execute
    match decision {
        Decision::Hold { ref reason } => {
            eprintln!("[{}] Hold: {}", chrono::Utc::now().to_rfc3339(), reason);

            // Deposit idle USDC sitting in wallet into current protocol (matches TS orchestrator.ts)
            // This prevents USDC fragmentation — any leftover wallet balance gets swept into the active position.
            if let Some(proto) = *current_protocol {
                deposit_idle_balance(proto, chain_config, &yields, notifier).await;
            }
        }

        Decision::Rebalance {
            from,
            to,
            yield_spread,
            break_even_days,
        } => {
            let amount = match *current_protocol {
                Some(proto) => detect_balance(proto, chain_config).await,
                None => detect_wallet_usdc_raw(chain_config).await,
            };

            if amount.is_zero() {
                eprintln!(
                    "[{}] Rebalance skipped: zero balance in {}",
                    chrono::Utc::now().to_rfc3339(),
                    from
                );
                return Ok(());
            }

            let msg = format!(
                "Rebalancing: {} -> {} (spread {:.2}%, break-even {:.1}d, ${:.2})",
                from, to, yield_spread, break_even_days, capital_usd
            );
            notifier.notify(NotifyLevel::Info, &msg).await;

            // Get the best Morpho vault address from yield data (for deposit target)
            let morpho_vault_addr = yields
                .iter()
                .find(|y| y.protocol == Protocol::Morpho)
                .and_then(|y| y.vault_address.clone());

            // If no current protocol, do deposit-only (no withdrawal needed)
            let exec_result = if current_protocol.is_none() {
                eprintln!(
                    "[{}] Initial deposit: wallet -> {} (${:.2})",
                    chrono::Utc::now().to_rfc3339(),
                    to,
                    capital_usd,
                );
                executor::deposit_only(to, amount, chain_config, morpho_vault_addr.as_deref()).await
            } else {
                executor::execute_rebalance_on(
                    from,
                    to,
                    amount,
                    chain_config,
                    morpho_vault_addr.as_deref(),
                )
                .await
            };

            match exec_result {
                Ok(result) => {
                    let msg = format!(
                        "Rebalance complete: {} -> {}, {} USDC, gas {}",
                        result.from, result.to, result.amount_usdc, result.total_gas_used
                    );
                    log_to_file(&msg);
                    notifier.notify(NotifyLevel::Success, &msg).await;

                    // Update state
                    let now = chrono::Utc::now().timestamp() as u64;
                    let tx_hashes: Vec<String> = result
                        .transactions
                        .iter()
                        .filter_map(|tx| {
                            tx.get("txHash")
                                .or_else(|| tx.get("tx_hash"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();

                    state.add_rebalance(RebalanceRecord {
                        timestamp: now,
                        from_protocol: from.to_string(),
                        to_protocol: to.to_string(),
                        amount: result.amount_usdc.parse::<f64>().unwrap_or(0.0),
                        gas: result.total_gas_used as f64,
                        spread: yield_spread,
                        tx_hashes,
                    });

                    let new_apy = yields
                        .iter()
                        .find(|y| y.protocol == to)
                        .map(|y| y.apy)
                        .unwrap_or(0.0);

                    // Sync on-chain position after rebalance (matches TS orchestrator.ts syncPosition)
                    let actual_balance_usd = detect_capital(to, chain_config).await;
                    let balance_usd = if actual_balance_usd > 0.0 {
                        actual_balance_usd
                    } else {
                        capital_usd // fallback if RPC fails
                    };

                    state.current_position = Some(PositionState {
                        protocol: to.to_string(),
                        balance_usd,
                        apy: new_apy,
                        entered_at: now,
                    });

                    *current_protocol = Some(to);
                }
                Err(e) => {
                    let msg = format!("Rebalance FAILED: {e:#}");
                    log_to_file(&msg);
                    notifier.notify(NotifyLevel::Error, &msg).await;
                }
            }
        }

        Decision::EmergencyWithdraw { ref reason } => {
            let msg = format!("EMERGENCY WITHDRAWAL: {}", reason);
            log_to_file(&msg);
            notifier.notify(NotifyLevel::Error, &msg).await;

            if let Some(proto) = *current_protocol {
                match executor::emergency_withdraw_on(proto, chain_config).await {
                    Ok(result) => {
                        let msg = format!(
                            "Emergency withdrawal complete: {} USDC from {}, tx {}",
                            result.amount_usdc, result.protocol, result.tx_hash
                        );
                        notifier.notify(NotifyLevel::Warning, &msg).await;
                        *current_protocol = None;
                        state.current_position = None;
                    }
                    Err(e) => {
                        notifier
                            .notify(
                                NotifyLevel::Error,
                                &format!("Emergency withdrawal FAILED: {e:#}"),
                            )
                            .await;
                    }
                }
            }
        }
    }

    Ok(())
}

// ── Morpho multi-vault discovery ────────────────────────────────────

/// Query Morpho GraphQL for top USDC vault addresses on the given chain.
/// Returns at most 5 vaults sorted by TVL (largest first), plus the config default.
/// This keeps RPC balance checks fast (max ~6 calls).
pub(crate) async fn get_morpho_usdc_vaults(config: &AutoRebalanceConfig) -> Vec<String> {
    let client = match MorphoClient::new() {
        Ok(c) => c,
        Err(_) => return vec![config.morpho_vault.to_string()],
    };

    let query = r#"
        query Vaults($chainId: Int!, $assetAddress: String!) {
            vaults(where: { chainId_in: [$chainId], assetAddress_in: [$assetAddress], totalAssetsUsd_gte: 100000 }) {
                items {
                    address
                    state { totalAssetsUsd }
                }
            }
        }
    "#;

    let vars = serde_json::json!({
        "chainId": config.chain_id,
        "assetAddress": config.usdc,
    });

    match client.query(query, vars).await {
        Ok(data) => {
            // Collect (address, tvl) pairs and sort by TVL descending
            let mut vault_tvls: Vec<(String, f64)> = Vec::new();
            if let Some(items) = data["vaults"]["items"].as_array() {
                for vault in items {
                    if let Some(addr) = vault["address"].as_str() {
                        let tvl = vault["state"]["totalAssetsUsd"].as_f64().unwrap_or(0.0);
                        vault_tvls.push((addr.to_string(), tvl));
                    }
                }
            }
            vault_tvls.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Return all vaults (balance may be in any of them after dynamic deposit)
            let mut addrs: Vec<String> = vault_tvls.into_iter().map(|(a, _)| a).collect();

            // Always include config default vault (user may have deposited there)
            let default_vault = config.morpho_vault.to_lowercase();
            if !addrs.iter().any(|a| a.to_lowercase() == default_vault) {
                addrs.push(config.morpho_vault.to_string());
            }
            addrs
        }
        Err(e) => {
            eprintln!("[WARN] Failed to query Morpho vaults: {e:#}, using config default");
            vec![config.morpho_vault.to_string()]
        }
    }
}

// ── Protocol detection helpers ──────────────────────────────────────

/// Query all 3 protocol balances and return the one with the largest balance.
async fn detect_current_protocol(config: &'static AutoRebalanceConfig) -> Option<Protocol> {
    let rpc = chains::rpc_url_for(config);
    let aave_chain = config.aave_chain_key;

    // Run all three balance checks concurrently to avoid sequential RPC rate limiting.
    // Free RPC endpoints (e.g. base.llamarpc.com) aggressively rate-limit; sequential
    // checks cause later calls to 429. Concurrent requests share the same time window.
    let rpc_c = rpc.clone();
    let rpc_m = rpc.clone();

    let compound_fut = async {
        match CompoundClient::new_with_onchainos(config.compound_comet, config.usdc, &rpc_c, config.chain_name) {
            Ok(c) => c
                .get_balance()
                .await
                .map(|b| b.to_string().parse::<f64>().unwrap_or(0.0) / 1e6)
                .unwrap_or_else(|e| {
                    eprintln!("[WARN] Compound balance check failed: {e:#}");
                    0.0
                }),
            Err(_) => 0.0,
        }
    };

    let morpho_fut = async {
        // Check multiple Morpho vaults: the daemon may have deposited into a
        // dynamically discovered vault (not the config default). Query the top
        // vaults via GraphQL (no RPC cost) then check balances.
        let vaults = get_morpho_usdc_vaults(config).await;
        let mut best = 0.0f64;
        for vault_addr in &vaults {
            let usd = match MorphoVaultClient::new_with_onchainos(vault_addr, config.usdc, &rpc_m, config.chain_name) {
                Ok(m) => m
                    .get_balance_usdc()
                    .await
                    .map(|b| b.to_string().parse::<f64>().unwrap_or(0.0) / 1e6)
                    .unwrap_or(0.0),
                Err(_) => 0.0,
            };
            if usd > best {
                best = usd;
            }
        }
        best
    };

    let aave_fut = async {
        match AaveClient::new_with_onchainos(aave_chain) {
            Ok(aave) => aave
                .get_usdc_atoken_balance()
                .await
                .map(|b| b.to_string().parse::<f64>().unwrap_or(0.0) / 1e6)
                .unwrap_or_else(|e| {
                    eprintln!("[WARN] Aave balance check failed: {e:#}");
                    0.0
                }),
            Err(_) => 0.0,
        }
    };

    let (compound_usd, morpho_usd, aave_usd) = tokio::join!(compound_fut, morpho_fut, aave_fut);

    eprintln!(
        "Current protocol: {} (aave={:.2}, compound={:.2}, morpho={:.2})",
        if aave_usd >= compound_usd && aave_usd >= morpho_usd && aave_usd > 0.01 {
            "Aave V3"
        } else if compound_usd >= morpho_usd && compound_usd > 0.01 {
            "Compound V3"
        } else if morpho_usd > 0.01 {
            "Morpho"
        } else {
            "none"
        },
        aave_usd,
        compound_usd,
        morpho_usd,
    );

    largest_protocol(aave_usd, compound_usd, morpho_usd)
}

/// Return the protocol with the largest USD balance, or None if all are zero.
fn largest_protocol(aave_usd: f64, compound_usd: f64, morpho_usd: f64) -> Option<Protocol> {
    let threshold = 0.01; // ignore dust
    if aave_usd < threshold && compound_usd < threshold && morpho_usd < threshold {
        return None;
    }
    if aave_usd >= compound_usd && aave_usd >= morpho_usd {
        Some(Protocol::Aave)
    } else if compound_usd >= morpho_usd {
        Some(Protocol::Compound)
    } else {
        Some(Protocol::Morpho)
    }
}

/// Get the USD value of capital in the given protocol.
async fn detect_capital(protocol: Protocol, config: &'static AutoRebalanceConfig) -> f64 {
    let balance = detect_balance(protocol, config).await;
    balance.to_string().parse::<f64>().unwrap_or(0.0) / 1e6
}

/// Get the raw U256 balance (6 decimals for USDC) from the given protocol.
async fn detect_balance(protocol: Protocol, config: &'static AutoRebalanceConfig) -> U256 {
    let rpc = chains::rpc_url_for(config);
    let aave_chain = config.aave_chain_key;

    match protocol {
        Protocol::Aave => match AaveClient::new_with_onchainos(aave_chain) {
            Ok(aave) => {
                // Query the actual aToken balance (includes accrued interest).
                // This is the exact amount that can be withdrawn.
                match aave.get_usdc_atoken_balance().await {
                    Ok(balance) => balance,
                    Err(e) => {
                        eprintln!("[WARN] Aave aToken balance check failed: {e:#}");
                        U256::ZERO
                    }
                }
            }
            Err(_) => U256::ZERO,
        },
        Protocol::Compound => {
            match CompoundClient::new_with_onchainos(config.compound_comet, config.usdc, &rpc, config.chain_name) {
                Ok(c) => c.get_balance().await.unwrap_or(U256::ZERO),
                Err(_) => U256::ZERO,
            }
        }
        Protocol::Morpho => {
            // Check all known vaults — balance may be in a dynamically discovered vault.
            let vaults = get_morpho_usdc_vaults(config).await;
            let mut best = U256::ZERO;
            for vault_addr in &vaults {
                if let Ok(m) = MorphoVaultClient::new_with_onchainos(vault_addr, config.usdc, &rpc, config.chain_name) {
                    if let Ok(b) = m.get_balance_usdc().await {
                        if b > best {
                            best = b;
                        }
                    }
                }
            }
            best
        }
    }
}

/// Estimate real gas cost in USD for a full rebalance (withdraw + approve + supply).
/// Matches TS profit-calculator.ts: gasUnits × gasPrice × ETH price.
async fn estimate_gas_cost_usd(config: &'static AutoRebalanceConfig) -> Result<f64> {
    const TOTAL_GAS_UNITS: u64 = 500_000;

    let gas_price: u128 = crate::onchainos::get_gas_price(config.chain_name)
        .context("failed to get gas price from onchainos")?;

    let gas_cost_wei = gas_price * TOTAL_GAS_UNITS as u128;
    let gas_cost_eth = gas_cost_wei as f64 / 1e18;

    let eth_price = fetch_eth_price_usd().await.unwrap_or(3000.0);

    Ok(gas_cost_eth * eth_price)
}

/// Fetch ETH/USD price from CoinGecko (free, no API key needed).
/// Matches TS utils/gas.ts getEthPriceUSD with 5-minute cache.
async fn fetch_eth_price_usd() -> Result<f64> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = http
        .get("https://api.coingecko.com/api/v3/simple/price?ids=ethereum&vs_currencies=usd")
        .send()
        .await
        .context("CoinGecko ETH price request failed")?;
    let body: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse CoinGecko response")?;
    body["ethereum"]["usd"]
        .as_f64()
        .context("no ETH price in CoinGecko response")
}

/// Deposit idle USDC from wallet into the current protocol (matches TS orchestrator.ts depositIdleBalance).
/// Prevents USDC fragmentation by sweeping leftover wallet balance after holds.
async fn deposit_idle_balance(
    current_proto: Protocol,
    config: &'static AutoRebalanceConfig,
    yields: &[yield_monitor::YieldSnapshot],
    notifier: &Notifier,
) {
    const MIN_DEPOSIT_USD: f64 = 1.0; // Don't deposit dust (matches TS MIN_DEPOSIT_USD = 1)

    let wallet_usdc = detect_wallet_usdc(config).await;
    if wallet_usdc < MIN_DEPOSIT_USD {
        return;
    }

    eprintln!(
        "[{}] Found ${:.2} idle USDC in wallet, depositing into {}",
        chrono::Utc::now().to_rfc3339(),
        wallet_usdc,
        current_proto,
    );

    let raw_amount = detect_wallet_usdc_raw(config).await;
    if raw_amount.is_zero() {
        return;
    }

    // Get Morpho vault address from yields if depositing into Morpho
    let morpho_vault = yields
        .iter()
        .find(|y| y.protocol == Protocol::Morpho)
        .and_then(|y| y.vault_address.clone());

    match executor::deposit_only(current_proto, raw_amount, config, morpho_vault.as_deref()).await {
        Ok(result) => {
            eprintln!(
                "[{}] Idle USDC deposited: {} USDC into {}",
                chrono::Utc::now().to_rfc3339(),
                result.amount_usdc,
                current_proto,
            );
        }
        Err(e) => {
            eprintln!(
                "[{}] Failed to deposit idle USDC: {e:#}",
                chrono::Utc::now().to_rfc3339(),
            );
            notifier
                .notify(
                    NotifyLevel::Warning,
                    &format!("Failed to deposit ${:.2} idle USDC: {e:#}", wallet_usdc),
                )
                .await;
        }
    }
}

/// Get the wallet's USDC balance in USD (for initial deposit when protocol=None).
async fn detect_wallet_usdc(config: &'static AutoRebalanceConfig) -> f64 {
    let balance = detect_wallet_usdc_raw(config).await;
    balance.to_string().parse::<f64>().unwrap_or(0.0) / 1e6
}

/// Get the wallet's raw USDC balance (U256, 6 decimals).
async fn detect_wallet_usdc_raw(config: &'static AutoRebalanceConfig) -> U256 {
    if let Ok(balances) = crate::onchainos::get_token_balances(config.chain_name) {
        if let Some(usdc) = balances.iter().find(|b| b.symbol.eq_ignore_ascii_case("USDC")) {
            let raw = (usdc.balance * 1e6) as u64;
            return U256::from(raw);
        }
    }
    U256::ZERO
}
