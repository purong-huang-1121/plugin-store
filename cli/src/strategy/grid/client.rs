//! GridClient — OKX DEX quote/swap/approve + alloy signing + RPC broadcast.

use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::str::FromStr;

use super::config::GridConfig;
use super::engine::{BASE_RPC, CHAIN_ID, ETH_ADDR, USDC_ADDR, USDC_DECIMALS};
use crate::client::ApiClient;

pub struct GridClient {
    api: ApiClient,
    signer: PrivateKeySigner,
    address: Address,
    rpc_url: String,
    slippage_pct: String,
}

pub struct SwapResult {
    pub tx_hash: Option<String>,
    pub amount_in: f64,
    pub amount_out: f64,
    pub price_impact: Option<f64>,
    pub failure: Option<FailureInfo>,
}

pub struct FailureInfo {
    pub reason: String,
    pub detail: String,
    pub retriable: bool,
    #[allow(dead_code)]
    pub hint: String,
}

impl GridClient {
    /// Create a fully authenticated client.
    pub fn new() -> Result<Self> {
        let api = ApiClient::new(None)?;

        let pk = std::env::var("EVM_PRIVATE_KEY")
            .context("EVM_PRIVATE_KEY not set — required for grid trading")?;
        let pk = pk.strip_prefix("0x").unwrap_or(&pk);
        let signer: PrivateKeySigner = pk.parse().context("invalid EVM_PRIVATE_KEY")?;
        let address = signer.address();

        let rpc_url = std::env::var("BASE_RPC_URL").unwrap_or_else(|_| BASE_RPC.to_string());

        let cfg = GridConfig::load().unwrap_or_default();
        let slippage_pct = cfg.slippage_pct;

        Ok(Self {
            api,
            signer,
            address,
            rpc_url,
            slippage_pct,
        })
    }

    /// Build a read-only provider (no wallet).
    fn read_provider(&self) -> Result<impl Provider> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        Ok(provider)
    }

    /// Build a wallet-enabled provider for signing transactions.
    fn wallet_provider(&self) -> Result<impl Provider> {
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.parse()?);
        Ok(provider)
    }

    #[allow(dead_code)]
    pub fn address(&self) -> Address {
        self.address
    }

    /// Get ETH/USDC price via OKX DEX aggregator quote API.
    pub async fn get_eth_price(&self) -> Result<f64> {
        let data = self
            .api
            .get(
                "/api/v6/dex/aggregator/quote",
                &[
                    ("chainIndex", CHAIN_ID),
                    ("fromTokenAddress", ETH_ADDR),
                    ("toTokenAddress", USDC_ADDR),
                    ("amount", "1000000000000000000"), // 1 ETH in wei
                ],
            )
            .await?;

        let quote = if data.is_array() {
            data.as_array()
                .and_then(|a| a.first())
                .context("empty quote response")?
        } else {
            &data
        };

        let to_amount_str = quote["toTokenAmount"]
            .as_str()
            .context("missing toTokenAmount in quote")?;
        let to_amount: f64 = to_amount_str.parse().context("invalid toTokenAmount")?;
        let price = to_amount / 1_000_000.0;
        Ok(price)
    }

    /// Get on-chain balances via alloy RPC.
    pub async fn get_balances(&self) -> Result<(f64, f64)> {
        let provider = self.read_provider()?;

        // ETH balance
        let eth_wei = provider.get_balance(self.address).await?;
        let eth_bal = wei_to_f64(eth_wei, 18);

        // USDC balance via balanceOf call
        let usdc_addr = Address::from_str(USDC_ADDR)?;
        let mut calldata = vec![0x70, 0xa0, 0x82, 0x31]; // balanceOf(address)
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(self.address.as_slice());

        let tx = TransactionRequest::default()
            .to(usdc_addr)
            .input(Bytes::from(calldata).into());

        let result = provider.call(tx).await?;
        let usdc_raw = U256::from_be_slice(&result);
        let usdc_bal = wei_to_f64(usdc_raw, USDC_DECIMALS);

        Ok((eth_bal, usdc_bal))
    }

    /// Execute full swap flow via OKX DEX aggregator.
    pub async fn execute_swap(
        &self,
        direction: &str,
        amount: U256,
        _price: f64,
    ) -> Result<SwapResult> {
        let (from_token, to_token) = match direction {
            "BUY" => (USDC_ADDR, ETH_ADDR),
            "SELL" => (ETH_ADDR, USDC_ADDR),
            _ => bail!("invalid direction: {}", direction),
        };

        let amount_str = amount.to_string();
        let wallet_str = format!("{:#x}", self.address);

        // For BUY (USDC->ETH), ensure USDC approval first
        if direction == "BUY" {
            self.ensure_usdc_approval(&amount_str).await?;
        }

        // Get swap tx from OKX API
        let data = self
            .api
            .get(
                "/api/v6/dex/aggregator/swap",
                &[
                    ("chainIndex", CHAIN_ID),
                    ("fromTokenAddress", from_token),
                    ("toTokenAddress", to_token),
                    ("amount", &amount_str),
                    ("slippagePercent", &self.slippage_pct),
                    ("userWalletAddress", &wallet_str),
                ],
            )
            .await?;

        let swap_data = if data.is_array() {
            data.as_array()
                .and_then(|a| a.first())
                .context("empty swap response")?
                .clone()
        } else {
            data
        };

        // Sign and broadcast the swap tx
        let tx_obj = &swap_data["tx"];
        eprintln!(
            "[grid-debug] swap_data keys: {:?}",
            swap_data.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
        eprintln!(
            "[grid-debug] tx_obj: {}",
            serde_json::to_string_pretty(tx_obj).unwrap_or_default()
        );
        let (tx_hash, success) = self.sign_and_broadcast(tx_obj).await?;

        if !success {
            return Ok(SwapResult {
                tx_hash: Some(tx_hash.clone()),
                amount_in: 0.0,
                amount_out: 0.0,
                price_impact: None,
                failure: Some(FailureInfo {
                    reason: "Transaction reverted".to_string(),
                    detail: format!("tx {} reverted on-chain", tx_hash),
                    retriable: true,
                    hint: "Check gas and slippage settings".to_string(),
                }),
            });
        }

        let amount_in = parse_swap_amount(&swap_data, "fromTokenAmount", direction, true);
        let amount_out = parse_swap_amount(&swap_data, "toTokenAmount", direction, false);

        Ok(SwapResult {
            tx_hash: Some(tx_hash),
            amount_in,
            amount_out,
            price_impact: swap_data["priceImpactPercentage"]
                .as_str()
                .and_then(|s| s.parse().ok()),
            failure: None,
        })
    }

    /// Get the OKX DEX router (spender) address from approve-transaction API.
    async fn get_dex_router(&self) -> Result<Address> {
        let data = self
            .api
            .get(
                "/api/v6/dex/aggregator/approve-transaction",
                &[
                    ("chainIndex", CHAIN_ID),
                    ("tokenContractAddress", USDC_ADDR),
                    ("approveAmount", "1000000"), // 1 USDC, just to get router address
                ],
            )
            .await?;

        let approve_data = if data.is_array() {
            data.as_array()
                .and_then(|a| a.first())
                .cloned()
                .context("empty approve response")?
        } else {
            data
        };

        let spender_str = approve_data["dexContractAddress"]
            .as_str()
            .context("missing dexContractAddress in approve response")?;

        Address::from_str(spender_str).context("invalid spender address")
    }

    /// Ensure USDC approval for the DEX router using alloy directly.
    /// Checks current allowance first; only approves if needed.
    async fn ensure_usdc_approval(&self, amount: &str) -> Result<()> {
        let provider = self.wallet_provider()?;
        let usdc_addr = Address::from_str(USDC_ADDR)?;
        let spender = self.get_dex_router().await?;
        let needed = U256::from_str(amount).unwrap_or(U256::ZERO);

        // Check current allowance: allowance(owner, spender)
        let mut calldata = vec![0xdd, 0x62, 0xed, 0x3e]; // allowance(address,address)
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(self.address.as_slice());
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(spender.as_slice());

        let read_provider = self.read_provider()?;
        let call_tx = TransactionRequest::default()
            .to(usdc_addr)
            .input(Bytes::from(calldata).into());
        let result = read_provider.call(call_tx).await?;
        let current_allowance = U256::from_be_slice(&result);

        if current_allowance >= needed {
            eprintln!(
                "[grid] USDC allowance sufficient ({} >= {})",
                current_allowance, needed
            );
            return Ok(());
        }

        eprintln!("[grid] Approving USDC for DEX router {:#x}...", spender);

        // Build approve(spender, type(uint256).max) calldata
        let mut approve_data = vec![0x09, 0x5e, 0xa7, 0xb3]; // approve(address,uint256)
        approve_data.extend_from_slice(&[0u8; 12]);
        approve_data.extend_from_slice(spender.as_slice());
        approve_data.extend_from_slice(&U256::MAX.to_be_bytes::<32>());

        let tx_request = TransactionRequest::default()
            .to(usdc_addr)
            .value(U256::ZERO)
            .input(Bytes::from(approve_data).into())
            .gas_limit(60_000)
            .with_chain_id(8453);

        let pending = provider
            .send_transaction(tx_request)
            .await
            .context("failed to send USDC approve tx")?;

        let tx_hash = format!("{:#x}", pending.tx_hash());
        let receipt = pending
            .get_receipt()
            .await
            .context("failed to get approve receipt")?;

        if !receipt.status() {
            bail!("USDC approve transaction {} reverted", tx_hash);
        }

        eprintln!("[grid] USDC approved (tx: {})", tx_hash);
        Ok(())
    }

    /// Sign and broadcast a raw tx object from OKX API.
    /// tx_obj has fields: to, data, value, gas/gasLimit
    /// Returns (tx_hash, success).
    async fn sign_and_broadcast(&self, tx_obj: &Value) -> Result<(String, bool)> {
        let provider = self.wallet_provider()?;

        let to_addr = tx_obj["to"]
            .as_str()
            .or_else(|| tx_obj["dexContractAddress"].as_str())
            .context("missing 'to' in tx object")?;
        let tx_data = tx_obj["data"]
            .as_str()
            .context("missing 'data' in tx object")?;
        let tx_value = tx_obj["value"].as_str().unwrap_or("0");
        let gas_limit = tx_obj["gas"]
            .as_str()
            .or_else(|| tx_obj["gasLimit"].as_str())
            .unwrap_or("300000");

        let to = Address::from_str(to_addr)?;
        let value = U256::from_str(tx_value).unwrap_or(U256::ZERO);
        let data_bytes = hex::decode(tx_data.strip_prefix("0x").unwrap_or(tx_data))?;
        // Add 50% gas buffer to avoid reverts on complex DEX routes
        let gas = (gas_limit.parse::<u64>().unwrap_or(300_000) as f64 * 1.5) as u64;

        let tx_request = TransactionRequest::default()
            .to(to)
            .value(value)
            .input(Bytes::from(data_bytes).into())
            .gas_limit(gas)
            .with_chain_id(8453);

        // The wallet provider handles nonce, gas price, signing, and broadcasting
        let pending = provider
            .send_transaction(tx_request)
            .await
            .context("failed to send transaction")?;

        let tx_hash = format!("{:#x}", pending.tx_hash());

        let receipt = pending
            .get_receipt()
            .await
            .context("failed to get receipt")?;

        Ok((tx_hash, receipt.status()))
    }
}

/// Convert U256 wei to f64 given decimals.
fn wei_to_f64(wei: U256, decimals: u8) -> f64 {
    let s = wei.to_string();
    let d = decimals as usize;
    if s.len() <= d {
        let padded = format!("0.{:0>width$}", s, width = d);
        padded.parse().unwrap_or(0.0)
    } else {
        let (whole, frac) = s.split_at(s.len() - d);
        format!("{}.{}", whole, frac).parse().unwrap_or(0.0)
    }
}

/// Parse swap amount from OKX response.
fn parse_swap_amount(swap_data: &Value, field: &str, direction: &str, is_from: bool) -> f64 {
    let raw = swap_data[field]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let decimals = match (direction, is_from) {
        ("BUY", true) => USDC_DECIMALS,
        ("BUY", false) => 18u8,
        ("SELL", true) => 18u8,
        ("SELL", false) => USDC_DECIMALS,
        _ => 18,
    };

    raw / 10f64.powi(decimals as i32)
}
