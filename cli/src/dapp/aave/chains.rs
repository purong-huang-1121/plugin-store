//! Aave V3 chain configuration: RPC URLs, contract addresses.

use anyhow::{bail, Result};

pub struct ChainConfig {
    pub chain_id: u64,
    pub rpc_url: &'static str,
    pub pool: &'static str,
    pub ui_pool_data_provider: &'static str,
    pub pool_address_provider: &'static str,
}

pub fn get_chain_config(chain: &str) -> Result<&'static ChainConfig> {
    match chain.to_lowercase().as_str() {
        "ethereum" | "eth" | "1" => Ok(&ETHEREUM),
        "polygon" | "matic" | "137" => Ok(&POLYGON),
        "arbitrum" | "arb" | "42161" => Ok(&ARBITRUM),
        "base" | "8453" => Ok(&BASE),
        _ => bail!(
            "Unsupported chain '{}'. Supported: ethereum, polygon, arbitrum, base",
            chain
        ),
    }
}

pub fn default_chain() -> &'static str {
    "ethereum"
}

static ETHEREUM: ChainConfig = ChainConfig {
    chain_id: 1,
    rpc_url: "https://ethereum-rpc.publicnode.com",
    pool: "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2",
    ui_pool_data_provider: "0x56b7A1012765C285afAC8b8F25C69Bf10ccfE978",
    pool_address_provider: "0x2f39d218133AFaB8F2B819B1066c7E434Ad94E9e",
};

static POLYGON: ChainConfig = ChainConfig {
    chain_id: 137,
    rpc_url: "https://polygon-bor-rpc.publicnode.com",
    pool: "0x794a61358D6845594F94dc1DB02A252b5b4814aD",
    ui_pool_data_provider: "0xFa1A7c4a8A63C9CAb150529c26f182cBB5500944",
    pool_address_provider: "0xa97684ead0e402dC232d5A977953DF7ECBaB3CDb",
};

static ARBITRUM: ChainConfig = ChainConfig {
    chain_id: 42161,
    rpc_url: "https://arbitrum-one-rpc.publicnode.com",
    pool: "0x794a61358D6845594F94dc1DB02A252b5b4814aD",
    ui_pool_data_provider: "0x13c833256BD767da2320d727a3691BAff3770E39",
    pool_address_provider: "0xa97684ead0e402dC232d5A977953DF7ECBaB3CDb",
};

static BASE: ChainConfig = ChainConfig {
    chain_id: 8453,
    rpc_url: "https://mainnet.base.org",
    pool: "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5",
    ui_pool_data_provider: "0xb84A20e848baE3e13897934bB4e74E2225f4546B",
    pool_address_provider: "0xe20fCBdBfFC4Dd138cE8b2E6FBb6CB49777ad64D",
};
