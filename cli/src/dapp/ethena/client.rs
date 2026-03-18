//! Ethena sUSDe client for staking/unstaking USDe.

use std::str::FromStr;

use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use alloy::sol_types::SolCall;
use anyhow::{bail, Context, Result};
use serde_json::json;

use super::contracts::{IStakedUSDe, IERC20};

// Ethereum mainnet addresses
const SUSDE_ADDRESS: &str = "0x9D39A5DE30e57443BfF2A8307A4256c8797A3497";
const USDE_ADDRESS: &str = "0x4c9EDD5852cd905f086C759E8383e09bff1E68B3";
const ETH_RPC: &str = "https://ethereum-rpc.publicnode.com";
const ONCHAINOS_CHAIN: &str = "eth";

/// Signing mode for write operations.
pub enum SignerMode {
    /// onchainos wallet CLI signing.
    OnchainOs,
}

pub struct EthenaClient {
    signer: Option<SignerMode>,
    rpc_url: String,
}

impl EthenaClient {
    /// Create a read-only client.
    pub fn new() -> Result<Self> {
        let rpc_url = std::env::var("ETHENA_RPC_URL").unwrap_or_else(|_| ETH_RPC.to_string());
        Ok(Self {
            signer: None,
            rpc_url,
        })
    }

    /// Create a client that signs via onchainos wallet CLI.
    pub fn new_with_onchainos() -> Result<Self> {
        let rpc_url = std::env::var("ETHENA_RPC_URL").unwrap_or_else(|_| ETH_RPC.to_string());
        Ok(Self {
            signer: Some(SignerMode::OnchainOs),
            rpc_url,
        })
    }

    /// Get the signer's address.
    pub fn address(&self) -> Result<Address> {
        match &self.signer {
            Some(SignerMode::OnchainOs) => {
                let addr_str = crate::onchainos::get_evm_address()?;
                Address::from_str(&addr_str).context("invalid onchainos EVM address")
            }
            None => bail!("No signer configured"),
        }
    }

    /// Query current sUSDe yield info: exchange rate, total assets, APY estimate.
    pub async fn get_yield_info(&self) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let susde = IStakedUSDe::new(Address::from_str(SUSDE_ADDRESS)?, &provider);

        let one_share = U256::from(10).pow(U256::from(18)); // 1 sUSDe = 1e18
        let assets_per_share = susde.convertToAssets(one_share).call().await?;
        let total_assets = susde.totalAssets().call().await?;
        let total_supply = susde.totalSupply().call().await?;
        let cooldown = susde.cooldownDuration().call().await?;

        // Exchange rate: how much USDe per 1 sUSDe
        let rate_f = format_units_18(assets_per_share);

        Ok(json!({
            "susde_address": SUSDE_ADDRESS,
            "usde_address": USDE_ADDRESS,
            "exchange_rate": rate_f,
            "total_assets_usde": format_units_18(total_assets),
            "total_supply_susde": format_units_18(total_supply),
            "cooldown_duration_seconds": cooldown.to_string(),
            "cooldown_duration_days": cooldown.to::<u64>() as f64 / 86400.0,
        }))
    }

    /// Query user's sUSDe balance and its USDe value.
    pub async fn get_balance(&self, address: &str) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let susde = IStakedUSDe::new(Address::from_str(SUSDE_ADDRESS)?, &provider);
        let usde = IERC20::new(Address::from_str(USDE_ADDRESS)?, &provider);
        let user = Address::from_str(address)?;

        let susde_balance = susde.balanceOf(user).call().await?;
        let usde_balance = usde.balanceOf(user).call().await?;

        // Convert sUSDe balance to USDe value
        let usde_value = if susde_balance > U256::ZERO {
            susde.convertToAssets(susde_balance).call().await?
        } else {
            U256::ZERO
        };

        Ok(json!({
            "address": address,
            "susde_balance": format_units_18(susde_balance),
            "susde_value_usde": format_units_18(usde_value),
            "usde_balance": format_units_18(usde_balance),
        }))
    }

    /// Stake USDe → sUSDe.
    pub async fn stake(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("signer required for stake")?;
        let susde_addr = Address::from_str(SUSDE_ADDRESS)?;
        let usde_addr = Address::from_str(USDE_ADDRESS)?;
        let user = self.address()?;

        match signer {
            SignerMode::OnchainOs => {
                let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);

                let usde = IERC20::new(usde_addr, &provider);
                let current_allowance = usde.allowance(user, susde_addr).call().await?;
                if current_allowance < amount {
                    let approve_calldata = IERC20::approveCall {
                        spender: susde_addr,
                        amount,
                    }
                    .abi_encode();
                    crate::onchainos::contract_call(
                        ONCHAINOS_CHAIN,
                        &format!("{}", usde_addr),
                        &format!("0x{}", hex::encode(&approve_calldata)),
                        "0",
                    )
                    .await?;
                }

                let deposit_calldata = IStakedUSDe::depositCall {
                    assets: amount,
                    receiver: user,
                }
                .abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    ONCHAINOS_CHAIN,
                    &format!("{}", susde_addr),
                    &format!("0x{}", hex::encode(&deposit_calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "stake",
                    "amount_usde": format_units_18(amount),
                    "tx_hash": tx_hash,
                    "status": "success",
                }))
            }
        }
    }

    /// Initiate unstake cooldown for a given USDe amount.
    /// After cooldown period (7 days), call `unstake()` to withdraw.
    pub async fn cooldown(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("signer required for cooldown")?;
        let susde_addr = Address::from_str(SUSDE_ADDRESS)?;

        match signer {
            SignerMode::OnchainOs => {
                let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
                let susde = IStakedUSDe::new(susde_addr, &provider);
                let duration = susde.cooldownDuration().call().await?;

                let calldata = IStakedUSDe::cooldownAssetsCall { assets: amount }.abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    ONCHAINOS_CHAIN,
                    &format!("{}", susde_addr),
                    &format!("0x{}", hex::encode(&calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "cooldown_initiated",
                    "amount_usde": format_units_18(amount),
                    "cooldown_days": duration.to::<u64>() as f64 / 86400.0,
                    "tx_hash": tx_hash,
                    "status": "success",
                }))
            }
        }
    }

    /// Withdraw USDe after cooldown period has elapsed.
    pub async fn unstake(&self) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("signer required for unstake")?;
        let susde_addr = Address::from_str(SUSDE_ADDRESS)?;
        let user = self.address()?;

        match signer {
            SignerMode::OnchainOs => {
                let calldata = IStakedUSDe::unstakeCall { receiver: user }.abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    ONCHAINOS_CHAIN,
                    &format!("{}", susde_addr),
                    &format!("0x{}", hex::encode(&calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "unstake",
                    "receiver": format!("{}", user),
                    "tx_hash": tx_hash,
                    "status": "success",
                }))
            }
        }
    }
}

/// Format U256 with 18 decimals to human-readable string.
fn format_units_18(value: U256) -> String {
    let divisor = U256::from(10).pow(U256::from(18));
    let whole = value / divisor;
    let frac = value % divisor;
    let frac_str = format!("{:0>18}", frac);
    let trimmed = frac_str.trim_end_matches('0');
    let trimmed = if trimmed.len() < 4 {
        &frac_str[..4]
    } else {
        trimmed
    };
    format!("{}.{}", whole, trimmed)
}
