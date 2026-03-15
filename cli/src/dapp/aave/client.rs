//! Aave V3 client for on-chain reads and writes.

use std::str::FromStr;

use alloy::network::EthereumWallet;
use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{bail, Context, Result};
use serde_json::json;

use super::chains::{self, ChainConfig};
use super::contracts::{IPool, IERC20};
use super::pool_data;

/// Aave V3 client. Stores chain config and optional signer for write operations.
pub struct AaveClient {
    config: &'static ChainConfig,
    signer: Option<PrivateKeySigner>,
}

impl AaveClient {
    /// Create a read-only client for the given chain.
    pub fn new(chain: &str) -> Result<Self> {
        let config = chains::get_chain_config(chain)?;
        Ok(Self {
            config,
            signer: None,
        })
    }

    /// Create a client with signing capability from EVM_PRIVATE_KEY env var.
    pub fn new_with_signer(chain: &str) -> Result<Self> {
        let config = chains::get_chain_config(chain)?;
        let pk = std::env::var("EVM_PRIVATE_KEY")
            .context("EVM_PRIVATE_KEY env var required for write operations")?;
        let pk = pk.strip_prefix("0x").unwrap_or(&pk);
        let signer: PrivateKeySigner = pk.parse().context("invalid EVM_PRIVATE_KEY")?;
        Ok(Self {
            config,
            signer: Some(signer),
        })
    }

    /// Get the signer's address.
    pub fn address(&self) -> Result<Address> {
        match &self.signer {
            Some(s) => Ok(s.address()),
            None => bail!("No signer configured"),
        }
    }

    /// Fetch all reserve data from UiPoolDataProvider.
    pub async fn get_reserves_data(&self) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let ui_addr = Address::from_str(self.config.ui_pool_data_provider)?;
        let pool_addr_provider = Address::from_str(self.config.pool_address_provider)?;

        let (reserves, base_currency) =
            pool_data::get_reserves_data(&provider, ui_addr, pool_addr_provider).await?;

        let mut markets = Vec::new();
        for r in reserves.iter() {
            if !r.is_active {
                continue;
            }
            let supply_apy = ray_to_apy(U256::from(r.liquidity_rate));
            let borrow_apy = ray_to_apy(U256::from(r.variable_borrow_rate));

            markets.push(json!({
                "symbol": r.symbol,
                "name": r.name,
                "underlying_asset": format!("{}", r.underlying_asset),
                "decimals": r.decimals.to_string(),
                "supply_apy_percent": format!("{:.2}", supply_apy),
                "variable_borrow_apy_percent": format!("{:.2}", borrow_apy),
                "total_supplied": format_units(r.available_liquidity + r.total_variable_debt, r.decimals),
                "total_variable_debt": format_units(r.total_variable_debt, r.decimals),
                "available_liquidity": format_units(r.available_liquidity, r.decimals),
                "ltv_percent": format!("{:.2}", r.base_ltv.to::<u64>() as f64 / 100.0),
                "liquidation_threshold_percent": format!("{:.2}", r.liquidation_threshold.to::<u64>() as f64 / 100.0),
                "can_be_collateral": r.usage_as_collateral_enabled,
                "borrowing_enabled": r.borrowing_enabled,
                "is_frozen": r.is_frozen,
                "supply_cap": r.supply_cap.to_string(),
                "borrow_cap": r.borrow_cap.to_string(),
                "a_token_address": format!("{}", r.a_token_address),
            }));
        }

        Ok(json!({
            "chain_id": self.config.chain_id,
            "markets": markets,
            "base_currency": {
                "market_reference_currency_unit": base_currency.market_ref_currency_unit.to_string(),
                "market_reference_currency_price_usd": base_currency.market_ref_currency_price_usd.to_string(),
                "network_base_token_price_usd": base_currency.network_base_token_price_usd.to_string(),
            },
        }))
    }

    /// Get user account summary from the Pool contract.
    pub async fn get_account_data(&self, user: &str) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let pool = IPool::new(Address::from_str(self.config.pool)?, &provider);
        let user_addr = Address::from_str(user)?;

        let result = pool.getUserAccountData(user_addr).call().await?;

        Ok(json!({
            "chain_id": self.config.chain_id,
            "user": user,
            "total_collateral_base": format_base_currency(result.totalCollateralBase),
            "total_debt_base": format_base_currency(result.totalDebtBase),
            "available_borrows_base": format_base_currency(result.availableBorrowsBase),
            "current_liquidation_threshold": format!("{:.2}", result.currentLiquidationThreshold.to::<u64>() as f64 / 100.0),
            "ltv": format!("{:.2}", result.ltv.to::<u64>() as f64 / 100.0),
            "health_factor": format_health_factor(result.healthFactor),
        }))
    }

    /// Get user reserve positions.
    pub async fn get_user_reserves(&self, user: &str) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let ui_addr = Address::from_str(self.config.ui_pool_data_provider)?;
        let pool_addr_provider = Address::from_str(self.config.pool_address_provider)?;
        let user_addr = Address::from_str(user)?;

        let (reserves, _) =
            pool_data::get_reserves_data(&provider, ui_addr, pool_addr_provider).await?;
        let user_reserves =
            pool_data::get_user_reserves_data(&provider, ui_addr, pool_addr_provider, user_addr)
                .await?;

        let mut positions = Vec::new();
        for ur in user_reserves.iter() {
            if ur.scaled_a_token_balance.is_zero() && ur.scaled_variable_debt.is_zero() {
                continue;
            }
            let reserve = reserves
                .iter()
                .find(|r| r.underlying_asset == ur.underlying_asset);
            let (symbol, decimals) = match reserve {
                Some(r) => (r.symbol.clone(), r.decimals),
                None => ("???".to_string(), U256::from(18)),
            };

            positions.push(json!({
                "symbol": symbol,
                "underlying_asset": format!("{}", ur.underlying_asset),
                "scaled_a_token_balance": format_units(ur.scaled_a_token_balance, decimals),
                "scaled_variable_debt": format_units(ur.scaled_variable_debt, decimals),
                "usage_as_collateral": ur.usage_as_collateral,
            }));
        }

        Ok(json!({
            "chain_id": self.config.chain_id,
            "user": user,
            "positions": positions,
        }))
    }

    /// Get the exact aToken balance for USDC (includes accrued interest).
    /// Returns raw U256 with 6 decimals.
    pub async fn get_usdc_atoken_balance(&self) -> Result<U256> {
        let signer = self.signer.as_ref().context("signer required")?;
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let ui_addr = Address::from_str(self.config.ui_pool_data_provider)?;
        let pool_addr_provider = Address::from_str(self.config.pool_address_provider)?;

        let (reserves, _) =
            pool_data::get_reserves_data(&provider, ui_addr, pool_addr_provider).await?;

        // Find USDC reserve to get its aToken address
        let usdc_reserve = reserves
            .iter()
            .find(|r| r.symbol.to_uppercase() == "USDC")
            .context("USDC reserve not found")?;

        let a_token = IERC20::new(usdc_reserve.a_token_address, &provider);
        let balance = a_token.balanceOf(signer.address()).call().await?;
        Ok(balance)
    }

    /// Find a reserve by symbol and return its underlying asset address and decimals.
    pub async fn resolve_asset(&self, symbol: &str) -> Result<(Address, u8)> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let ui_addr = Address::from_str(self.config.ui_pool_data_provider)?;
        let pool_addr_provider = Address::from_str(self.config.pool_address_provider)?;

        let (reserves, _) =
            pool_data::get_reserves_data(&provider, ui_addr, pool_addr_provider).await?;

        let upper = symbol.to_uppercase();
        for r in reserves.iter() {
            if r.symbol.to_uppercase() == upper {
                let decimals: u8 = r.decimals.to::<u8>();
                return Ok((r.underlying_asset, decimals));
            }
        }
        bail!("Reserve not found for symbol '{}'", symbol);
    }

    /// Get reserve data for a specific symbol.
    pub async fn get_reserve_by_symbol(&self, symbol: &str) -> Result<serde_json::Value> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let ui_addr = Address::from_str(self.config.ui_pool_data_provider)?;
        let pool_addr_provider = Address::from_str(self.config.pool_address_provider)?;

        let (reserves, _) =
            pool_data::get_reserves_data(&provider, ui_addr, pool_addr_provider).await?;

        let upper = symbol.to_uppercase();
        for r in reserves.iter() {
            if r.symbol.to_uppercase() == upper {
                let supply_apy = ray_to_apy(U256::from(r.liquidity_rate));
                let borrow_apy = ray_to_apy(U256::from(r.variable_borrow_rate));

                return Ok(json!({
                    "symbol": r.symbol,
                    "name": r.name,
                    "underlying_asset": format!("{}", r.underlying_asset),
                    "decimals": r.decimals.to_string(),
                    "supply_apy_percent": format!("{:.4}", supply_apy),
                    "variable_borrow_apy_percent": format!("{:.4}", borrow_apy),
                    "total_supplied": format_units(r.available_liquidity + r.total_variable_debt, r.decimals),
                    "total_variable_debt": format_units(r.total_variable_debt, r.decimals),
                    "available_liquidity": format_units(r.available_liquidity, r.decimals),
                    "ltv_percent": format!("{:.2}", r.base_ltv.to::<u64>() as f64 / 100.0),
                    "liquidation_threshold_percent": format!("{:.2}", r.liquidation_threshold.to::<u64>() as f64 / 100.0),
                    "can_be_collateral": r.usage_as_collateral_enabled,
                    "borrowing_enabled": r.borrowing_enabled,
                    "is_frozen": r.is_frozen,
                    "supply_cap": r.supply_cap.to_string(),
                    "borrow_cap": r.borrow_cap.to_string(),
                    "a_token_address": format!("{}", r.a_token_address),
                }));
            }
        }
        bail!("Reserve not found for symbol '{}'", symbol);
    }

    /// Supply an asset to Aave V3.
    pub async fn supply(
        &self,
        asset: Address,
        amount: U256,
        decimals: u8,
    ) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("signer required for supply")?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse()?);

        let pool_addr = Address::from_str(self.config.pool)?;
        let user = signer.address();

        // Check and approve allowance
        let erc20 = IERC20::new(asset, &provider);
        let current_allowance = erc20.allowance(user, pool_addr).call().await?;
        if current_allowance < amount {
            let approve_receipt = erc20
                .approve(pool_addr, amount)
                .send()
                .await?
                .get_receipt()
                .await?;
            if !approve_receipt.status() {
                bail!("ERC20 approve transaction failed");
            }
        }

        // Supply
        let pool = IPool::new(pool_addr, &provider);
        let receipt = pool
            .supply(asset, amount, user, 0)
            .send()
            .await?
            .get_receipt()
            .await?;

        Ok(json!({
            "action": "supply",
            "chain_id": self.config.chain_id,
            "asset": format!("{}", asset),
            "amount": format_units(amount, U256::from(decimals)),
            "tx_hash": format!("{}", receipt.transaction_hash),
            "status": if receipt.status() { "success" } else { "failed" },
            "block_number": receipt.block_number.unwrap_or_default(),
        }))
    }

    /// Withdraw an asset from Aave V3.
    /// If amount is U256::MAX, queries the aToken balance first and withdraws the full amount.
    pub async fn withdraw(
        &self,
        asset: Address,
        amount: U256,
        decimals: u8,
    ) -> Result<serde_json::Value> {
        let signer = self
            .signer
            .as_ref()
            .context("signer required for withdraw")?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse()?);

        let pool_addr = Address::from_str(self.config.pool)?;
        let user = signer.address();

        // For "max" withdrawal, resolve the actual aToken balance to avoid
        // arithmetic overflow in some Pool contract versions.
        let actual_amount = if amount == U256::MAX {
            let a_token = self.find_a_token_address(asset).await?;
            let erc20 = IERC20::new(a_token, &provider);
            let balance = erc20.balanceOf(user).call().await?;
            if balance.is_zero() {
                bail!("No aToken balance to withdraw");
            }
            balance
        } else {
            amount
        };

        let pool = IPool::new(pool_addr, &provider);
        let receipt = pool
            .withdraw(asset, actual_amount, user)
            .send()
            .await?
            .get_receipt()
            .await?;

        Ok(json!({
            "action": "withdraw",
            "chain_id": self.config.chain_id,
            "asset": format!("{}", asset),
            "amount": format_units(actual_amount, U256::from(decimals)),
            "tx_hash": format!("{}", receipt.transaction_hash),
            "status": if receipt.status() { "success" } else { "failed" },
            "block_number": receipt.block_number.unwrap_or_default(),
        }))
    }

    /// Look up the aToken address for a given underlying asset from reserves data.
    async fn find_a_token_address(&self, asset: Address) -> Result<Address> {
        let provider = ProviderBuilder::new().connect_http(self.config.rpc_url.parse()?);
        let ui_addr = Address::from_str(self.config.ui_pool_data_provider)?;
        let pool_addr_provider = Address::from_str(self.config.pool_address_provider)?;
        let (reserves, _) =
            pool_data::get_reserves_data(&provider, ui_addr, pool_addr_provider).await?;
        for r in &reserves {
            if r.underlying_asset == asset {
                return Ok(r.a_token_address);
            }
        }
        bail!("aToken not found for asset {}", asset)
    }

    /// Borrow an asset from Aave V3 (variable rate).
    pub async fn borrow(
        &self,
        asset: Address,
        amount: U256,
        decimals: u8,
    ) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("signer required for borrow")?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse()?);

        let pool_addr = Address::from_str(self.config.pool)?;
        let user = signer.address();

        // interestRateMode = 2 means variable rate
        let pool = IPool::new(pool_addr, &provider);
        let receipt = pool
            .borrow(asset, amount, U256::from(2), 0, user)
            .send()
            .await?
            .get_receipt()
            .await?;

        Ok(json!({
            "action": "borrow",
            "chain_id": self.config.chain_id,
            "asset": format!("{}", asset),
            "amount": format_units(amount, U256::from(decimals)),
            "interest_rate_mode": "variable",
            "tx_hash": format!("{}", receipt.transaction_hash),
            "status": if receipt.status() { "success" } else { "failed" },
            "block_number": receipt.block_number.unwrap_or_default(),
        }))
    }

    /// Repay a borrowed asset to Aave V3 (variable rate).
    pub async fn repay(
        &self,
        asset: Address,
        amount: U256,
        decimals: u8,
    ) -> Result<serde_json::Value> {
        let signer = self.signer.as_ref().context("signer required for repay")?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse()?);

        let pool_addr = Address::from_str(self.config.pool)?;
        let user = signer.address();

        // Approve pool to spend the repay amount
        let erc20 = IERC20::new(asset, &provider);
        let current_allowance = erc20.allowance(user, pool_addr).call().await?;
        if current_allowance < amount {
            let approve_receipt = erc20
                .approve(pool_addr, amount)
                .send()
                .await?
                .get_receipt()
                .await?;
            if !approve_receipt.status() {
                bail!("ERC20 approve transaction failed");
            }
        }

        // interestRateMode = 2 means variable rate
        let pool = IPool::new(pool_addr, &provider);
        let receipt = pool
            .repay(asset, amount, U256::from(2), user)
            .send()
            .await?
            .get_receipt()
            .await?;

        Ok(json!({
            "action": "repay",
            "chain_id": self.config.chain_id,
            "asset": format!("{}", asset),
            "amount": format_units(amount, U256::from(decimals)),
            "interest_rate_mode": "variable",
            "tx_hash": format!("{}", receipt.transaction_hash),
            "status": if receipt.status() { "success" } else { "failed" },
            "block_number": receipt.block_number.unwrap_or_default(),
        }))
    }
}

/// Convert Aave RAY rate (1e27) to compounded APY percentage.
/// RAY rate is the per-second rate. APY = ((1 + rate/sec)^seconds_per_year - 1) * 100
fn ray_to_apy(ray: U256) -> f64 {
    let ray_f = ray.to_string().parse::<f64>().unwrap_or(0.0);
    // Aave liquidityRate / variableBorrowRate are annual rates in RAY (1e27 = 100%).
    // APY = ((1 + annual_rate / seconds_per_year) ^ seconds_per_year - 1) * 100
    let seconds_per_year: f64 = 31_536_000.0;
    let rate_per_second = ray_f / 1e27 / seconds_per_year;
    ((1.0 + rate_per_second).powf(seconds_per_year) - 1.0) * 100.0
}

/// Format U256 with decimals to a human-readable string.
fn format_units(value: U256, decimals: U256) -> String {
    let dec: u32 = decimals.to::<u32>();
    if dec == 0 {
        return value.to_string();
    }
    let divisor = U256::from(10).pow(U256::from(dec));
    let whole = value / divisor;
    let frac = value % divisor;

    let frac_str = format!("{:0>width$}", frac, width = dec as usize);
    let trimmed = frac_str.trim_end_matches('0');
    let trimmed = if trimmed.len() < 2 {
        &frac_str[..2]
    } else {
        trimmed
    };
    format!("{}.{}", whole, trimmed)
}

/// Format base currency value (8 decimals for USD).
fn format_base_currency(value: U256) -> String {
    format_units(value, U256::from(8))
}

/// Format health factor: U256 / 1e18, where max U256 means infinity.
fn format_health_factor(hf: U256) -> String {
    if hf == U256::MAX {
        return "infinity".to_string();
    }
    let hf_f = hf.to_string().parse::<f64>().unwrap_or(0.0);
    format!("{:.4}", hf_f / 1e18)
}

/// Parse a decimal string like "100.5" into U256 given token decimals.
pub fn parse_token_amount(amount_str: &str, decimals: u8) -> Result<U256> {
    let parts: Vec<&str> = amount_str.split('.').collect();
    match parts.len() {
        1 => {
            let whole: U256 = U256::from_str(parts[0]).context("invalid amount")?;
            Ok(whole * U256::from(10).pow(U256::from(decimals)))
        }
        2 => {
            let whole: U256 = U256::from_str(parts[0]).context("invalid whole part")?;
            let frac_str = parts[1];
            let frac_len = frac_str.len();
            if frac_len > decimals as usize {
                bail!("Too many decimal places: {} (max {})", frac_len, decimals);
            }
            let padded = format!("{:0<width$}", frac_str, width = decimals as usize);
            let frac: U256 = U256::from_str(&padded).context("invalid fractional part")?;
            let whole_scaled = whole * U256::from(10).pow(U256::from(decimals));
            Ok(whole_scaled + frac)
        }
        _ => bail!("Invalid amount format: {}", amount_str),
    }
}
