//! Morpho MetaMorpho vault client (ERC-4626) for on-chain operations.

use alloy::network::EthereumWallet;
use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use anyhow::{Context, Result};
use serde_json::json;
use std::str::FromStr;

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
    signer: Option<PrivateKeySigner>,
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

    /// Authenticated client with EVM_PRIVATE_KEY.
    pub fn new_with_signer(vault_address: &str, usdc_address: &str, rpc_url: &str) -> Result<Self> {
        let pk = std::env::var("EVM_PRIVATE_KEY")
            .context("EVM_PRIVATE_KEY env var required for write operations")?;
        let pk = pk.strip_prefix("0x").unwrap_or(&pk);
        let signer: PrivateKeySigner = pk.parse().context("invalid EVM_PRIVATE_KEY")?;
        Ok(Self {
            vault_address: Address::from_str(vault_address).context("invalid vault address")?,
            usdc_address: Address::from_str(usdc_address).context("invalid USDC address")?,
            rpc_url: rpc_url.to_string(),
            signer: Some(signer),
        })
    }

    fn address(&self) -> Result<Address> {
        self.signer
            .as_ref()
            .map(|s| s.address())
            .context("no signer configured")
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
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.parse()?);
        let user = signer.address();

        let erc20 = IERC20::new(self.usdc_address, &provider);
        let allowance = erc20
            .allowance(user, self.vault_address)
            .call()
            .await
            .context("failed to check allowance")?;
        if allowance < amount {
            let approve_receipt = erc20
                .approve(self.vault_address, U256::MAX)
                .send()
                .await
                .context("approve tx failed")?
                .get_receipt()
                .await
                .context("failed to get approve receipt")?;
            if !approve_receipt.status() {
                anyhow::bail!("approve transaction reverted");
            }
        }

        let vault = IERC4626::new(self.vault_address, &provider);
        let receipt = vault
            .deposit(amount, user)
            .send()
            .await
            .context("deposit tx failed")?
            .get_receipt()
            .await
            .context("failed to get deposit receipt")?;

        Ok(json!({
            "action": "deposit",
            "protocol": "Morpho",
            "status": if receipt.status() { "success" } else { "failed" },
            "tx_hash": format!("{}", receipt.transaction_hash),
            "block_number": receipt.block_number.unwrap_or_default(),
            "gas_used": receipt.gas_used.to_string(),
        }))
    }

    /// Withdraw USDC from the vault.
    pub async fn withdraw(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("no signer for withdraw")?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.parse()?);
        let user = signer.address();

        let vault = IERC4626::new(self.vault_address, &provider);
        let receipt = vault
            .withdraw(amount, user, user)
            .send()
            .await
            .context("withdraw tx failed")?
            .get_receipt()
            .await
            .context("failed to get withdraw receipt")?;

        Ok(json!({
            "action": "withdraw",
            "protocol": "Morpho",
            "status": if receipt.status() { "success" } else { "failed" },
            "tx_hash": format!("{}", receipt.transaction_hash),
            "block_number": receipt.block_number.unwrap_or_default(),
            "gas_used": receipt.gas_used.to_string(),
        }))
    }
}
