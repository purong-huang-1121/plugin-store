use clap::Parser;
use plugin_store_cli::commands::strategy_auto_rebalance::{execute, AutoRebalanceCommand};
use plugin_store_cli::output;

#[derive(Parser)]
#[command(
    name = "strategy-auto-rebalance",
    version,
    about = "Auto-rebalance USDC across Aave, Compound, Morpho on Base"
)]
struct Cli {
    #[command(subcommand)]
    command: AutoRebalanceCommand,
}

#[tokio::main]
async fn main() {
    plugin_store_cli::update::check("strategy-auto-rebalance", env!("CARGO_PKG_VERSION"));
    let cli = Cli::parse();
    if let Err(e) = execute(cli.command).await {
        output::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}
