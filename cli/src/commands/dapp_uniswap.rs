use alloy::primitives::{Address, U256};
use anyhow::{Context as _, Result};
use clap::Subcommand;
use serde_json::json;
use std::str::FromStr;

use crate::dapp::uniswap::client::{self, UniswapClient};
use crate::output;

#[derive(Subcommand)]
pub enum UniswapCommand {
    /// Get a swap quote (estimated output, no execution)
    Quote {
        /// Input token symbol (e.g. WETH) or contract address
        #[arg(long)]
        from: String,
        /// Output token symbol (e.g. wstETH) or contract address
        #[arg(long)]
        to: String,
        /// Amount of input token (human-readable, e.g. "0.05")
        #[arg(long)]
        amount: String,
        /// Chain: arbitrum, ethereum, polygon
        #[arg(long, default_value = "arbitrum")]
        chain: String,
        /// Pool fee tier in bps: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%)
        #[arg(long, default_value = "100")]
        fee: u32,
    },
    /// Execute a swap on Uniswap V3
    Swap {
        /// Input token symbol (e.g. WETH) or contract address
        #[arg(long)]
        from: String,
        /// Output token symbol (e.g. wstETH) or contract address
        #[arg(long)]
        to: String,
        /// Amount of input token (human-readable, e.g. "0.05")
        #[arg(long)]
        amount: String,
        /// Chain: arbitrum, ethereum, polygon
        #[arg(long, default_value = "arbitrum")]
        chain: String,
        /// Pool fee tier in bps: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%)
        #[arg(long, default_value = "100")]
        fee: u32,
        /// Slippage tolerance in bps (default 50 = 0.5%)
        #[arg(long, default_value = "50")]
        slippage: u32,
    },
    /// List well-known token addresses for a chain
    Tokens {
        /// Chain: arbitrum, ethereum, polygon
        #[arg(long, default_value = "arbitrum")]
        chain: String,
    },
}

pub async fn execute(cmd: UniswapCommand) -> Result<()> {
    match cmd {
        UniswapCommand::Quote {
            from,
            to,
            amount,
            chain,
            fee,
        } => cmd_quote(&from, &to, &amount, &chain, fee).await,
        UniswapCommand::Swap {
            from,
            to,
            amount,
            chain,
            fee,
            slippage,
        } => cmd_swap(&from, &to, &amount, &chain, fee, slippage).await,
        UniswapCommand::Tokens { chain } => cmd_tokens(&chain),
    }
}

// ---------------------------------------------------------------------------
// Token resolution: symbol or address
// ---------------------------------------------------------------------------

fn resolve_token(token: &str, chain_id: u64) -> Result<(Address, u8)> {
    // If it starts with 0x, treat as address — default to 18 decimals
    if token.starts_with("0x") || token.starts_with("0X") {
        let addr = Address::from_str(token).context("invalid token address")?;
        return Ok((addr, 18));
    }
    client::resolve_token(token, chain_id)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn cmd_quote(from: &str, to: &str, amount: &str, chain: &str, fee: u32) -> Result<()> {
    let config = client::get_chain_config(chain)?;
    let (token_in, decimals_in) = resolve_token(from, config.chain_id)?;
    let (token_out, decimals_out) = resolve_token(to, config.chain_id)?;
    let amount_in = parse_amount(amount, decimals_in)?;

    let uniswap = UniswapClient::new(chain)?;
    let quote = uniswap.quote(token_in, token_out, amount_in, fee).await?;

    let amount_out_raw =
        U256::from_str(quote["amount_out"].as_str().unwrap_or("0")).unwrap_or(U256::ZERO);

    output::success(json!({
        "from": from.to_uppercase(),
        "to": to.to_uppercase(),
        "amount_in": amount,
        "amount_out": format_units(amount_out_raw, decimals_out),
        "fee_tier": format!("{}bps ({}%)", fee, fee as f64 / 10000.0),
        "chain": chain,
    }));
    Ok(())
}

async fn cmd_swap(
    from: &str,
    to: &str,
    amount: &str,
    chain: &str,
    fee: u32,
    slippage: u32,
) -> Result<()> {
    let config = client::get_chain_config(chain)?;
    let (token_in, decimals_in) = resolve_token(from, config.chain_id)?;
    let (token_out, decimals_out) = resolve_token(to, config.chain_id)?;
    let amount_in = parse_amount(amount, decimals_in)?;

    let uniswap = UniswapClient::new(chain)?;
    let result = uniswap
        .swap(
            token_in,
            token_out,
            amount_in,
            fee,
            slippage,
            decimals_in,
            decimals_out,
        )
        .await?;

    output::success(result);
    Ok(())
}

fn cmd_tokens(chain: &str) -> Result<()> {
    let config = client::get_chain_config(chain)?;

    let tokens: Vec<(&str, &str)> = match config.chain_id {
        42161 => vec![
            ("WETH", "0x82aF49447D8a07e3bd95BD0d56f35241523fBab1"),
            ("USDC", "0xaf88d065e77c8cC2239327C5EDb3A432268e5831"),
            ("USDC.e", "0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8"),
            ("USDT", "0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9"),
            ("wstETH", "0x5979D7b546E38E414F7E9822514be443A4800529"),
            ("weETH", "0x35751007a407ca6FEFfE80b3cB397736D2cf4dbe"),
            ("WBTC", "0x2f2a2543B76A4166549F7aaB2e75Bef0aefC5B0f"),
            ("ARB", "0x912CE59144191C1204E64559FE8253a0e49E6548"),
        ],
        1 => vec![
            ("WETH", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"),
            ("USDC", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
            ("USDT", "0xdAC17F958D2ee523a2206206994597C13D831ec7"),
            ("wstETH", "0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0"),
            ("weETH", "0xCd5fE23C85820F7B72D0926FC9b05b43E359b7ee"),
            ("WBTC", "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"),
            ("DAI", "0x6B175474E89094C44Da98b954EedeAC495271d0F"),
            ("sUSDe", "0x9D39A5DE30e57443BfF2A8307A4256c8797A3497"),
            ("USDe", "0x4c9EDD5852cd905f086C759E8383e09bff1E68B3"),
        ],
        137 => vec![
            ("WETH", "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619"),
            ("USDC", "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359"),
            ("USDT", "0xc2132D05D31c914a87C6611C10748AEb04B58e8F"),
            ("WMATIC", "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270"),
            ("wstETH", "0x03b54A6e9a984069379fae1a4fC4dBAE93B3bCCD"),
        ],
        _ => vec![],
    };

    let list: Vec<serde_json::Value> = tokens
        .iter()
        .map(|(sym, addr)| {
            json!({
                "symbol": sym,
                "address": addr,
            })
        })
        .collect();

    output::success(json!({
        "chain": chain,
        "chain_id": config.chain_id,
        "tokens": list,
    }));
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_amount(amount: &str, decimals: u8) -> Result<U256> {
    let parts: Vec<&str> = amount.split('.').collect();
    match parts.len() {
        1 => {
            let whole = U256::from_str(parts[0]).context("invalid amount")?;
            Ok(whole * U256::from(10).pow(U256::from(decimals)))
        }
        2 => {
            let whole = U256::from_str(parts[0]).context("invalid whole part")?;
            let frac_str = parts[1];
            let frac_len = frac_str.len();
            if frac_len > decimals as usize {
                anyhow::bail!("Too many decimal places: {} (max {})", frac_len, decimals);
            }
            let padded = format!("{:0<width$}", frac_str, width = decimals as usize);
            let frac = U256::from_str(&padded).context("invalid fractional part")?;
            Ok(whole * U256::from(10).pow(U256::from(decimals)) + frac)
        }
        _ => anyhow::bail!("Invalid amount: {}", amount),
    }
}

fn format_units(value: U256, decimals: u8) -> String {
    if decimals == 0 {
        return value.to_string();
    }
    let divisor = U256::from(10).pow(U256::from(decimals));
    let whole = value / divisor;
    let frac = value % divisor;
    let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
    let trimmed = frac_str.trim_end_matches('0');
    if trimmed.is_empty() {
        whole.to_string()
    } else {
        format!("{}.{}", whole, trimmed)
    }
}
