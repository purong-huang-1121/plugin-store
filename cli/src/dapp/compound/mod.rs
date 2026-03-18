//! Compound V3 (cUSDCv3 Comet) client for Base chain.

use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use alloy::sol;
use alloy::sol_types::SolCall;
use anyhow::{Context, Result};
use serde_json::json;
use std::str::FromStr;

/// Signing mode for write operations.
pub enum SignerMode {
    OnchainOs { chain_flag: String },
}

sol! {
    #[sol(rpc)]
    interface IComet {
        function supply(address asset, uint256 amount) external;
        function withdraw(address asset, uint256 amount) external;
        function balanceOf(address account) external view returns (uint256);
        function getSupplyRate(uint256 utilization) external view returns (uint64);
        function getUtilization() external view returns (uint256);
        function totalSupply() external view returns (uint256);
    }
}

sol! {
    #[sol(rpc)]
    interface IERC20Compound {
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
    }
}

pub struct CompoundClient {
    comet_address: Address,
    usdc_address: Address,
    rpc_url: String,
    signer: Option<SignerMode>,
}

impl CompoundClient {
    /// Read-only client.
    pub fn new(comet_address: &str, usdc_address: &str, rpc_url: &str) -> Result<Self> {
        Ok(Self {
            comet_address: Address::from_str(comet_address).context("invalid comet address")?,
            usdc_address: Address::from_str(usdc_address).context("invalid USDC address")?,
            rpc_url: rpc_url.to_string(),
            signer: None,
        })
    }

    /// Client that signs via onchainos wallet CLI.
    pub fn new_with_onchainos(comet_address: &str, usdc_address: &str, rpc_url: &str, chain_name: &str) -> Result<Self> {
        let chain_flag = crate::onchainos::chain_flag(chain_name).to_string();
        Ok(Self {
            comet_address: Address::from_str(comet_address).context("invalid comet address")?,
            usdc_address: Address::from_str(usdc_address).context("invalid USDC address")?,
            rpc_url: rpc_url.to_string(),
            signer: Some(SignerMode::OnchainOs { chain_flag }),
        })
    }

    fn address(&self) -> Result<Address> {
        match &self.signer {
            Some(SignerMode::OnchainOs { .. }) => {
                let addr_str = crate::onchainos::get_evm_address()?;
                Address::from_str(&addr_str).context("invalid onchainos EVM address")
            }
            None => anyhow::bail!("no signer configured"),
        }
    }

    /// Get current USDC supply APY as a percentage (e.g. 2.88 for 2.88%).
    pub async fn get_supply_apy(&self) -> Result<f64> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let comet = IComet::new(self.comet_address, &provider);

        let utilization = comet
            .getUtilization()
            .call()
            .await
            .context("failed to call getUtilization")?;
        let supply_rate = comet
            .getSupplyRate(utilization)
            .call()
            .await
            .context("failed to call getSupplyRate")?;

        // supply_rate is per-second rate scaled to 1e18
        let rate_per_second = supply_rate as f64 / 1e18;
        let seconds_per_year: f64 = 31_536_000.0;
        let apy = ((1.0 + rate_per_second).powf(seconds_per_year) - 1.0) * 100.0;
        Ok(apy)
    }

    /// Get total supply in USDC (raw U256, 6 decimals).
    pub async fn get_total_supply(&self) -> Result<U256> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let comet = IComet::new(self.comet_address, &provider);
        let result = comet
            .totalSupply()
            .call()
            .await
            .context("failed to call totalSupply")?;
        Ok(result)
    }

    /// Get user's USDC balance in Compound (raw U256, 6 decimals).
    pub async fn get_balance(&self) -> Result<U256> {
        let user = self.address()?;
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let comet = IComet::new(self.comet_address, &provider);
        let result = comet
            .balanceOf(user)
            .call()
            .await
            .context("failed to call balanceOf")?;
        Ok(result)
    }

    /// Supply USDC to Compound. Handles ERC-20 approve if needed.
    pub async fn supply(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("no signer for supply")?;
        let user = self.address()?;

        match signer {
            SignerMode::OnchainOs { chain_flag } => {
                let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);

                let erc20 = IERC20Compound::new(self.usdc_address, &provider);
                let allowance = erc20
                    .allowance(user, self.comet_address)
                    .call()
                    .await
                    .context("failed to check allowance")?;
                if allowance < amount {
                    let approve_calldata = IERC20Compound::approveCall {
                        spender: self.comet_address,
                        amount: U256::MAX,
                    }
                    .abi_encode();
                    crate::onchainos::contract_call(
                        chain_flag,
                        &format!("{}", self.usdc_address),
                        &format!("0x{}", hex::encode(&approve_calldata)),
                        "0",
                    )
                    .await?;
                    // Wait for approve to be confirmed on-chain before supplying
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                }

                let supply_calldata = IComet::supplyCall {
                    asset: self.usdc_address,
                    amount,
                }
                .abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    chain_flag,
                    &format!("{}", self.comet_address),
                    &format!("0x{}", hex::encode(&supply_calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "supply",
                    "protocol": "Compound V3",
                    "status": "success",
                    "tx_hash": tx_hash,
                }))
            }
        }
    }

    /// Withdraw USDC from Compound.
    pub async fn withdraw(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("no signer for withdraw")?;

        match signer {
            SignerMode::OnchainOs { chain_flag } => {
                let withdraw_calldata = IComet::withdrawCall {
                    asset: self.usdc_address,
                    amount,
                }
                .abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    chain_flag,
                    &format!("{}", self.comet_address),
                    &format!("0x{}", hex::encode(&withdraw_calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "withdraw",
                    "protocol": "Compound V3",
                    "status": "success",
                    "tx_hash": tx_hash,
                }))
            }
        }
    }
}
