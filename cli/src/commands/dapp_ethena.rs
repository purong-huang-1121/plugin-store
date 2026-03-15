use anyhow::Result;
use clap::Subcommand;

use crate::dapp::ethena::client::EthenaClient;
use crate::output;

#[derive(Subcommand)]
pub enum EthenaCommand {
    /// View sUSDe yield info: exchange rate, total assets, cooldown period
    Apy,
    /// View sUSDe and USDe balances for an address
    Balance {
        /// Wallet address
        address: String,
    },
    /// Stake USDe → sUSDe (requires EVM_PRIVATE_KEY)
    Stake {
        /// Amount of USDe to stake (e.g. "100.5")
        #[arg(long)]
        amount: String,
    },
    /// Initiate unstake cooldown (requires EVM_PRIVATE_KEY)
    Cooldown {
        /// Amount of USDe to unstake (e.g. "100.5")
        #[arg(long)]
        amount: String,
    },
    /// Withdraw USDe after cooldown period has elapsed (requires EVM_PRIVATE_KEY)
    Unstake,
}

pub async fn execute(cmd: EthenaCommand) -> Result<()> {
    match cmd {
        EthenaCommand::Apy => cmd_apy().await,
        EthenaCommand::Balance { address } => cmd_balance(&address).await,
        EthenaCommand::Stake { amount } => cmd_stake(&amount).await,
        EthenaCommand::Cooldown { amount } => cmd_cooldown(&amount).await,
        EthenaCommand::Unstake => cmd_unstake().await,
    }
}

async fn cmd_apy() -> Result<()> {
    let client = EthenaClient::new()?;
    let data = client.get_yield_info().await?;
    output::success(data);
    Ok(())
}

async fn cmd_balance(address: &str) -> Result<()> {
    let client = EthenaClient::new()?;
    let data = client.get_balance(address).await?;
    output::success(data);
    Ok(())
}

async fn cmd_stake(amount: &str) -> Result<()> {
    let client = EthenaClient::new_with_signer()?;
    let amount_u256 = parse_usde_amount(amount)?;
    let data = client.stake(amount_u256).await?;
    output::success(data);
    Ok(())
}

async fn cmd_cooldown(amount: &str) -> Result<()> {
    let client = EthenaClient::new_with_signer()?;
    let amount_u256 = parse_usde_amount(amount)?;
    let data = client.cooldown(amount_u256).await?;
    output::success(data);
    Ok(())
}

async fn cmd_unstake() -> Result<()> {
    let client = EthenaClient::new_with_signer()?;
    let data = client.unstake().await?;
    output::success(data);
    Ok(())
}

/// Parse a decimal USDe amount string to U256 (18 decimals).
fn parse_usde_amount(amount_str: &str) -> Result<alloy::primitives::U256> {
    use alloy::primitives::U256;
    use anyhow::{bail, Context};
    use std::str::FromStr;

    let parts: Vec<&str> = amount_str.split('.').collect();
    let decimals: usize = 18;
    match parts.len() {
        1 => {
            let whole = U256::from_str(parts[0]).context("invalid amount")?;
            Ok(whole * U256::from(10).pow(U256::from(decimals)))
        }
        2 => {
            let whole = U256::from_str(parts[0]).context("invalid whole part")?;
            let frac_str = parts[1];
            if frac_str.len() > decimals {
                bail!(
                    "Too many decimal places: {} (max {})",
                    frac_str.len(),
                    decimals
                );
            }
            let padded = format!("{:0<width$}", frac_str, width = decimals);
            let frac = U256::from_str(&padded).context("invalid fractional part")?;
            Ok(whole * U256::from(10).pow(U256::from(decimals)) + frac)
        }
        _ => bail!("Invalid amount format: {}", amount_str),
    }
}
