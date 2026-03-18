use clap::Parser;
use plugin_store_cli::commands::strategy_ranking_sniper::{execute, RankingSniperCommand};
use plugin_store_cli::output;

#[derive(Parser)]
#[command(
    name = "strategy-ranking-sniper",
    version,
    about = "SOL ranking sniper — buy trending Solana tokens with safety checks"
)]
struct Cli {
    #[command(subcommand)]
    command: RankingSniperCommand,
}

#[tokio::main]
async fn main() {
    plugin_store_cli::update::check("strategy-ranking-sniper", env!("CARGO_PKG_VERSION"));
    let cli = Cli::parse();
    if let Err(e) = execute(cli.command).await {
        output::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}
