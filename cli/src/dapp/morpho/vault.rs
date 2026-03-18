//! Morpho MetaMorpho vault client (ERC-4626) for on-chain operations.

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
    interface IERC4626 {
        function deposit(uint256 assets, address receiver) external returns (uint256 shares);
        function withdraw(uint256 assets, address receiver, address owner) external returns (uint256 shares);
        function redeem(uint256 shares, address receiver, address owner) external returns (uint256 assets);
        function convertToAssets(uint256 shares) external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function totalAssets() external view returns (uint256);
    }
}

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
    }
}

pub struct MorphoVaultClient {
    vault_address: Address,
    usdc_address: Address,
    rpc_url: String,
    signer: Option<SignerMode>,
}

impl MorphoVaultClient {
    /// Read-only client.
    pub fn new(vault_address: &str, usdc_address: &str, rpc_url: &str) -> Result<Self> {
        Ok(Self {
            vault_address: Address::from_str(vault_address).context("invalid vault address")?,
            usdc_address: Address::from_str(usdc_address).context("invalid USDC address")?,
            rpc_url: rpc_url.to_string(),
            signer: None,
        })
    }

    /// Client that signs via onchainos wallet CLI.
    pub fn new_with_onchainos(vault_address: &str, usdc_address: &str, rpc_url: &str, chain_name: &str) -> Result<Self> {
        let chain_flag = crate::onchainos::chain_flag(chain_name).to_string();
        Ok(Self {
            vault_address: Address::from_str(vault_address).context("invalid vault address")?,
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

    /// Get user's USDC-equivalent balance in the vault.
    pub async fn get_balance_usdc(&self) -> Result<U256> {
        let user = self.address()?;
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let vault = IERC4626::new(self.vault_address, &provider);

        let shares = vault
            .balanceOf(user)
            .call()
            .await
            .context("failed to call balanceOf")?;
        if shares.is_zero() {
            return Ok(U256::ZERO);
        }
        let assets = vault
            .convertToAssets(shares)
            .call()
            .await
            .context("failed to call convertToAssets")?;
        Ok(assets)
    }

    /// Get total USDC assets in the vault.
    pub async fn get_total_assets(&self) -> Result<U256> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);
        let vault = IERC4626::new(self.vault_address, &provider);
        let result = vault
            .totalAssets()
            .call()
            .await
            .context("failed to call totalAssets")?;
        Ok(result)
    }

    /// Deposit USDC into the vault. Handles ERC-20 approve if needed.
    pub async fn deposit(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("no signer for deposit")?;
        let user = self.address()?;

        match signer {
            SignerMode::OnchainOs { chain_flag } => {
                let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);

                let erc20 = IERC20::new(self.usdc_address, &provider);
                let allowance = erc20
                    .allowance(user, self.vault_address)
                    .call()
                    .await
                    .context("failed to check allowance")?;
                if allowance < amount {
                    let approve_calldata = IERC20::approveCall {
                        spender: self.vault_address,
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
                    // Wait for approve to be confirmed on-chain before depositing
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                }

                let deposit_calldata = IERC4626::depositCall {
                    assets: amount,
                    receiver: user,
                }
                .abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    chain_flag,
                    &format!("{}", self.vault_address),
                    &format!("0x{}", hex::encode(&deposit_calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "deposit",
                    "protocol": "Morpho",
                    "status": "success",
                    "tx_hash": tx_hash,
                }))
            }
        }
    }

    /// Withdraw USDC from the vault.
    pub async fn withdraw(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("no signer for withdraw")?;
        let user = self.address()?;

        match signer {
            SignerMode::OnchainOs { chain_flag } => {
                let withdraw_calldata = IERC4626::withdrawCall {
                    assets: amount,
                    receiver: user,
                    owner: user,
                }
                .abi_encode();
                let tx_hash = crate::onchainos::contract_call(
                    chain_flag,
                    &format!("{}", self.vault_address),
                    &format!("0x{}", hex::encode(&withdraw_calldata)),
                    "0",
                )
                .await?;

                Ok(json!({
                    "action": "withdraw",
                    "protocol": "Morpho",
                    "status": "success",
                    "tx_hash": tx_hash,
                }))
            }
        }
    }
}
