//! ScannerClient — OKX Trenches API + DEX swap execution for memepump scanner on Solana.
//!
//! Uses ApiClient for all HTTP calls. Wallet address comes from SOL_ADDRESS env var.
//! No local Solana signing — OKX handles it server-side via /aggregator/swap with userWalletAddress.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::engine::{safe_float, CHAIN_INDEX, SOL_DECIMALS, SOL_NATIVE};
use crate::client::ApiClient;

pub struct ScannerClient {
    api: ApiClient,
    pub wallet: String,
}

pub struct SwapResult {
    pub tx_hash: Option<String>,
    pub amount_out: f64,
}

impl ScannerClient {
    /// Create a fully authenticated client.
    /// Requires SOL_ADDRESS + OKX API keys (via ApiClient).
    pub fn new() -> Result<Self> {
        let api = ApiClient::new(None)?;
        let wallet = std::env::var("SOL_ADDRESS")
            .context("SOL_ADDRESS not set — required for memepump scanner")?;
        Ok(Self { api, wallet })
    }

    /// Read-only client (no wallet needed for data queries).
    pub fn new_read_only() -> Result<Self> {
        let api = ApiClient::new(None)?;
        let wallet = std::env::var("SOL_ADDRESS").unwrap_or_default();
        Ok(Self { api, wallet })
    }

    // ── Trenches API ────────────────────────────────────────────────

    /// Fetch memepump token list with server-side filters.
    /// `params` is a JSON object whose keys are used as GET query params.
    pub async fn get_memepump_list(&self, params: &Value) -> Result<Vec<Value>> {
        // Build query pairs from JSON object
        let query: Vec<(&str, String)> = params
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        let s = v.as_str().map(|s| s.to_string()).or_else(|| {
                            if v.is_number() {
                                Some(v.to_string())
                            } else {
                                None
                            }
                        })?;
                        if s.is_empty() {
                            None
                        } else {
                            Some((k.as_str(), s))
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let data = self
            .api
            .get("/api/v6/dex/market/memepump/tokenList", &query_refs)
            .await?;

        match data {
            Value::Array(arr) => Ok(arr),
            _ => Ok(data.as_array().cloned().unwrap_or_default()),
        }
    }

    /// Fetch dev info for a token (rug history, total launched, holdings).
    pub async fn get_dev_info(&self, token_addr: &str) -> Result<Value> {
        let data = self
            .api
            .get(
                "/api/v6/dex/market/memepump/tokenDevInfo",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                ],
            )
            .await?;

        match data {
            Value::Array(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            Value::Object(_) => Ok(data),
            _ => Ok(json!({})),
        }
    }

    /// Fetch bundle info for a token (bundler ATH %, bundler count).
    pub async fn get_bundle_info(&self, token_addr: &str) -> Result<Value> {
        let data = self
            .api
            .get(
                "/api/v6/dex/market/memepump/tokenBundleInfo",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                ],
            )
            .await?;

        match data {
            Value::Array(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            Value::Object(_) => Ok(data),
            _ => Ok(json!({})),
        }
    }

    /// Fetch 1-minute candles for volume analysis.
    pub async fn get_candles(&self, token_addr: &str, limit: u32) -> Result<Value> {
        self.api
            .get(
                "/api/v6/dex/market/candles",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                    ("bar", "1m"),
                    ("limit", &limit.to_string()),
                ],
            )
            .await
    }

    /// Fetch price info (MC, holders, price, top10, etc.).
    pub async fn get_price_info(&self, token_addr: &str) -> Result<Value> {
        let data = self
            .api
            .get(
                "/api/v6/dex/market/price-info",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                ],
            )
            .await?;

        match data {
            Value::Array(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            Value::Object(_) => Ok(data),
            _ => bail!("unexpected price-info response"),
        }
    }

    // ── Swap execution ──────────────────────────────────────────────

    /// Execute a swap via OKX DEX aggregator.
    /// `from_token` and `to_token` are Solana token addresses.
    /// `amount_raw` is in minimal units (lamports for SOL, raw for SPL).
    pub async fn execute_swap(
        &self,
        from_token: &str,
        to_token: &str,
        amount_raw: &str,
        slippage_pct: &str,
    ) -> Result<SwapResult> {
        if self.wallet.is_empty() {
            bail!("SOL_ADDRESS not set — cannot execute swap");
        }

        let data = self
            .api
            .get(
                "/api/v6/dex/aggregator/swap",
                &[
                    ("chainIndex", CHAIN_INDEX),
                    ("fromTokenAddress", from_token),
                    ("toTokenAddress", to_token),
                    ("amount", amount_raw),
                    ("slippagePercent", slippage_pct),
                    ("userWalletAddress", &self.wallet),
                ],
            )
            .await?;

        let swap_data = match &data {
            Value::Array(arr) if !arr.is_empty() => arr[0].clone(),
            _ => data,
        };

        let tx_hash = swap_data["txHash"]
            .as_str()
            .or_else(|| swap_data["tx"]["txHash"].as_str())
            .unwrap_or("")
            .to_string();

        let amount_out = safe_float(&swap_data["toTokenAmount"], 0.0);

        Ok(SwapResult {
            tx_hash: if tx_hash.is_empty() {
                None
            } else {
                Some(tx_hash)
            },
            amount_out,
        })
    }

    /// Buy a token with SOL.
    pub async fn buy_token(
        &self,
        token_addr: &str,
        sol_amount: f64,
        slippage_pct: u32,
    ) -> Result<SwapResult> {
        let amount_raw = format!("{}", (sol_amount * 10f64.powi(SOL_DECIMALS as i32)) as u64);
        let slippage_str = slippage_pct.to_string();
        self.execute_swap(SOL_NATIVE, token_addr, &amount_raw, &slippage_str)
            .await
    }

    /// Sell a token for SOL. `amount_raw` is the raw token amount.
    pub async fn sell_token(
        &self,
        token_addr: &str,
        amount_raw: &str,
        slippage_pct: u32,
    ) -> Result<SwapResult> {
        let slippage_str = slippage_pct.to_string();
        self.execute_swap(token_addr, SOL_NATIVE, amount_raw, &slippage_str)
            .await
    }
}
