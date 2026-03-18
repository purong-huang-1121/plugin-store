//! GridClient — OKX DEX quote/swap/approve + signing + RPC broadcast.
//!
//! Supports two signing modes:
//! - **Local**: alloy PrivateKeySigner (legacy, unused)
//! - **OnchainOs**: onchainos wallet CLI (TEE signing)

use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::str::FromStr;

use super::config::GridConfig;
use super::engine::{BASE_RPC, ETH_ADDR, USDC_ADDR, USDC_DECIMALS};

enum SignerMode {
    OnchainOs {
        address: Address,
        chain_flag: String,
    },
}

pub struct GridClient {
    signer_mode: SignerMode,
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
    /// Create a fully authenticated client via onchainos wallet.
    pub fn new() -> Result<Self> {
        let rpc_url = std::env::var("BASE_RPC_URL").unwrap_or_else(|_| BASE_RPC.to_string());
        let cfg = GridConfig::load().unwrap_or_default();
        let slippage_pct = cfg.slippage_pct;

        let addr_str = crate::onchainos::get_evm_address()
            .context("onchainos wallet not available — please login first")?;
        let address = Address::from_str(&addr_str).context("invalid onchainos EVM address")?;
        let chain_flag = crate::onchainos::chain_flag("base").to_string();
        let signer_mode = SignerMode::OnchainOs {
            address,
            chain_flag,
        };

        Ok(Self {
            signer_mode,
            rpc_url,
            slippage_pct,
        })
    }

    fn wallet_address(&self) -> Address {
        match &self.signer_mode {
            SignerMode::OnchainOs { address, .. } => *address,
        }
    }

    #[allow(dead_code)]
    pub fn address(&self) -> Address {
        self.wallet_address()
    }

    /// Get ETH/USDC price via onchainos swap quote CLI.
    pub async fn get_eth_price(&self) -> Result<f64> {
        let data = crate::onchainos::swap_quote(ETH_ADDR, USDC_ADDR, "1000000000000000000", "base", None)?;

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

    /// Get on-chain balances via onchainos.
    pub async fn get_balances(&self) -> Result<(f64, f64)> {
        let balances = crate::onchainos::get_token_balances("base")?;
        let eth_bal = balances
            .iter()
            .find(|b| b.symbol.eq_ignore_ascii_case("ETH"))
            .map(|b| b.balance)
            .unwrap_or(0.0);
        let usdc_bal = balances
            .iter()
            .find(|b| b.symbol.eq_ignore_ascii_case("USDC"))
            .map(|b| b.balance)
            .unwrap_or(0.0);
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
        let wallet_str = format!("{:#x}", self.wallet_address());

        // For BUY (USDC->ETH), ensure USDC approval first
        if direction == "BUY" {
            self.ensure_usdc_approval(&amount_str).await?;
        }

        // Get swap tx from onchainos swap CLI
        let data = crate::onchainos::swap_swap(from_token, to_token, &amount_str, "base", &wallet_str, Some(&self.slippage_pct))?;

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

    /// Get the DEX router (spender) address from onchainos swap approve CLI.
    async fn get_dex_router(&self) -> Result<Address> {
        let data = crate::onchainos::swap_approve(USDC_ADDR, "1000000", "base")?;

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

    /// Ensure USDC approval for the DEX router.
    /// Checks current allowance first; only approves if needed.
    async fn ensure_usdc_approval(&self, amount: &str) -> Result<()> {
        let usdc_addr = Address::from_str(USDC_ADDR)?;
        let spender = self.get_dex_router().await?;
        let needed = U256::from_str(amount).unwrap_or(U256::ZERO);
        let addr = self.wallet_address();

        // Check current allowance (read-only)
        let mut calldata = vec![0xdd, 0x62, 0xed, 0x3e]; // allowance(address,address)
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(addr.as_slice());
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(spender.as_slice());

        let read_provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
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

        match &self.signer_mode {
            SignerMode::OnchainOs { chain_flag, .. } => {
                let tx_hash = crate::onchainos::contract_call(
                    chain_flag,
                    &format!("{}", usdc_addr),
                    &format!("0x{}", hex::encode(&approve_data)),
                    "0",
                )
                .await?;
                eprintln!("[grid] USDC approved via onchainos (tx: {})", tx_hash);
            }
        }

        Ok(())
    }

    /// Sign and broadcast a raw tx object from OKX API.
    /// tx_obj has fields: to, data, value, gas/gasLimit
    /// Returns (tx_hash, success).
    async fn sign_and_broadcast(&self, tx_obj: &Value) -> Result<(String, bool)> {
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

        match &self.signer_mode {
            SignerMode::OnchainOs { chain_flag, .. } => {
                // Convert value from wei to UI units for onchainos --value
                let value_ui = if tx_value == "0" || tx_value.is_empty() {
                    "0".to_string()
                } else {
                    let wei = U256::from_str(tx_value).unwrap_or(U256::ZERO);
                    wei_to_f64(wei, 18).to_string()
                };

                let input_data = if tx_data.starts_with("0x") {
                    tx_data.to_string()
                } else {
                    format!("0x{}", tx_data)
                };

                let tx_hash = crate::onchainos::contract_call(
                    chain_flag,
                    to_addr,
                    &input_data,
                    &value_ui,
                )
                .await?;

                Ok((tx_hash, true))
            }
        }
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
