#![allow(dead_code)]

pub mod chains;
mod client;
mod commands;
mod config;
mod dapp;
pub mod notifier;
mod output;
mod strategy;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "plugin-store",
    version,
    about = "onchainOS CLI — on-chain DeFi operations"
)]
pub struct Cli {
    /// Output format
    #[arg(short, long, global = true, default_value = "json")]
    pub output: OutputFormat,

    /// Backend service URL (overrides config)
    #[arg(long, global = true)]
    pub base_url: Option<String>,

    /// Chain: ethereum, solana, base, bsc, polygon, arbitrum, sui, etc.
    #[arg(long, global = true)]
    pub chain: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Json,
    Table,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Polymarket prediction markets
    Polymarket {
        #[command(subcommand)]
        command: commands::dapp_polymarket::PolymarketCommand,
    },
    /// Aave V3 lending protocol
    Aave {
        #[command(subcommand)]
        command: commands::dapp_aave::AaveCommand,
    },
    /// Hyperliquid perpetual and spot exchange
    Hyperliquid {
        #[command(subcommand)]
        command: commands::dapp_hyperliquid::HyperliquidCommand,
    },
    /// Kalshi regulated prediction markets (US)
    Kalshi {
        /// API environment: demo (default) or prod
        #[arg(long, default_value = "demo")]
        env: dapp::kalshi::auth::KalshiEnv,
        #[command(subcommand)]
        command: commands::dapp_kalshi::KalshiCommand,
    },
    /// Ethena sUSDe staking (yield-bearing stablecoin)
    Ethena {
        #[command(subcommand)]
        command: commands::dapp_ethena::EthenaCommand,
    },
    /// Morpho Protocol — permissionless lending markets and MetaMorpho vaults
    Morpho {
        #[command(subcommand)]
        command: commands::dapp_morpho::MorphoCommand,
    },
    /// Auto-rebalance USDC across Aave, Compound, Morpho
    AutoRebalance {
        #[command(subcommand)]
        command: commands::strategy_auto_rebalance::AutoRebalanceCommand,
    },
    /// ETH/USDC grid trading bot on Base
    Grid {
        #[command(subcommand)]
        command: commands::strategy_grid::GridCommand,
    },
    /// Uniswap V3 on-chain swap and quote
    Uniswap {
        #[command(subcommand)]
        command: commands::dapp_uniswap::UniswapCommand,
    },
    /// SOL ranking sniper — buy trending Solana tokens with safety checks
    RankingSniper {
        #[command(subcommand)]
        command: commands::strategy_ranking_sniper::RankingSniperCommand,
    },
    /// SOL signal tracker — follow smart money signals with safety filter
    SignalTracker {
        #[command(subcommand)]
        command: commands::strategy_signal_tracker::SignalTrackerCommand,
    },
    /// SOL memepump scanner — automated pump.fun token trading
    Scanner {
        #[command(subcommand)]
        command: commands::strategy_memepump_scanner::ScannerCommand,
    },
}

#[tokio::main]
async fn main() {
    // Load .env from current directory
    dotenvy::dotenv().ok();
    // Also load .env from executable directory (for deployed binary)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let env_path = dir.join(".env");
            if env_path.exists() {
                dotenvy::from_path(&env_path).ok();
            }
        }
    }

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Polymarket { command } => commands::dapp_polymarket::execute(command).await,
        Commands::Aave { command } => commands::dapp_aave::execute(command).await,
        Commands::Hyperliquid { command } => commands::dapp_hyperliquid::execute(command).await,
        Commands::Kalshi { env, command } => commands::dapp_kalshi::execute(command, env).await,
        Commands::Ethena { command } => commands::dapp_ethena::execute(command).await,
        Commands::Morpho { command } => commands::dapp_morpho::execute(command).await,
        Commands::AutoRebalance { command } => {
            commands::strategy_auto_rebalance::execute(command).await
        }
        Commands::Grid { command } => commands::strategy_grid::execute(command).await,
        Commands::Uniswap { command } => commands::dapp_uniswap::execute(command).await,
        Commands::RankingSniper { command } => {
            commands::strategy_ranking_sniper::execute(command).await
        }
        Commands::SignalTracker { command } => {
            commands::strategy_signal_tracker::execute(command).await
        }
        Commands::Scanner { command } => {
            commands::strategy_memepump_scanner::execute(command).await
        }
    };

    if let Err(e) = result {
        output::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}
