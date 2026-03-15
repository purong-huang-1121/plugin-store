//! Compound V3 (cUSDCv3 Comet) client for Base chain.

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
    signer: Option<PrivateKeySigner>,
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

    /// Authenticated client with EVM_PRIVATE_KEY.
    pub fn new_with_signer(comet_address: &str, usdc_address: &str, rpc_url: &str) -> Result<Self> {
        let pk = std::env::var("EVM_PRIVATE_KEY")
            .context("EVM_PRIVATE_KEY env var required for write operations")?;
        let pk = pk.strip_prefix("0x").unwrap_or(&pk);
        let signer: PrivateKeySigner = pk.parse().context("invalid EVM_PRIVATE_KEY")?;
        Ok(Self {
            comet_address: Address::from_str(comet_address).context("invalid comet address")?,
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
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.parse()?);
        let user = signer.address();

        // Check and set allowance
        let erc20 = IERC20Compound::new(self.usdc_address, &provider);
        let allowance = erc20
            .allowance(user, self.comet_address)
            .call()
            .await
            .context("failed to check allowance")?;
        if allowance < amount {
            let approve_receipt = erc20
                .approve(self.comet_address, U256::MAX)
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

        // Supply
        let comet = IComet::new(self.comet_address, &provider);
        let receipt = comet
            .supply(self.usdc_address, amount)
            .send()
            .await
            .context("supply tx failed")?
            .get_receipt()
            .await
            .context("failed to get supply receipt")?;

        Ok(json!({
            "action": "supply",
            "protocol": "Compound V3",
            "status": if receipt.status() { "success" } else { "failed" },
            "tx_hash": format!("{}", receipt.transaction_hash),
            "block_number": receipt.block_number.unwrap_or_default(),
            "gas_used": receipt.gas_used.to_string(),
        }))
    }

    /// Withdraw USDC from Compound.
    pub async fn withdraw(&self, amount: U256) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("no signer for withdraw")?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.parse()?);

        let comet = IComet::new(self.comet_address, &provider);
        let receipt = comet
            .withdraw(self.usdc_address, amount)
            .send()
            .await
            .context("withdraw tx failed")?
            .get_receipt()
            .await
            .context("failed to get withdraw receipt")?;

        Ok(json!({
            "action": "withdraw",
            "protocol": "Compound V3",
            "status": if receipt.status() { "success" } else { "failed" },
            "tx_hash": format!("{}", receipt.transaction_hash),
            "block_number": receipt.block_number.unwrap_or_default(),
            "gas_used": receipt.gas_used.to_string(),
        }))
    }
}
