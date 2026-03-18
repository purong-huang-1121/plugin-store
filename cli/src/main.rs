use clap::{Parser, Subcommand, ValueEnum};
use plugin_store_cli::{commands, output};

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
    /// Aave V3 lending protocol
    Aave {
        #[command(subcommand)]
        command: commands::dapp_aave::AaveCommand,
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
    /// Uniswap V3 on-chain swap and quote
    Uniswap {
        #[command(subcommand)]
        command: commands::dapp_uniswap::UniswapCommand,
    },
}

#[tokio::main]
async fn main() {
    plugin_store_cli::update::check("plugin-store", env!("CARGO_PKG_VERSION"));
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Aave { command } => commands::dapp_aave::execute(command).await,
        Commands::Ethena { command } => commands::dapp_ethena::execute(command).await,
        Commands::Morpho { command } => commands::dapp_morpho::execute(command).await,
        Commands::Uniswap { command } => commands::dapp_uniswap::execute(command).await,
    };

    if let Err(e) = result {
        output::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}
