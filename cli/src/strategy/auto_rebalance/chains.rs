//! Chain contract addresses for the auto-rebalance strategy.
//! Supports Base and Ethereum mainnet.

use anyhow::{bail, Result};

pub struct AutoRebalanceConfig {
    pub chain_id: u64,
    pub chain_name: &'static str,
    pub rpc_url: &'static str,
    pub rpc_env_var: &'static str,
    pub usdc: &'static str,
    pub usdc_decimals: u8,
    pub aave_pool: &'static str,
    pub aave_chain_key: &'static str,
    pub compound_comet: &'static str,
    pub morpho_vault: &'static str,
    /// DeFiLlama chain name for fallback queries.
    pub defillama_chain: &'static str,
    /// Gas spike threshold in gwei.
    pub gas_spike_gwei: f64,
}

pub static BASE_CONFIG: AutoRebalanceConfig = AutoRebalanceConfig {
    chain_id: 8453,
    chain_name: "base",
    rpc_url: "https://base-rpc.publicnode.com",
    rpc_env_var: "BASE_RPC_URL",
    usdc: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
    usdc_decimals: 6,
    aave_pool: "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5",
    aave_chain_key: "base",
    compound_comet: "0xb125E6687d4313864e53df431d5425969c15Eb2F",
    morpho_vault: "0xBEEFE94c8aD530842bfE7d8B397938fFc1cb83b2",
    defillama_chain: "Base",
    gas_spike_gwei: 0.5, // Base normal gas ~0.001-0.05 gwei; 0.5 is conservative
};

pub static ETHEREUM_CONFIG: AutoRebalanceConfig = AutoRebalanceConfig {
    chain_id: 1,
    chain_name: "ethereum",
    rpc_url: "https://ethereum-rpc.publicnode.com",
    rpc_env_var: "ETH_RPC_URL",
    usdc: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    usdc_decimals: 6,
    aave_pool: "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2",
    aave_chain_key: "ethereum",
    compound_comet: "0xc3d688B66703497DAA19211EEdff47f25384cdc3",
    morpho_vault: "0xBEEF01735c132Ada46AA9aA4c54623cAA92A64CB",
    defillama_chain: "Ethereum",
    gas_spike_gwei: 50.0, // Ethereum mainnet gas is much higher than Base
};

/// Get config by chain name.
pub fn get_config(chain: &str) -> Result<&'static AutoRebalanceConfig> {
    match chain.to_lowercase().as_str() {
        "base" | "8453" => Ok(&BASE_CONFIG),
        "ethereum" | "eth" | "1" => Ok(&ETHEREUM_CONFIG),
        _ => bail!(
            "Unsupported chain '{}' for auto-rebalance. Supported: base, ethereum",
            chain
        ),
    }
}

/// Get RPC URL for a chain config, allowing override via env var.
pub fn rpc_url_for(config: &AutoRebalanceConfig) -> String {
    std::env::var(config.rpc_env_var).unwrap_or_else(|_| config.rpc_url.to_string())
}

/// Get RPC URL for Base (backward compat).
pub fn rpc_url() -> String {
    rpc_url_for(&BASE_CONFIG)
}
