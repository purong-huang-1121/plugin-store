//! SniperClient — OKX DEX API calls for token data + swap execution on Solana.
//!
//! Unlike GridClient (which uses alloy for on-chain EVM RPC), this client uses
//! only the OKX HTTP API via ApiClient for all operations. Wallet address and
//! private key come from SOL_ADDRESS and SOL_PRIVATE_KEY env vars.

use anyhow::{bail, Context, Result};
use serde_json::Value;

use super::engine::{safe_float, CHAIN_INDEX, SLIPPAGE_PCT, SOL_DECIMALS, SOL_NATIVE};
use crate::client::ApiClient;

const SOLANA_RPC: &str = "https://api.mainnet-beta.solana.com";

pub struct SniperClient {
    api: ApiClient,
    pub wallet: String,
}

impl SniperClient {
    /// Create a fully authenticated client.
    /// Requires SOL_ADDRESS + OKX API keys (via ApiClient).
    /// SOL_PRIVATE_KEY is only needed for live swap execution.
    pub fn new() -> Result<Self> {
        let api = ApiClient::new(None)?;
        let wallet = std::env::var("SOL_ADDRESS")
            .context("SOL_ADDRESS not set — required for ranking sniper")?;
        Ok(Self { api, wallet })
    }

    /// Create client for read-only operations (no wallet needed for data queries).
    pub fn new_read_only() -> Result<Self> {
        let api = ApiClient::new(None)?;
        let wallet = std::env::var("SOL_ADDRESS").unwrap_or_default();
        Ok(Self { api, wallet })
    }

    // ── Data queries ────────────────────────────────────────────────

    /// Fetch Solana top tokens by 24h price change (trending).
    pub async fn fetch_ranking(&self, top_n: usize) -> Result<Vec<Value>> {
        let data = self
            .api
            .get(
                "/api/v6/dex/market/token/toplist",
                &[
                    ("chains", CHAIN_INDEX),
                    ("sortBy", "2"),    // sort by price change
                    ("timeFrame", "1"), // 5 minutes
                ],
            )
            .await?;

        let tokens = match data {
            Value::Array(arr) => arr,
            _ => data.as_array().cloned().unwrap_or_default(),
        };

        Ok(tokens.into_iter().take(top_n).collect())
    }

    /// Fetch advanced token info for safety checks.
    pub async fn fetch_advanced_info(&self, token_addr: &str) -> Result<Value> {
        let data = self
            .api
            .get(
                "/api/v6/dex/market/token/advanced-info",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                ],
            )
            .await?;

        // Response may be array or object
        match data {
            Value::Array(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            Value::Object(_) => Ok(data),
            _ => bail!("unexpected advanced-info response format"),
        }
    }

    /// Fetch current token price in USD.
    /// The v6 price-info endpoint requires POST with a JSON array body.
    pub async fn fetch_price(&self, token_addr: &str) -> Result<f64> {
        let body = serde_json::json!([{
            "tokenContractAddress": token_addr,
            "chainIndex": CHAIN_INDEX,
        }]);
        let data = self
            .api
            .post("/api/v6/dex/market/price-info", &body)
            .await?;

        // Response is an array
        let item = match &data {
            Value::Array(arr) if !arr.is_empty() => &arr[0],
            _ => &data,
        };

        let price = safe_float(&item["price"], 0.0);
        if price <= 0.0 {
            bail!("invalid price for {token_addr}");
        }
        Ok(price)
    }

    /// Fetch holder data filtered by tag (6=Suspicious, 8=Phishing).
    pub async fn fetch_holder_risk(
        &self,
        token_addr: &str,
        tag_filter: &str,
    ) -> Result<Vec<Value>> {
        let data = self
            .api
            .get(
                "/api/v6/dex/market/token/holder",
                &[
                    ("tokenContractAddress", token_addr),
                    ("chainIndex", CHAIN_INDEX),
                    ("tagFilter", tag_filter),
                ],
            )
            .await?;

        match data {
            Value::Array(arr) => Ok(arr),
            _ => Ok(data.as_array().cloned().unwrap_or_default()),
        }
    }

    // ── Swap execution ──────────────────────────────────────────────

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
        eprintln!(
            "[swap] {} -> {} amount={} | routerResult.toTokenAmount={} | tx.data.len={}",
            from_token,
            to_token,
            amount_raw,
            amount_out,
            swap_data["tx"]["data"]
                .as_str()
                .map(|s| s.len())
                .unwrap_or(0)
        );

        // Step 2: Extract the unsigned transaction (base58-encoded)
        let tx_data_b58 = swap_data["tx"]["data"]
            .as_str()
            .context("missing tx.data in swap response")?;

        // Step 3: Sign and broadcast
        let tx_hash = self.sign_and_broadcast(tx_data_b58).await?;

        Ok(SwapResult {
            tx_hash: Some(tx_hash),
            amount_out,
            raw_response: swap_data,
        })
    }

    /// Sign a base58-encoded Solana transaction and broadcast it.
    /// First tries with the original blockhash from OKX (fastest path).
    /// If that fails with "Blockhash not found", fetches a fresh one and retries.
    async fn sign_and_broadcast(&self, tx_data_b58: &str) -> Result<String> {
        let pk_b58 = std::env::var("SOL_PRIVATE_KEY")
            .context("SOL_PRIVATE_KEY not set — required for swap execution")?;

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

        let tx_bytes = bs58::decode(tx_data_b58)
            .into_vec()
            .context("invalid base58 transaction data")?;

        // Strategy: try fresh blockhash + Solana RPC first, fall back to OKX broadcast
        let fresh_blockhash = self.get_latest_blockhash().await?;
        let mut tx_with_fresh = tx_bytes.clone();
        replace_blockhash(&mut tx_with_fresh, &fresh_blockhash)?;
        let signed_fresh = sign_solana_transaction(&tx_with_fresh, &signing_key)?;
        let signed_fresh_b58 = bs58::encode(&signed_fresh).into_string();

        // Try Solana RPC direct broadcast (fastest confirmation)
        match self.broadcast_to_solana_rpc(&signed_fresh_b58).await {
            Ok(hash) => {
                eprintln!("[broadcast] Solana RPC accepted: {}", hash);
                // Verify on-chain
                for attempt in 0..15 {
                    if attempt > 0 {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                    if let Ok(true) = self.check_tx_on_solana(&hash).await {
                        eprintln!("[broadcast] confirmed on-chain: {}", hash);
                        return Ok(hash);
                    }
                }
                eprintln!("[broadcast] Solana RPC tx not confirmed after 45s");
            }
            Err(e) => {
                eprintln!("[broadcast] Solana RPC rejected: {:#}", e);
            }
        }

        // Fall back: sign with ORIGINAL blockhash and broadcast via OKX
        // OKX may handle blockhash refresh and resubmission on their side
        eprintln!("[broadcast] falling back to OKX broadcast with original blockhash");
        let signed_orig = sign_solana_transaction(&tx_bytes, &signing_key)?;
        let signed_orig_b58 = bs58::encode(&signed_orig).into_string();

        let body = serde_json::json!({
            "chainIndex": CHAIN_INDEX,
            "signedTx": signed_orig_b58,
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

        eprintln!(
            "[broadcast] OKX response: {}",
            serde_json::to_string(&result).unwrap_or_default()
        );

        let order_id = result["orderId"].as_str().unwrap_or("").to_string();
        if order_id.is_empty() {
            bail!("OKX broadcast returned no orderId");
        }

        // Poll OKX order status for real txHash
        eprintln!("[broadcast] polling orderId: {}", order_id);
        for attempt in 0..20 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
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
                    let confirmed_hash = order["txHash"].as_str().unwrap_or("");
                    if attempt % 5 == 0 {
                        eprintln!(
                            "[broadcast] poll {}: status={} txHash={}",
                            attempt, tx_status, confirmed_hash
                        );
                    }
                    if tx_status == "2" && !confirmed_hash.is_empty() {
                        // Verify this is a real Solana txHash
                        if let Ok(true) = self.check_tx_on_solana(confirmed_hash).await {
                            eprintln!("[broadcast] OKX tx confirmed on-chain: {}", confirmed_hash);
                            return Ok(confirmed_hash.to_string());
                        }
                    }
                    if tx_status == "3" {
                        let reason = order["failReason"].as_str().unwrap_or("unknown");
                        bail!("transaction failed on-chain: {}", reason);
                    }
                }
            }
        }

        bail!("transaction not confirmed after 60s (orderId={})", order_id)
    }

    /// Get latest blockhash from Solana RPC.
    async fn get_latest_blockhash(&self) -> Result<[u8; 32]> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": [{"commitment": "finalized"}]
        });
        let resp = reqwest::Client::new()
            .post(SOLANA_RPC)
            .json(&body)
            .send()
            .await?;
        let data: Value = resp.json().await?;
        let bh_str = data["result"]["value"]["blockhash"]
            .as_str()
            .context("failed to get blockhash from RPC")?;
        let bh_bytes = bs58::decode(bh_str)
            .into_vec()
            .context("invalid blockhash base58")?;
        let mut result = [0u8; 32];
        result.copy_from_slice(&bh_bytes);
        Ok(result)
    }

    /// Broadcast a signed transaction directly to Solana RPC.
    /// Returns the tx hash on success, or an error with the Solana error details.
    async fn broadcast_to_solana_rpc(&self, signed_b58: &str) -> Result<String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [signed_b58, {"encoding": "base58", "skipPreflight": true}]
        });
        let resp = reqwest::Client::new()
            .post(SOLANA_RPC)
            .json(&body)
            .send()
            .await?;
        let data: Value = resp.json().await?;
        if let Some(error) = data.get("error") {
            bail!(
                "Solana RPC error: {}",
                serde_json::to_string(error).unwrap_or_default()
            );
        }
        let hash = data["result"].as_str().unwrap_or("").to_string();
        if hash.is_empty() {
            bail!("Solana RPC returned empty result");
        }
        Ok(hash)
    }

    /// Check if a transaction is confirmed on Solana via public RPC.
    async fn check_tx_on_solana(&self, tx_hash: &str) -> Result<bool> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[tx_hash], {"searchTransactionHistory": true}]
        });
        let resp = reqwest::Client::new()
            .post(SOLANA_RPC)
            .json(&body)
            .send()
            .await?;
        let data: Value = resp.json().await?;
        let status = &data["result"]["value"][0];
        if status.is_null() {
            return Ok(false);
        }
        // confirmationStatus: "processed" | "confirmed" | "finalized"
        let conf = status["confirmationStatus"].as_str().unwrap_or("");
        Ok(conf == "confirmed" || conf == "finalized")
    }

    /// Ensure the wSOL ATA exists and has at least `amount_lamports` in it.
    /// OKX ProFi expects the wSOL ATA to be pre-funded before the swap.
    /// If the account exists with enough balance, do nothing.
    /// If it exists with insufficient balance, top it up.
    /// If it doesn't exist, create it, fund it, and sync.
    async fn ensure_wsol_ata(&self, amount_lamports: u64) -> Result<()> {
        let pk_b58 = std::env::var("SOL_PRIVATE_KEY").context("SOL_PRIVATE_KEY not set")?;
        let pk_bytes = bs58::decode(&pk_b58).into_vec()?;
        let signing_key = if pk_bytes.len() == 64 {
            ed25519_dalek::SigningKey::from_keypair_bytes(pk_bytes.as_slice().try_into()?)
                .context("invalid ed25519 keypair")?
        } else if pk_bytes.len() == 32 {
            ed25519_dalek::SigningKey::from_bytes(pk_bytes.as_slice().try_into()?)
        } else {
            bail!("SOL_PRIVATE_KEY must be 32 or 64 bytes");
        };

        // Check if wSOL ATA exists and its balance
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getTokenAccountsByOwner",
            "params": [self.wallet, {"mint": SOL_NATIVE}, {"encoding": "jsonParsed"}]
        });
        let resp = reqwest::Client::new()
            .post(SOLANA_RPC)
            .json(&body)
            .send()
            .await?;
        let data: Value = resp.json().await?;
        let accounts = data["result"]["value"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let existing_balance: u64 = accounts
            .first()
            .and_then(|a| a["account"]["data"]["parsed"]["info"]["tokenAmount"]["amount"].as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Fund generously: OKX node may see stale balance, so deposit 2x the swap
        // amount plus buffer to ensure the swap tx always sees enough funds.
        let needed = amount_lamports * 2 + 20_000_000; // 2x swap + ~0.02 SOL buffer
        if existing_balance >= needed {
            eprintln!(
                "[wsol] ATA exists with {} lamports (need {}), skipping",
                existing_balance, needed
            );
            return Ok(());
        }

        let deposit = needed - existing_balance;
        eprintln!(
            "[wsol] ensuring wSOL ATA has {} lamports (current: {}, depositing: {})",
            needed, existing_balance, deposit
        );

        let wallet_bytes = bs58::decode(&self.wallet).into_vec()?;
        let blockhash = self.get_latest_blockhash().await?;

        let ata_exists = !accounts.is_empty();
        let tx = build_create_and_fund_wsol_tx(&wallet_bytes, &blockhash, deposit, ata_exists)?;

        let signed = sign_solana_transaction(&tx, &signing_key)?;
        let signed_b58 = bs58::encode(&signed).into_string();

        match self.broadcast_to_solana_rpc(&signed_b58).await {
            Ok(hash) => {
                eprintln!("[wsol] fund tx: {}", hash);
                for _ in 0..15 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    if let Ok(true) = self.check_tx_on_solana(&hash).await {
                        eprintln!("[wsol] ATA funded, waiting 10s for network sync...");
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        return Ok(());
                    }
                }
                bail!("wSOL ATA funding tx not confirmed");
            }
            Err(e) => {
                bail!("failed to fund wSOL ATA: {:#}", e);
            }
        }
    }

    /// Close wSOL ATA after a sell to recover SOL.
    pub async fn close_wsol_ata(&self) -> Result<()> {
        let pk_b58 = std::env::var("SOL_PRIVATE_KEY").context("SOL_PRIVATE_KEY not set")?;
        let pk_bytes = bs58::decode(&pk_b58).into_vec()?;
        let signing_key = if pk_bytes.len() == 64 {
            ed25519_dalek::SigningKey::from_keypair_bytes(pk_bytes.as_slice().try_into()?)
                .context("invalid ed25519 keypair")?
        } else if pk_bytes.len() == 32 {
            ed25519_dalek::SigningKey::from_bytes(pk_bytes.as_slice().try_into()?)
        } else {
            bail!("SOL_PRIVATE_KEY must be 32 or 64 bytes");
        };

        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getTokenAccountsByOwner",
            "params": [self.wallet, {"mint": SOL_NATIVE}, {"encoding": "jsonParsed"}]
        });
        let resp = reqwest::Client::new()
            .post(SOLANA_RPC)
            .json(&body)
            .send()
            .await?;
        let data: Value = resp.json().await?;
        let accounts = data["result"]["value"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        for account in &accounts {
            let pubkey = account["pubkey"].as_str().unwrap_or("");
            let wallet_bytes = bs58::decode(&self.wallet).into_vec()?;
            let account_bytes = bs58::decode(pubkey).into_vec()?;
            let token_program =
                bs58::decode("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").into_vec()?;
            let blockhash = self.get_latest_blockhash().await?;

            let tx =
                build_close_account_tx(&wallet_bytes, &account_bytes, &token_program, &blockhash)?;
            let signed = sign_solana_transaction(&tx, &signing_key)?;
            let signed_b58 = bs58::encode(&signed).into_string();

            match self.broadcast_to_solana_rpc(&signed_b58).await {
                Ok(hash) => {
                    eprintln!("[cleanup] wSOL close tx: {}", hash);
                    for _ in 0..10 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        if let Ok(true) = self.check_tx_on_solana(&hash).await {
                            eprintln!("[cleanup] wSOL account closed");
                            break;
                        }
                    }
                }
                Err(e) => eprintln!("[cleanup] close failed: {:#}", e),
            }
        }
        Ok(())
    }

    /// Buy a token with SOL.
    pub async fn buy_token(&self, token_addr: &str, sol_amount: f64) -> Result<SwapResult> {
        let amount_lamports = (sol_amount * 10f64.powi(SOL_DECIMALS as i32)) as u64;
        // Ensure wSOL ATA exists and is funded before OKX builds the swap tx
        self.ensure_wsol_ata(amount_lamports).await?;
        let amount_raw = format!("{}", amount_lamports);
        self.execute_swap(SOL_NATIVE, token_addr, &amount_raw).await
    }

    /// Sell a token for SOL. `amount_raw` is the raw token amount.
    pub async fn sell_token(&self, token_addr: &str, amount_raw: &str) -> Result<SwapResult> {
        self.execute_swap(token_addr, SOL_NATIVE, amount_raw).await
    }
}

/// Sign a Solana serialized transaction.
///
/// Solana wire format:
///   [compact-u16 num_signatures] [64-byte signature × num_signatures] [message...]
///
/// The OKX API returns a transaction with placeholder (zero) signatures.
/// We replace the first signature with our ed25519 signature over the message.
fn sign_solana_transaction(
    tx_bytes: &[u8],
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<Vec<u8>> {
    use ed25519_dalek::Signer;

    if tx_bytes.is_empty() {
        bail!("empty transaction data");
    }

    // Parse compact-u16 for number of signatures
    let (num_sigs, offset) = decode_compact_u16(tx_bytes)?;
    if num_sigs == 0 {
        bail!("transaction has 0 signatures slots");
    }

    let sigs_end = offset + (num_sigs as usize) * 64;
    if sigs_end > tx_bytes.len() {
        bail!("transaction too short for {} signatures", num_sigs);
    }

    // The message is everything after the signatures
    let message = &tx_bytes[sigs_end..];

    // Sign the message
    let signature = signing_key.sign(message);

    // Build signed transaction: same structure but with our signature in slot 0
    let mut signed = Vec::with_capacity(tx_bytes.len());
    signed.extend_from_slice(&tx_bytes[..offset]); // compact-u16 header
    signed.extend_from_slice(&signature.to_bytes()); // our signature (64 bytes)
                                                     // Keep remaining signature slots (if any) as-is
    if num_sigs > 1 {
        signed.extend_from_slice(&tx_bytes[offset + 64..sigs_end]);
    }
    signed.extend_from_slice(message); // the message

    Ok(signed)
}

/// Replace the blockhash in a Solana transaction's message.
///
/// Supports both legacy and versioned (v0) transactions.
///
/// Legacy message layout:
///   [u8 num_required_sigs] [u8 num_readonly_signed] [u8 num_readonly_unsigned]
///   [compact-u16 num_accounts] [32-byte pubkey × num_accounts]
///   [32-byte recent_blockhash] [instructions...]
///
/// Versioned (v0) message layout:
///   [u8 0x80 (version)] [u8 num_required_sigs] [u8 num_readonly_signed] [u8 num_readonly_unsigned]
///   [compact-u16 num_static_accounts] [32-byte pubkey × num_static_accounts]
///   [32-byte recent_blockhash] [instructions...] [address_table_lookups...]
fn replace_blockhash(tx_bytes: &mut [u8], new_blockhash: &[u8]) -> Result<()> {
    if new_blockhash.len() != 32 {
        bail!("blockhash must be 32 bytes");
    }

    // Find where the message starts (after signatures)
    let (num_sigs, sig_header_len) = decode_compact_u16(tx_bytes)?;
    let msg_start = sig_header_len + (num_sigs as usize) * 64;

    if msg_start >= tx_bytes.len() {
        bail!("transaction too short for message");
    }

    let msg = &tx_bytes[msg_start..];

    // Detect versioned (v0) vs legacy message.
    // Versioned messages have the high bit set in the first byte (0x80 = v0).
    // Legacy messages start with num_required_signatures which is always < 0x80.
    let version_offset = if msg[0] & 0x80 != 0 { 1 } else { 0 };

    if msg.len() < version_offset + 3 {
        bail!("transaction too short for message header");
    }

    // After optional version byte + 3-byte header, read compact-u16 for account count
    let header_end = version_offset + 3;
    let (num_accounts, accounts_header_len) = decode_compact_u16(&msg[header_end..])?;
    let accounts_start = header_end + accounts_header_len;
    let accounts_end = accounts_start + (num_accounts as usize) * 32;

    // The blockhash immediately follows the account keys
    let bh_offset = msg_start + accounts_end;
    if bh_offset + 32 > tx_bytes.len() {
        bail!("transaction too short for blockhash at offset {bh_offset}");
    }

    tx_bytes[bh_offset..bh_offset + 32].copy_from_slice(new_blockhash);
    Ok(())
}

/// Decode a Solana compact-u16 from a byte slice.
/// Returns (value, bytes_consumed).
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

/// Derive the Associated Token Account (ATA) address for wSOL.
/// ATA = PDA of [wallet, TOKEN_PROGRAM, mint] seeded with ATA_PROGRAM.
fn derive_wsol_ata(wallet: &[u8]) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha256};

    let ata_program = bs58::decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").into_vec()?;
    let token_program = bs58::decode("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").into_vec()?;
    let mint = bs58::decode(SOL_NATIVE).into_vec()?;

    // find_program_address: try nonce 255..0, PDA must NOT be on ed25519 curve
    for nonce in (0..=255u8).rev() {
        let mut hasher = Sha256::new();
        hasher.update(wallet);
        hasher.update(&token_program);
        hasher.update(&mint);
        hasher.update([nonce]);
        hasher.update(&ata_program);
        hasher.update(b"ProgramDerivedAddress");
        let hash = hasher.finalize();

        // A valid PDA is NOT on the ed25519 curve.
        // VerifyingKey::from_bytes succeeds only for on-curve points.
        let hash_arr: [u8; 32] = hash.into();
        if ed25519_dalek::VerifyingKey::from_bytes(&hash_arr).is_err() {
            return Ok(hash_arr.to_vec());
        }
    }
    bail!("failed to derive wSOL ATA address")
}

/// Build a transaction that creates (if needed) and funds the wSOL ATA.
///
/// If ATA doesn't exist: CreateAssociatedTokenAccount + SystemTransfer + SyncNative
/// If ATA exists: SystemTransfer + SyncNative
fn build_create_and_fund_wsol_tx(
    wallet: &[u8],
    blockhash: &[u8],
    deposit_lamports: u64,
    ata_exists: bool,
) -> Result<Vec<u8>> {
    let ata_bytes = derive_wsol_ata(wallet)?;
    let ata_program = bs58::decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").into_vec()?;
    let token_program = bs58::decode("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").into_vec()?;
    let system_program = bs58::decode("11111111111111111111111111111111").into_vec()?;
    let mint = bs58::decode(SOL_NATIVE).into_vec()?;

    let mut tx = Vec::with_capacity(512);

    // 1 signature
    tx.push(1u8);
    tx.extend_from_slice(&[0u8; 64]);

    if ata_exists {
        // Message header: 1 signer, 0 readonly signed, 2 readonly unsigned
        tx.push(1); // num_required_signatures
        tx.push(0); // num_readonly_signed
        tx.push(2); // num_readonly_unsigned (system_program, token_program)

        // 4 accounts: wallet(signer,writable), ata(writable), system_program(ro), token_program(ro)
        tx.push(4u8);
        tx.extend_from_slice(wallet); // [0] wallet
        tx.extend_from_slice(&ata_bytes); // [1] ata
        tx.extend_from_slice(&system_program); // [2] system
        tx.extend_from_slice(&token_program); // [3] token

        tx.extend_from_slice(blockhash);

        // 2 instructions: System Transfer + SyncNative
        tx.push(2u8);

        // Instruction 1: System Transfer (wallet -> ATA)
        tx.push(2); // program_id = system_program [2]
        tx.push(2); // 2 account indices
        tx.push(0); // from: wallet
        tx.push(1); // to: ata
        let mut transfer_data = vec![2, 0, 0, 0]; // Transfer instruction (opcode 2)
        transfer_data.extend_from_slice(&deposit_lamports.to_le_bytes());
        tx.push(transfer_data.len() as u8);
        tx.extend_from_slice(&transfer_data);

        // Instruction 2: SyncNative (Token Program, opcode 17)
        tx.push(3); // program_id = token_program [3]
        tx.push(1); // 1 account
        tx.push(1); // ata
        tx.push(1); // 1 byte data
        tx.push(17); // SyncNative opcode
    } else {
        // Message header: 1 signer, 0 readonly signed, 4 readonly unsigned
        tx.push(1); // num_required_signatures
        tx.push(0); // num_readonly_signed
        tx.push(4); // num_readonly_unsigned

        // 6 accounts
        tx.push(6u8);
        tx.extend_from_slice(wallet); // [0] wallet (signer, writable)
        tx.extend_from_slice(&ata_bytes); // [1] ata (writable)
        tx.extend_from_slice(&system_program); // [2] system (readonly)
        tx.extend_from_slice(&token_program); // [3] token (readonly)
        tx.extend_from_slice(&mint); // [4] mint (readonly)
        tx.extend_from_slice(&ata_program); // [5] ata_program (readonly)

        tx.extend_from_slice(blockhash);

        // 3 instructions: CreateATA + Transfer + SyncNative
        tx.push(3u8);

        // Instruction 1: Create Associated Token Account
        // ATA program instruction with accounts: [funder, ata, wallet, mint, system, token_program]
        tx.push(5); // program_id = ata_program [5]
        tx.push(6); // 6 accounts
        tx.push(0); // funder (wallet)
        tx.push(1); // ata
        tx.push(0); // wallet (owner)
        tx.push(4); // mint
        tx.push(2); // system_program
        tx.push(3); // token_program
        tx.push(0); // 0 bytes of data (CreateAssociatedTokenAccount has no data)

        // Instruction 2: System Transfer (wallet -> ATA)
        tx.push(2); // program_id = system [2]
        tx.push(2); // 2 accounts
        tx.push(0); // from: wallet
        tx.push(1); // to: ata
        let mut transfer_data = vec![2, 0, 0, 0]; // Transfer opcode
        transfer_data.extend_from_slice(&deposit_lamports.to_le_bytes());
        tx.push(transfer_data.len() as u8);
        tx.extend_from_slice(&transfer_data);

        // Instruction 3: SyncNative
        tx.push(3); // program_id = token [3]
        tx.push(1);
        tx.push(1); // ata
        tx.push(1);
        tx.push(17); // SyncNative
    }

    Ok(tx)
}

/// Build a legacy Solana transaction that closes a token account.
///
/// Token Program CloseAccount instruction (opcode 9):
///   Accounts: [token_account(writable), destination(writable), owner(signer)]
///
/// Transaction layout:
///   [1 byte: num_sigs=1] [64 bytes: signature placeholder]
///   Message: [header] [3 accounts] [blockhash] [1 instruction]
fn build_close_account_tx(
    wallet: &[u8],        // 32 bytes
    token_account: &[u8], // 32 bytes
    token_program: &[u8], // 32 bytes
    blockhash: &[u8],     // 32 bytes
) -> Result<Vec<u8>> {
    let mut tx = Vec::with_capacity(256);

    // Compact-u16: 1 signature
    tx.push(1u8);
    // Placeholder signature (64 zero bytes)
    tx.extend_from_slice(&[0u8; 64]);

    // Message header: [num_required_sigs, num_readonly_signed, num_readonly_unsigned]
    // wallet = signer+writable, token_account = writable, token_program = readonly
    tx.push(1); // 1 required signature (wallet)
    tx.push(0); // 0 readonly signed
    tx.push(1); // 1 readonly unsigned (token_program)

    // Compact-u16: 3 accounts
    tx.push(3u8);

    // Account keys (order matters: signers first, then writable, then readonly)
    // 0: wallet (signer, writable) — fee payer + owner + destination
    tx.extend_from_slice(wallet);
    // 1: token_account (writable, not signer)
    tx.extend_from_slice(token_account);
    // 2: token_program (readonly)
    tx.extend_from_slice(token_program);

    // Recent blockhash
    tx.extend_from_slice(blockhash);

    // Instructions: 1 instruction
    tx.push(1u8); // compact-u16: 1 instruction

    // Instruction: CloseAccount
    tx.push(2); // program_id_index = 2 (token_program)
    tx.push(3); // compact-u16: 3 account indices
    tx.push(1); // token_account
    tx.push(0); // destination (wallet)
    tx.push(0); // owner (wallet)
    tx.push(1); // compact-u16: 1 byte of data
    tx.push(9); // CloseAccount opcode

    Ok(tx)
}

pub struct SwapResult {
    pub tx_hash: Option<String>,
    pub amount_out: f64,
    pub raw_response: Value,
}
