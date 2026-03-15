//! Rebalance executor — withdraw from source protocol, supply to target protocol.

use alloy::primitives::U256;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use super::chains::{self, AutoRebalanceConfig};
use super::yield_monitor::Protocol;
use crate::dapp::aave::client::AaveClient;
use crate::dapp::compound::CompoundClient;
use crate::dapp::morpho::vault::MorphoVaultClient;

#[derive(Debug, Serialize)]
pub struct RebalanceResult {
    pub from: String,
    pub to: String,
    pub amount_usdc: String,
    pub transactions: Vec<Value>,
    pub total_gas_used: u64,
}

#[derive(Debug, Serialize)]
pub struct WithdrawResult {
    pub protocol: String,
    pub amount_usdc: String,
    pub tx_hash: String,
    pub gas_used: u64,
}

/// Execute a rebalance on Base (backward compat).
pub async fn execute_rebalance(
    from: Protocol,
    to: Protocol,
    amount: U256,
) -> Result<RebalanceResult> {
    execute_rebalance_on(from, to, amount, &chains::BASE_CONFIG, None).await
}

/// Execute a rebalance on a specific chain.
/// `morpho_vault_override` — if provided, use this vault address for Morpho deposits
/// instead of the hardcoded config default (supports dynamic vault discovery).
pub async fn execute_rebalance_on(
    from: Protocol,
    to: Protocol,
    amount: U256,
    chain_config: &'static AutoRebalanceConfig,
    morpho_vault_override: Option<&str>,
) -> Result<RebalanceResult> {
    let config = chain_config;
    let rpc = chains::rpc_url_for(config);
    let aave_chain = config.aave_chain_key;
    let mut transactions = Vec::new();
    let mut total_gas: u64 = 0;

    // Step 1: Withdraw from source
    let withdraw_result = match from {
        Protocol::Aave => {
            let client = AaveClient::new_with_signer(aave_chain)?;
            let (asset_addr, decimals) = client.resolve_asset("USDC").await?;
            client.withdraw(asset_addr, amount, decimals).await?
        }
        Protocol::Compound => {
            let client = CompoundClient::new_with_signer(config.compound_comet, config.usdc, &rpc)?;
            client.withdraw(amount).await?
        }
        Protocol::Morpho => {
            // For withdrawal, we need to find which vault has the balance.
            // Try the override first (it came from yield discovery), then fall back to config.
            let vault_addr = morpho_vault_override.unwrap_or(config.morpho_vault);
            let client = MorphoVaultClient::new_with_signer(vault_addr, config.usdc, &rpc)?;
            client.withdraw(amount).await?
        }
    };

    total_gas += extract_gas_used(&withdraw_result);
    transactions.push(withdraw_result);

    // Step 1.5: Verify wallet received USDC after withdrawal and use actual balance for supply
    // (matches TS behavior: use wallet balance, not original amount, to handle partial withdrawals)
    let wallet_usdc = check_wallet_usdc(config).await;
    if wallet_usdc.is_zero() {
        anyhow::bail!(
            "Post-withdraw verification failed: wallet USDC balance is 0 after withdrawing from {}",
            from
        );
    }
    let supply_amount = wallet_usdc; // Use actual wallet balance for supply

    // Step 2: Supply to target
    let supply_result = match to {
        Protocol::Aave => {
            let client = AaveClient::new_with_signer(aave_chain)?;
            let (asset_addr, decimals) = client.resolve_asset("USDC").await?;
            client.supply(asset_addr, supply_amount, decimals).await?
        }
        Protocol::Compound => {
            let client = CompoundClient::new_with_signer(config.compound_comet, config.usdc, &rpc)?;
            client.supply(supply_amount).await?
        }
        Protocol::Morpho => {
            // For deposit, use the best vault from yield discovery
            let vault_addr = morpho_vault_override.unwrap_or(config.morpho_vault);
            let client = MorphoVaultClient::new_with_signer(vault_addr, config.usdc, &rpc)?;
            client.deposit(supply_amount).await?
        }
    };

    total_gas += extract_gas_used(&supply_result);
    transactions.push(supply_result);

    // Step 3: Post-supply verification — confirm funds arrived in target protocol
    // (matches TS rebalance-executor.ts step 3: verify newBalance in target)
    let post_supply_balance = check_protocol_balance(to, config, morpho_vault_override).await;
    if post_supply_balance.is_zero() {
        eprintln!(
            "[WARN] Post-supply verification: target {} balance is 0 after supply",
            to
        );
    }

    let amount_usd = amount.to_string().parse::<f64>().unwrap_or(0.0) / 1e6;

    Ok(RebalanceResult {
        from: from.to_string(),
        to: to.to_string(),
        amount_usdc: format!("{:.2}", amount_usd),
        transactions,
        total_gas_used: total_gas,
    })
}

/// Deposit wallet USDC directly into a protocol (no withdrawal step).
/// Used for initial deposit when protocol=None.
pub async fn deposit_only(
    to: Protocol,
    amount: U256,
    chain_config: &'static AutoRebalanceConfig,
    morpho_vault_override: Option<&str>,
) -> Result<RebalanceResult> {
    let config = chain_config;
    let rpc = chains::rpc_url_for(config);
    let aave_chain = config.aave_chain_key;
    let mut transactions = Vec::new();

    let supply_result = match to {
        Protocol::Aave => {
            let client = AaveClient::new_with_signer(aave_chain)?;
            let (asset_addr, decimals) = client.resolve_asset("USDC").await?;
            client.supply(asset_addr, amount, decimals).await?
        }
        Protocol::Compound => {
            let client = CompoundClient::new_with_signer(config.compound_comet, config.usdc, &rpc)?;
            client.supply(amount).await?
        }
        Protocol::Morpho => {
            let vault_addr = morpho_vault_override.unwrap_or(config.morpho_vault);
            let client = MorphoVaultClient::new_with_signer(vault_addr, config.usdc, &rpc)?;
            client.deposit(amount).await?
        }
    };

    let total_gas = extract_gas_used(&supply_result);
    transactions.push(supply_result);

    let amount_usd = amount.to_string().parse::<f64>().unwrap_or(0.0) / 1e6;

    Ok(RebalanceResult {
        from: "wallet".to_string(),
        to: to.to_string(),
        amount_usdc: format!("{:.2}", amount_usd),
        transactions,
        total_gas_used: total_gas,
    })
}

/// Emergency withdraw on Base (backward compat).
pub async fn emergency_withdraw(protocol: Protocol) -> Result<WithdrawResult> {
    emergency_withdraw_on(protocol, &chains::BASE_CONFIG).await
}

/// Emergency withdraw all funds from a protocol on a specific chain.
pub async fn emergency_withdraw_on(
    protocol: Protocol,
    chain_config: &'static AutoRebalanceConfig,
) -> Result<WithdrawResult> {
    let config = chain_config;
    let rpc = chains::rpc_url_for(config);
    let aave_chain = config.aave_chain_key;

    let tx_result = match protocol {
        Protocol::Aave => {
            let client = AaveClient::new_with_signer(aave_chain)?;
            let (asset_addr, decimals) = client.resolve_asset("USDC").await?;
            client.withdraw(asset_addr, U256::MAX, decimals).await?
        }
        Protocol::Compound => {
            let client = CompoundClient::new_with_signer(config.compound_comet, config.usdc, &rpc)?;
            let balance = client.get_balance().await?;
            client.withdraw(balance).await?
        }
        Protocol::Morpho => {
            // Try all known vaults to find the one with balance
            let known_vaults = super::daemon::get_morpho_usdc_vaults(config).await;
            let mut result = None;
            for vault_addr in &known_vaults {
                if let Ok(m) = MorphoVaultClient::new_with_signer(vault_addr, config.usdc, &rpc) {
                    if let Ok(b) = m.get_balance_usdc().await {
                        if !b.is_zero() {
                            result = Some(m.withdraw(b).await?);
                            break;
                        }
                    }
                }
            }
            result.unwrap_or_else(
                || serde_json::json!({"error": "no Morpho vault with balance found"}),
            )
        }
    };

    let tx_hash = tx_result["txHash"]
        .as_str()
        .or_else(|| tx_result["tx_hash"].as_str())
        .unwrap_or("unknown")
        .to_string();
    let gas_used = extract_gas_used(&tx_result);

    let amount = tx_result["amount"]
        .as_str()
        .or_else(|| tx_result["amount_usdc"].as_str())
        .unwrap_or("0")
        .to_string();

    Ok(WithdrawResult {
        protocol: protocol.to_string(),
        amount_usdc: amount,
        tx_hash,
        gas_used,
    })
}

/// Extract gas_used from a transaction result JSON.
fn extract_gas_used(tx: &Value) -> u64 {
    tx.get("gas_used")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Check protocol balance for post-supply verification.
async fn check_protocol_balance(
    protocol: Protocol,
    config: &AutoRebalanceConfig,
    morpho_vault_override: Option<&str>,
) -> U256 {
    let rpc = chains::rpc_url_for(config);
    match protocol {
        Protocol::Aave => match AaveClient::new_with_signer(config.aave_chain_key) {
            Ok(aave) => aave.get_usdc_atoken_balance().await.unwrap_or(U256::ZERO),
            Err(_) => U256::ZERO,
        },
        Protocol::Compound => {
            match CompoundClient::new_with_signer(config.compound_comet, config.usdc, &rpc) {
                Ok(c) => c.get_balance().await.unwrap_or(U256::ZERO),
                Err(_) => U256::ZERO,
            }
        }
        Protocol::Morpho => {
            let vault_addr = morpho_vault_override.unwrap_or(config.morpho_vault);
            match MorphoVaultClient::new_with_signer(vault_addr, config.usdc, &rpc) {
                Ok(m) => m.get_balance_usdc().await.unwrap_or(U256::ZERO),
                Err(_) => U256::ZERO,
            }
        }
    }
}

/// Check wallet USDC balance for post-execution verification.
async fn check_wallet_usdc(config: &AutoRebalanceConfig) -> U256 {
    use alloy::primitives::Address;
    use alloy::providers::ProviderBuilder;
    use alloy::signers::local::PrivateKeySigner;
    use std::str::FromStr;

    let pk = match std::env::var("EVM_PRIVATE_KEY") {
        Ok(pk) => pk,
        Err(_) => return U256::ZERO,
    };
    let pk = pk.strip_prefix("0x").unwrap_or(&pk);
    let signer: PrivateKeySigner = match pk.parse() {
        Ok(s) => s,
        Err(_) => return U256::ZERO,
    };
    let user = signer.address();

    let rpc = chains::rpc_url_for(config);
    let provider = match rpc.parse() {
        Ok(url) => ProviderBuilder::new().connect_http(url),
        Err(_) => return U256::ZERO,
    };

    let usdc_addr = match Address::from_str(config.usdc) {
        Ok(a) => a,
        Err(_) => return U256::ZERO,
    };

    alloy::sol! {
        #[sol(rpc)]
        interface IERC20Bal {
            function balanceOf(address account) external view returns (uint256);
        }
    }

    let erc20 = IERC20Bal::new(usdc_addr, &provider);
    match erc20.balanceOf(user).call().await {
        Ok(balance) => balance,
        Err(_) => U256::ZERO,
    }
}
