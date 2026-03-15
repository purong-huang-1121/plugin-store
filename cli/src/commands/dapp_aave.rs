use anyhow::Result;
use clap::Subcommand;

use crate::dapp::aave::client::{parse_token_amount, AaveClient};
use crate::output;

#[derive(Subcommand)]
pub enum AaveCommand {
    /// List all Aave V3 reserve markets with APY rates
    Markets {
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
    /// Get user account summary (collateral, debt, health factor)
    Account {
        /// User wallet address
        address: String,
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
    /// Get detailed data for a specific reserve
    Reserve {
        /// Token symbol (e.g. WETH, USDC, DAI)
        symbol: String,
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
    /// Supply an asset to Aave V3 (requires EVM_PRIVATE_KEY)
    Supply {
        /// Token symbol (e.g. WETH, USDC, DAI)
        #[arg(long)]
        token: String,
        /// Amount to supply (e.g. "100.5")
        #[arg(long)]
        amount: String,
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
    /// Withdraw an asset from Aave V3 (requires EVM_PRIVATE_KEY)
    Withdraw {
        /// Token symbol (e.g. WETH, USDC, DAI)
        #[arg(long)]
        token: String,
        /// Amount to withdraw (e.g. "100.5", or "max" for full withdrawal)
        #[arg(long)]
        amount: String,
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
    /// Borrow an asset from Aave V3 at variable rate (requires EVM_PRIVATE_KEY)
    Borrow {
        /// Token symbol (e.g. WETH, USDC, DAI)
        #[arg(long)]
        token: String,
        /// Amount to borrow (e.g. "100.5")
        #[arg(long)]
        amount: String,
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
    /// Repay a borrowed asset to Aave V3 (requires EVM_PRIVATE_KEY)
    Repay {
        /// Token symbol (e.g. WETH, USDC, DAI)
        #[arg(long)]
        token: String,
        /// Amount to repay (e.g. "100.5", or "max" to repay full debt)
        #[arg(long)]
        amount: String,
        /// Chain: ethereum, polygon, arbitrum
        #[arg(long, default_value = "ethereum")]
        chain: String,
    },
}

pub async fn execute(cmd: AaveCommand) -> Result<()> {
    match cmd {
        AaveCommand::Markets { chain } => cmd_markets(&chain).await,
        AaveCommand::Account { address, chain } => cmd_account(&address, &chain).await,
        AaveCommand::Reserve { symbol, chain } => cmd_reserve(&symbol, &chain).await,
        AaveCommand::Supply {
            token,
            amount,
            chain,
        } => cmd_supply(&token, &amount, &chain).await,
        AaveCommand::Withdraw {
            token,
            amount,
            chain,
        } => cmd_withdraw(&token, &amount, &chain).await,
        AaveCommand::Borrow {
            token,
            amount,
            chain,
        } => cmd_borrow(&token, &amount, &chain).await,
        AaveCommand::Repay {
            token,
            amount,
            chain,
        } => cmd_repay(&token, &amount, &chain).await,
    }
}

async fn cmd_markets(chain: &str) -> Result<()> {
    let client = AaveClient::new(chain)?;
    let data = client.get_reserves_data().await?;
    output::success(data);
    Ok(())
}

async fn cmd_account(address: &str, chain: &str) -> Result<()> {
    let client = AaveClient::new(chain)?;
    let data = client.get_account_data(address).await?;
    output::success(data);
    Ok(())
}

async fn cmd_reserve(symbol: &str, chain: &str) -> Result<()> {
    let client = AaveClient::new(chain)?;
    let data = client.get_reserve_by_symbol(symbol).await?;
    output::success(data);
    Ok(())
}

async fn cmd_supply(token: &str, amount: &str, chain: &str) -> Result<()> {
    let client = AaveClient::new_with_signer(chain)?;
    let (asset, decimals) = client.resolve_asset(token).await?;
    let amount_u256 = parse_token_amount(amount, decimals)?;
    let data = client.supply(asset, amount_u256, decimals).await?;
    output::success(data);
    Ok(())
}

async fn cmd_withdraw(token: &str, amount: &str, chain: &str) -> Result<()> {
    let client = AaveClient::new_with_signer(chain)?;
    let (asset, decimals) = client.resolve_asset(token).await?;
    let amount_u256 = if amount == "max" {
        alloy::primitives::U256::MAX
    } else {
        parse_token_amount(amount, decimals)?
    };
    let data = client.withdraw(asset, amount_u256, decimals).await?;
    output::success(data);
    Ok(())
}

async fn cmd_borrow(token: &str, amount: &str, chain: &str) -> Result<()> {
    let client = AaveClient::new_with_signer(chain)?;
    let (asset, decimals) = client.resolve_asset(token).await?;
    let amount_u256 = parse_token_amount(amount, decimals)?;
    let data = client.borrow(asset, amount_u256, decimals).await?;
    output::success(data);
    Ok(())
}

async fn cmd_repay(token: &str, amount: &str, chain: &str) -> Result<()> {
    let client = AaveClient::new_with_signer(chain)?;
    let (asset, decimals) = client.resolve_asset(token).await?;
    let amount_u256 = if amount == "max" {
        alloy::primitives::U256::MAX
    } else {
        parse_token_amount(amount, decimals)?
    };
    let data = client.repay(asset, amount_u256, decimals).await?;
    output::success(data);
    Ok(())
}
