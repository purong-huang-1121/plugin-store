//! SignalClient — OKX API calls for signal data + swap execution on Solana.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::engine::{safe_float, CHAIN_INDEX, SLIPPAGE_PCT, SOL_DECIMALS, SOL_NATIVE};
use crate::client::ApiClient;

pub struct SignalClient {
    api: ApiClient,
    pub wallet: String,
}

pub struct SwapResult {
    pub tx_hash: Option<String>,
    pub amount_out: f64,
}

impl SignalClient {
    /// Create a fully authenticated client.
    pub fn new() -> Result<Self> {
        let api = ApiClient::new(None)?;
        let wallet = std::env::var("SOL_ADDRESS")
            .context("SOL_ADDRESS not set — required for signal tracker")?;
        Ok(Self { api, wallet })
    }

    /// Read-only client (no wallet needed for data queries).
    pub fn new_read_only() -> Result<Self> {
        let api = ApiClient::new(None)?;
        let wallet = std::env::var("SOL_ADDRESS").unwrap_or_default();
        Ok(Self { api, wallet })
    }

    // ── Signal API ────────────────────────────────────────────────

    /// Fetch smart money signals from OKX Signal API.
    pub async fn fetch_signals(&self) -> Result<Vec<Value>> {
        let body = json!({
            "chainIndex": CHAIN_INDEX,
            "walletType": super::engine::SIGNAL_LABELS,
            "minAddressCount": super::engine::MIN_WALLET_COUNT.to_string(),
            "minMarketCapUsd": super::engine::MIN_MCAP.to_string(),
            "minLiquidityUsd": super::engine::MIN_LIQUIDITY.to_string(),
        });

        let data = self
            .api
            .post("/api/v6/dex/market/signal/list", &body)
            .await?;

        match data {
            Value::Array(arr) => Ok(arr),
            _ => Ok(data.as_array().cloned().unwrap_or_default()),
        }
    }

    // ── Market Data ───────────────────────────────────────────────

    /// Fetch price info (MC, Liq, Holders, Price, Top10).
    pub async fn fetch_price_info(&self, token_addr: &str) -> Result<Value> {
        let body = json!([{
            "tokenContractAddress": token_addr,
            "chainIndex": CHAIN_INDEX,
        }]);
        let data = self
            .api
            .post("/api/v6/dex/market/price-info", &body)
            .await?;

        match data {
            Value::Array(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            Value::Object(_) => Ok(data),
            _ => bail!("unexpected price-info response"),
        }
    }

    /// Fetch 1-minute candles for pump check.
    pub async fn fetch_candles_1m(&self, token_addr: &str) -> Result<Value> {
        self.api
            .get(
                "/api/v6/dex/market/candles",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                    ("bar", "1m"),
                    ("limit", "5"),
                ],
            )
            .await
    }

    /// Fetch dev info from Trenches API.
    pub async fn fetch_dev_info(&self, token_addr: &str) -> Result<Value> {
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

    /// Fetch bundle info from Trenches API.
    pub async fn fetch_bundle_info(&self, token_addr: &str) -> Result<Value> {
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

    /// Fetch current token price in USD.
    pub async fn fetch_price(&self, token_addr: &str) -> Result<f64> {
        let info = self.fetch_price_info(token_addr).await?;
        let price = safe_float(&info["price"], 0.0);
        if price <= 0.0 {
            bail!("invalid price for {token_addr}");
        }
        Ok(price)
    }

    /// Fetch SOL balance for the wallet.
    pub async fn fetch_sol_balance(&self) -> Result<f64> {
        if self.wallet.is_empty() {
            bail!("SOL_ADDRESS not set");
        }
        let data = self
            .api
            .get(
                "/api/v6/dex/balance/all-token-balances-by-address",
                &[("address", &*self.wallet), ("chains", CHAIN_INDEX)],
            )
            .await?;

        // Response: [{"tokenAssets": [{"symbol":"SOL","balance":"1.09",...}, ...]}]
        let assets = if let Some(arr) = data.as_array() {
            arr.first()
                .and_then(|item| item["tokenAssets"].as_array())
                .cloned()
                .unwrap_or_default()
        } else {
            data["tokenAssets"].as_array().cloned().unwrap_or_default()
        };

        for b in &assets {
            let sym = b["symbol"].as_str().unwrap_or("");
            let contract = b["tokenContractAddress"].as_str().unwrap_or("");
            if sym == "SOL" || contract == SOL_NATIVE {
                return Ok(safe_float(&b["balance"], 0.0));
            }
        }
        Ok(0.0)
    }

    /// Fetch quote (for honeypot detection).
    pub async fn fetch_quote(&self, token_addr: &str, amount_sol: f64) -> Result<Value> {
        let amount_raw = format!("{}", (amount_sol * 10f64.powi(SOL_DECIMALS as i32)) as u64);
        let data = self
            .api
            .get(
                "/api/v6/dex/aggregator/quote",
                &[
                    ("chainIndex", CHAIN_INDEX),
                    ("fromTokenAddress", SOL_NATIVE),
                    ("toTokenAddress", token_addr),
                    ("amount", &amount_raw),
                    ("slippagePercent", SLIPPAGE_PCT),
                ],
            )
            .await?;

        match data {
            Value::Array(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            _ => Ok(data),
        }
    }

    // ── Swap Execution (sign + broadcast) ──────────────────────────

    /// Execute a swap via OKX DEX aggregator on Solana.
    /// Flow: get swap tx → sign with SOL_PRIVATE_KEY → broadcast to Solana RPC.
    pub async fn execute_swap(
        &self,
        from_token: &str,
        to_token: &str,
        amount_raw: &str,
    ) -> Result<SwapResult> {
        if self.wallet.is_empty() {
            bail!("SOL_ADDRESS not set — cannot execute swap");
        }

        // Step 1: Get swap transaction from OKX API
        let data = self
            .api
            .get(
                "/api/v6/dex/aggregator/swap",
                &[
                    ("chainIndex", CHAIN_INDEX),
                    ("fromTokenAddress", from_token),
                    ("toTokenAddress", to_token),
                    ("amount", amount_raw),
                    ("slippagePercent", SLIPPAGE_PCT),
                    ("userWalletAddress", &self.wallet),
                ],
            )
            .await?;

        let swap_data = match &data {
            Value::Array(arr) if !arr.is_empty() => arr[0].clone(),
            _ => data,
        };

        let amount_out = safe_float(&swap_data["routerResult"]["toTokenAmount"], 0.0);

        // Step 2: Extract the unsigned transaction (base58-encoded)
        let tx_data_b58 = swap_data["tx"]["data"]
            .as_str()
            .context("missing tx.data in swap response")?;

        // Step 3: Sign and broadcast
        let tx_hash = self.sign_and_broadcast(tx_data_b58).await?;

        Ok(SwapResult {
            tx_hash: Some(tx_hash),
            amount_out,
        })
    }

    /// Sign a base58-encoded Solana transaction and broadcast it.
    async fn sign_and_broadcast(&self, tx_data_b58: &str) -> Result<String> {
        let pk_b58 = std::env::var("SOL_PRIVATE_KEY")
            .context("SOL_PRIVATE_KEY not set — required for swap execution")?;

        // Decode private key (64-byte keypair or 32-byte secret)
        let pk_bytes = bs58::decode(&pk_b58)
            .into_vec()
            .context("invalid SOL_PRIVATE_KEY (not valid base58)")?;

        let signing_key = if pk_bytes.len() == 64 {
            ed25519_dalek::SigningKey::from_keypair_bytes(
                pk_bytes
                    .as_slice()
                    .try_into()
                    .context("invalid keypair length")?,
            )
            .context("invalid ed25519 keypair")?
        } else if pk_bytes.len() == 32 {
            ed25519_dalek::SigningKey::from_bytes(
                pk_bytes
                    .as_slice()
                    .try_into()
                    .context("invalid key length")?,
            )
        } else {
            bail!(
                "SOL_PRIVATE_KEY must be 32 or 64 bytes (got {})",
                pk_bytes.len()
            );
        };

        // Decode the transaction
        let tx_bytes = bs58::decode(tx_data_b58)
            .into_vec()
            .context("invalid base58 transaction data")?;

        // Sign the transaction as-is (OKX's blockhash should be recent enough)
        let signed_tx = sign_solana_transaction(&tx_bytes, &signing_key)?;
        let signed_b58 = bs58::encode(&signed_tx).into_string();

        // Broadcast via OKX broadcast-transaction endpoint
        let body = json!({
            "chainIndex": CHAIN_INDEX,
            "signedTx": signed_b58,
            "address": self.wallet,
        });
        let data = self
            .api
            .post("/api/v6/dex/pre-transaction/broadcast-transaction", &body)
            .await?;

        let result = match &data {
            Value::Array(arr) if !arr.is_empty() => arr[0].clone(),
            _ => data,
        };

        // If we got a direct txHash, return it
        if let Some(hash) = result["txHash"]
            .as_str()
            .or_else(|| result["orderHash"].as_str())
            .or_else(|| result["hash"].as_str())
        {
            if !hash.is_empty() {
                return Ok(hash.to_string());
            }
        }

        // Otherwise we got an orderId — poll for the txHash
        let order_id = result["orderId"]
            .as_str()
            .context("broadcast returned neither txHash nor orderId")?
            .to_string();

        // Poll OKX order status up to 30s
        for attempt in 0..6 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            if let Ok(data) = self
                .api
                .get(
                    "/api/v6/dex/post-transaction/orders",
                    &[
                        ("address", &*self.wallet),
                        ("chainIndex", CHAIN_INDEX),
                        ("orderId", &order_id),
                    ],
                )
                .await
            {
                let orders = match &data {
                    Value::Array(arr) => arr.clone(),
                    _ => data["orders"].as_array().cloned().unwrap_or_default(),
                };
                for order in &orders {
                    let tx_status = order["txStatus"].as_str().unwrap_or("");
                    let tx_hash = order["txHash"].as_str().unwrap_or("");
                    if tx_status == "2" && !tx_hash.is_empty() {
                        return Ok(tx_hash.to_string());
                    }
                    if tx_status == "3" {
                        let reason = order["failReason"].as_str().unwrap_or("unknown");
                        bail!("transaction failed on-chain: {}", reason);
                    }
                }
            }
        }

        Ok(order_id)
    }

    /// Buy a token with SOL.
    pub async fn buy_token(&self, token_addr: &str, sol_amount: f64) -> Result<SwapResult> {
        let amount_raw = format!("{}", (sol_amount * 10f64.powi(SOL_DECIMALS as i32)) as u64);
        self.execute_swap(SOL_NATIVE, token_addr, &amount_raw).await
    }

    /// Sell a token for SOL.
    pub async fn sell_token(&self, token_addr: &str, amount_raw: &str) -> Result<SwapResult> {
        self.execute_swap(token_addr, SOL_NATIVE, amount_raw).await
    }
}

const SOLANA_RPC: &str = "https://api.mainnet-beta.solana.com";

/// Sign a Solana serialized transaction.
fn sign_solana_transaction(
    tx_bytes: &[u8],
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<Vec<u8>> {
    use ed25519_dalek::Signer;

    if tx_bytes.is_empty() {
        bail!("empty transaction data");
    }

    let (num_sigs, offset) = decode_compact_u16(tx_bytes)?;
    if num_sigs == 0 {
        bail!("transaction has 0 signature slots");
    }

    let sigs_end = offset + (num_sigs as usize) * 64;
    if sigs_end > tx_bytes.len() {
        bail!("transaction too short for {} signatures", num_sigs);
    }

    let message = &tx_bytes[sigs_end..];
    let signature = signing_key.sign(message);

    let mut signed = Vec::with_capacity(tx_bytes.len());
    signed.extend_from_slice(&tx_bytes[..offset]); // compact-u16 header
    signed.extend_from_slice(&signature.to_bytes()); // our signature (64 bytes)
    if num_sigs > 1 {
        signed.extend_from_slice(&tx_bytes[offset + 64..sigs_end]);
    }
    signed.extend_from_slice(message);

    Ok(signed)
}

/// Replace the blockhash in a Solana transaction's message.
/// Supports both legacy and versioned (v0) transactions.
fn replace_blockhash(tx_bytes: &mut [u8], new_blockhash: &[u8]) -> Result<()> {
    if new_blockhash.len() != 32 {
        bail!("blockhash must be 32 bytes");
    }

    let (num_sigs, sig_header_len) = decode_compact_u16(tx_bytes)?;
    let msg_start = sig_header_len + (num_sigs as usize) * 64;

    if msg_start >= tx_bytes.len() {
        bail!("transaction too short for message");
    }

    let msg = &tx_bytes[msg_start..];

    // Detect versioned (v0) vs legacy: high bit set = versioned
    let version_offset = if msg[0] & 0x80 != 0 { 1 } else { 0 };

    if msg.len() < version_offset + 3 {
        bail!("transaction too short for message header");
    }

    let header_end = version_offset + 3;
    let (num_accounts, accounts_header_len) = decode_compact_u16(&msg[header_end..])?;
    let accounts_start = header_end + accounts_header_len;
    let accounts_end = accounts_start + (num_accounts as usize) * 32;

    let bh_offset = msg_start + accounts_end;
    if bh_offset + 32 > tx_bytes.len() {
        bail!("transaction too short for blockhash at offset {bh_offset}");
    }

    tx_bytes[bh_offset..bh_offset + 32].copy_from_slice(new_blockhash);
    Ok(())
}

/// Decode a Solana compact-u16 from a byte slice.
fn decode_compact_u16(data: &[u8]) -> Result<(u16, usize)> {
    if data.is_empty() {
        bail!("empty data for compact-u16");
    }
    let first = data[0] as u16;
    if first < 0x80 {
        return Ok((first, 1));
    }
    if data.len() < 2 {
        bail!("truncated compact-u16");
    }
    let second = data[1] as u16;
    if second < 0x80 {
        return Ok(((first & 0x7f) | (second << 7), 2));
    }
    if data.len() < 3 {
        bail!("truncated compact-u16");
    }
    let third = data[2] as u16;
    Ok(((first & 0x7f) | ((second & 0x7f) << 7) | (third << 14), 3))
}
