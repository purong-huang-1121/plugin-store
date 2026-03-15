//! Uniswap V3 client for on-chain swaps via SwapRouter02.

use std::str::FromStr;

use alloy::network::EthereumWallet;
use alloy::primitives::{Address, U160, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::SolCall;
use anyhow::{bail, Context, Result};
use serde_json::json;

use super::contracts::{IQuoterV2, ISwapRouter02, IERC20};

// ---------------------------------------------------------------------------
// Chain configurations
// ---------------------------------------------------------------------------

pub struct UniswapChainConfig {
    pub chain_id: u64,
    pub rpc_url: &'static str,
    pub swap_router: &'static str,
    pub quoter_v2: &'static str,
}

static ARBITRUM: UniswapChainConfig = UniswapChainConfig {
    chain_id: 42161,
    rpc_url: "https://arbitrum-one-rpc.publicnode.com",
    swap_router: "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45",
    quoter_v2: "0x61fFE014bA17989E743c5F6cB21bF9697530B21e",
};

static ETHEREUM: UniswapChainConfig = UniswapChainConfig {
    chain_id: 1,
    rpc_url: "https://ethereum-rpc.publicnode.com",
    swap_router: "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45",
    quoter_v2: "0x61fFE014bA17989E743c5F6cB21bF9697530B21e",
};

static POLYGON: UniswapChainConfig = UniswapChainConfig {
    chain_id: 137,
    rpc_url: "https://polygon-bor-rpc.publicnode.com",
    swap_router: "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45",
    quoter_v2: "0x61fFE014bA17989E743c5F6cB21bF9697530B21e",
};

pub fn get_chain_config(chain: &str) -> Result<&'static UniswapChainConfig> {
    match chain.to_lowercase().as_str() {
        "arbitrum" | "arb" | "42161" => Ok(&ARBITRUM),
        "ethereum" | "eth" | "1" => Ok(&ETHEREUM),
        "polygon" | "matic" | "137" => Ok(&POLYGON),
        _ => bail!(
            "Unsupported chain '{}' for Uniswap. Supported: arbitrum, ethereum, polygon",
            chain
        ),
    }
}

// ---------------------------------------------------------------------------
// Well-known token addresses (per chain)
// ---------------------------------------------------------------------------

pub fn resolve_token(symbol: &str, chain_id: u64) -> Result<(Address, u8)> {
    let upper = symbol.to_uppercase();
    match (upper.as_str(), chain_id) {
        // Arbitrum
        ("WETH", 42161) => Ok((addr("0x82aF49447D8a07e3bd95BD0d56f35241523fBab1"), 18)),
        ("USDC", 42161) => Ok((addr("0xaf88d065e77c8cC2239327C5EDb3A432268e5831"), 6)),
        ("USDC.E", 42161) => Ok((addr("0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8"), 6)),
        ("USDT", 42161) => Ok((addr("0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9"), 6)),
        ("WSTETH", 42161) => Ok((addr("0x5979D7b546E38E414F7E9822514be443A4800529"), 18)),
        ("WEETH", 42161) => Ok((addr("0x35751007a407ca6FEFfE80b3cB397736D2cf4dbe"), 18)),
        ("WBTC", 42161) => Ok((addr("0x2f2a2543B76A4166549F7aaB2e75Bef0aefC5B0f"), 8)),
        ("ARB", 42161) => Ok((addr("0x912CE59144191C1204E64559FE8253a0e49E6548"), 18)),
        // Ethereum
        ("WETH", 1) => Ok((addr("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"), 18)),
        ("USDC", 1) => Ok((addr("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"), 6)),
        ("USDT", 1) => Ok((addr("0xdAC17F958D2ee523a2206206994597C13D831ec7"), 6)),
        ("WSTETH", 1) => Ok((addr("0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0"), 18)),
        ("WEETH", 1) => Ok((addr("0xCd5fE23C85820F7B72D0926FC9b05b43E359b7ee"), 18)),
        ("WBTC", 1) => Ok((addr("0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"), 8)),
        ("DAI", 1) => Ok((addr("0x6B175474E89094C44Da98b954EedeAC495271d0F"), 18)),
        ("SUSDE", 1) => Ok((addr("0x9D39A5DE30e57443BfF2A8307A4256c8797A3497"), 18)),
        ("USDE", 1) => Ok((addr("0x4c9EDD5852cd905f086C759E8383e09bff1E68B3"), 18)),
        // Polygon
        ("WETH", 137) => Ok((addr("0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619"), 18)),
        ("USDC", 137) => Ok((addr("0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359"), 6)),
        ("USDT", 137) => Ok((addr("0xc2132D05D31c914a87C6611C10748AEb04B58e8F"), 6)),
        ("WMATIC", 137) => Ok((addr("0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270"), 18)),
        ("WSTETH", 137) => Ok((addr("0x03b54A6e9a984069379fae1a4fC4dBAE93B3bCCD"), 18)),
        _ => bail!(
            "Unknown token '{}' on chain {}. Use a contract address instead.",
            symbol,
            chain_id
        ),
    }
}

fn addr(s: &str) -> Address {
    Address::from_str(s).expect("invalid hardcoded address")
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct UniswapClient {
    config: &'static UniswapChainConfig,
    signer: PrivateKeySigner,
}

impl UniswapClient {
    pub fn new(chain: &str) -> Result<Self> {
        let config = get_chain_config(chain)?;
        let pk = std::env::var("EVM_PRIVATE_KEY")
            .context("EVM_PRIVATE_KEY env var required for Uniswap swaps")?;
        let pk = pk.strip_prefix("0x").unwrap_or(&pk);
        let signer: PrivateKeySigner = pk.parse().context("invalid EVM_PRIVATE_KEY")?;
        Ok(Self { config, signer })
    }

    /// Get a swap quote (estimated output amount) without executing.
    pub async fn quote(
        &self,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        fee: u32,
    ) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let quoter = IQuoterV2::new(Address::from_str(self.config.quoter_v2)?, &provider);

        let params = IQuoterV2::QuoteExactInputSingleParams {
            tokenIn: token_in,
            tokenOut: token_out,
            amountIn: amount_in,
            fee: fee.try_into().context("invalid fee")?,
            sqrtPriceLimitX96: U160::ZERO,
        };

        let result = quoter
            .quoteExactInputSingle(params)
            .call()
            .await
            .context("quote failed — pool may not exist for this pair/fee tier")?;

        Ok(json!({
            "amount_out": result.amountOut.to_string(),
            "gas_estimate": result.gasEstimate.to_string(),
        }))
    }

    /// Execute exact-input-single swap on Uniswap V3.
    #[allow(clippy::too_many_arguments)]
    pub async fn swap(
        &self,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        fee: u32,
        slippage_bps: u32,
        decimals_in: u8,
        decimals_out: u8,
    ) -> Result<serde_json::Value> {
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse()?);

        let router_addr = Address::from_str(self.config.swap_router)?;
        let user = self.signer.address();

        // 1. Get quote for minimum output calculation
        let quoter = IQuoterV2::new(Address::from_str(self.config.quoter_v2)?, &provider);
        let quote_params = IQuoterV2::QuoteExactInputSingleParams {
            tokenIn: token_in,
            tokenOut: token_out,
            amountIn: amount_in,
            fee: fee.try_into().context("invalid fee")?,
            sqrtPriceLimitX96: U160::ZERO,
        };
        let quote = quoter
            .quoteExactInputSingle(quote_params)
            .call()
            .await
            .context("quote failed — pool may not exist for this pair/fee tier")?;

        // Apply slippage tolerance
        let min_out = quote.amountOut * U256::from(10000 - slippage_bps) / U256::from(10000);

        // 2. Approve router to spend tokenIn
        let erc20 = IERC20::new(token_in, &provider);
        let current_allowance = erc20.allowance(user, router_addr).call().await?;
        if current_allowance < amount_in {
            let approve_receipt = erc20
                .approve(router_addr, amount_in)
                .send()
                .await?
                .get_receipt()
                .await?;
            if !approve_receipt.status() {
                bail!("ERC20 approve transaction failed");
            }
        }

        // 3. Execute swap via multicall with deadline
        let router = ISwapRouter02::new(router_addr, &provider);
        let deadline = U256::from(chrono::Utc::now().timestamp() as u64 + 300); // 5 min
        let swap_params = ISwapRouter02::ExactInputSingleParams {
            tokenIn: token_in,
            tokenOut: token_out,
            fee: fee.try_into().context("invalid fee")?,
            recipient: user,
            amountIn: amount_in,
            amountOutMinimum: min_out,
            sqrtPriceLimitX96: U160::ZERO,
        };

        // Encode the exactInputSingle call, then wrap in multicall with deadline
        let swap_calldata = ISwapRouter02::exactInputSingleCall {
            params: swap_params,
        }
        .abi_encode();

        let receipt = router
            .multicall(deadline, vec![swap_calldata.into()])
            .send()
            .await
            .context("swap transaction failed")?
            .get_receipt()
            .await
            .context("failed to get swap receipt")?;

        Ok(json!({
            "action": "swap",
            "chain_id": self.config.chain_id,
            "token_in": format!("{}", token_in),
            "token_out": format!("{}", token_out),
            "amount_in": format_units(amount_in, decimals_in),
            "expected_out": format_units(quote.amountOut, decimals_out),
            "minimum_out": format_units(min_out, decimals_out),
            "slippage_bps": slippage_bps,
            "fee_tier": fee,
            "tx_hash": format!("{}", receipt.transaction_hash),
            "status": if receipt.status() { "success" } else { "failed" },
            "block_number": receipt.block_number.unwrap_or_default(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
