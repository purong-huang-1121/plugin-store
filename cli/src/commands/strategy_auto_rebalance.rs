use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::strategy::auto_rebalance::chains;
use crate::strategy::auto_rebalance::config::AutoRebalanceConfig;

#[derive(Subcommand)]
pub enum AutoRebalanceCommand {
    /// Start the auto-rebalance daemon (foreground process)
    Start {
        /// Check interval in seconds (overrides config file)
        #[arg(long)]
        interval: Option<u64>,
        /// Minimum APY spread to trigger rebalance (%, overrides config file)
        #[arg(long)]
        min_spread: Option<f64>,
        /// Maximum break-even days (overrides config file)
        #[arg(long)]
        max_break_even: Option<u64>,
        /// Telegram Bot API token (or set TELEGRAM_BOT_TOKEN env var)
        #[arg(long)]
        telegram_token: Option<String>,
        /// Telegram chat ID (or set TELEGRAM_CHAT_ID env var)
        #[arg(long)]
        telegram_chat: Option<String>,
        /// Chain: base, ethereum (overrides config file)
        #[arg(long)]
        chain: Option<String>,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status and recent activity
    Status,
    /// Show current configuration
    Config,
    /// Set a config parameter: plugin-store auto-rebalance set --key interval_secs --value 300
    Set {
        /// Parameter name
        #[arg(long)]
        key: String,
        /// New value
        #[arg(long)]
        value: String,
    },
}

pub async fn execute(cmd: AutoRebalanceCommand) -> Result<()> {
    match cmd {
        AutoRebalanceCommand::Start {
            interval,
            min_spread,
            max_break_even,
            telegram_token,
            telegram_chat,
            chain,
            yes,
        } => {
            // Load saved config, then override with CLI args
            let cfg = AutoRebalanceConfig::load()?;
            let interval = interval.unwrap_or(cfg.interval_secs);
            let min_spread = min_spread.unwrap_or(cfg.min_spread);
            let max_break_even = max_break_even.unwrap_or(cfg.max_break_even);
            let chain = chain.unwrap_or(cfg.chain.clone());
            let telegram_token = telegram_token.or(cfg.telegram_token.clone());
            let telegram_chat = telegram_chat.or(cfg.telegram_chat.clone());

            let chain_config = chains::get_config(&chain)?;

            // Check not already running (before showing config)
            if let Some(pid) = crate::strategy::auto_rebalance::daemon::check_running() {
                anyhow::bail!(
                    "Auto-rebalance daemon already running (PID {}). Use 'auto-rebalance stop' first.",
                    pid
                );
            }

            // Derive wallet address from onchainos wallet
            let wallet_address = derive_wallet_address()?;

            // Resolve Telegram config
            let tg_token = telegram_token
                .clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
            let tg_chat = telegram_chat
                .clone()
                .or_else(|| std::env::var("TELEGRAM_CHAT_ID").ok());
            let telegram_enabled = tg_token.is_some() && tg_chat.is_some();

            // Format interval for display
            let interval_display = if interval >= 3600 {
                format!("{}h", interval / 3600)
            } else if interval >= 60 {
                format!("{}m", interval / 60)
            } else {
                format!("{}s", interval)
            };

            // RPC URL (may be overridden by env)
            let rpc_url = chains::rpc_url_for(chain_config);

            // File paths
            let state_path = crate::strategy::auto_rebalance::state::StateData::default_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "~/.plugin-store/auto-rebalance-state.json".to_string());

            // Build config summary
            let config_summary = json!({
                "chain": chain_config.chain_name,
                "chain_id": chain_config.chain_id,
                "wallet": wallet_address,
                "interval": format!("{interval}s ({interval_display})"),
                "min_spread": format!("{min_spread}%"),
                "max_break_even": format!("{max_break_even} days"),
                "protocols": ["Aave V3", "Compound V3", "Morpho"],
                "gas_spike_threshold": format!("{} gwei", chain_config.gas_spike_gwei),
                "tvl_safety": "Emergency withdraw if TVL drops >30%",
                "telegram": if telegram_enabled { "enabled" } else { "disabled" },
                "rpc_url": rpc_url,
                "config_file": AutoRebalanceConfig::config_path().display().to_string(),
                "log_file": AutoRebalanceConfig::log_path().display().to_string(),
                "state_file": state_path,
                "pid_file": crate::strategy::auto_rebalance::daemon::pid_path().display().to_string(),
            });

            // Output config for confirmation
            crate::output::success(json!({
                "action": "confirm_start",
                "config": config_summary,
            }));

            // If --yes not passed, wait for stdin confirmation
            if !yes {
                eprint!("\nProceed? (y/n): ");
                let mut input = String::new();
                std::io::stdin()
                    .read_line(&mut input)
                    .context("failed to read input")?;
                let input = input.trim().to_lowercase();
                if input != "y" && input != "yes" {
                    crate::output::error("Cancelled by user");
                    return Ok(());
                }
            }

            crate::strategy::auto_rebalance::daemon::start(
                interval,
                min_spread,
                max_break_even,
                telegram_token,
                telegram_chat,
                &chain,
            )
            .await
        }
        AutoRebalanceCommand::Stop => crate::strategy::auto_rebalance::daemon::stop().await,
        AutoRebalanceCommand::Status => crate::strategy::auto_rebalance::daemon::status().await,
        AutoRebalanceCommand::Config => cmd_config().await,
        AutoRebalanceCommand::Set { key, value } => cmd_set(&key, &value).await,
    }
}

// ── config ──────────────────────────────────────────────────────────

async fn cmd_config() -> Result<()> {
    let cfg = AutoRebalanceConfig::load()?;
    let path = AutoRebalanceConfig::config_path();
    let is_custom = path.exists();

    crate::output::success(json!({
        "config_file": path.to_string_lossy(),
        "log_file": AutoRebalanceConfig::log_path().to_string_lossy(),
        "state_file": crate::strategy::auto_rebalance::state::StateData::default_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.plugin-store/auto-rebalance-state.json".to_string()),
        "pid_file": crate::strategy::auto_rebalance::daemon::pid_path().display().to_string(),
        "is_custom": is_custom,
        "parameters": {
            "daemon": {
                "interval_secs": cfg.interval_secs,
                "chain": cfg.chain,
            },
            "engine": {
                "min_spread": cfg.min_spread,
                "max_break_even": cfg.max_break_even,
                "max_gas_cost_usd": cfg.max_gas_cost_usd,
                "min_rebalance_interval_secs": cfg.min_rebalance_interval_secs,
            },
            "telegram": {
                "token": if cfg.telegram_token.is_some() { "configured" } else { "not set" },
                "chat": if cfg.telegram_chat.is_some() { "configured" } else { "not set" },
            },
        }
    }));
    Ok(())
}

// ── set ─────────────────────────────────────────────────────────────

async fn cmd_set(key: &str, value: &str) -> Result<()> {
    let mut cfg = AutoRebalanceConfig::load()?;

    match key {
        "interval_secs" => cfg.interval_secs = value.parse().context("invalid u64")?,
        "min_spread" => cfg.min_spread = value.parse().context("invalid f64")?,
        "max_break_even" => cfg.max_break_even = value.parse().context("invalid u64")?,
        "chain" => {
            // Validate chain name
            chains::get_config(value)?;
            cfg.chain = value.to_string();
        }
        "max_gas_cost_usd" => cfg.max_gas_cost_usd = value.parse().context("invalid f64")?,
        "min_rebalance_interval_secs" => {
            cfg.min_rebalance_interval_secs = value.parse().context("invalid u64")?
        }
        "telegram_token" => cfg.telegram_token = Some(value.to_string()),
        "telegram_chat" => cfg.telegram_chat = Some(value.to_string()),
        _ => bail!(
            "Unknown parameter '{}'. Use 'plugin-store auto-rebalance config' to see available parameters.",
            key
        ),
    }

    cfg.save()?;
    crate::output::success(json!({
        "message": format!("Set {} = {}", key, value),
        "config_file": AutoRebalanceConfig::config_path().to_string_lossy(),
    }));
    Ok(())
}

/// Derive wallet address from onchainos wallet.
fn derive_wallet_address() -> Result<String> {
    crate::onchainos::get_evm_address()
        .context("onchainos wallet not available — please login first")
}
