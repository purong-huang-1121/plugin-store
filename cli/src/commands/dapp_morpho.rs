use anyhow::Result;
use clap::Subcommand;
use serde_json::{json, Value};

use crate::dapp::morpho::client::MorphoClient;
use crate::output;

// ---------------------------------------------------------------------------
// Chain name → chain ID helper
// ---------------------------------------------------------------------------

/// Resolve a chain name or numeric string to its Morpho chain ID.
fn resolve_chain_id(chain: &str) -> Option<u64> {
    match chain.to_lowercase().as_str() {
        "ethereum" | "eth" | "mainnet" => Some(1),
        "base" => Some(8453),
        "arbitrum" | "arb" => Some(42161),
        "optimism" | "op" => Some(10),
        "polygon" | "matic" => Some(137),
        _ => chain.parse::<u64>().ok(),
    }
}

// ---------------------------------------------------------------------------
// Command definitions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum MorphoCommand {
    /// List Morpho Blue lending markets
    Markets {
        /// Filter by chain (ethereum, base, arbitrum, optimism, polygon, or chain ID)
        #[arg(long)]
        chain: Option<String>,
        /// Max results (1–1000)
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Sort field: SupplyAssetsUsd, BorrowAssetsUsd, Utilization, SupplyApy, BorrowApy
        #[arg(long, default_value = "SupplyAssetsUsd")]
        order_by: String,
        /// Sort direction: Desc or Asc
        #[arg(long, default_value = "Desc")]
        direction: String,
    },

    /// Get details for a specific Morpho Blue market by unique key
    Market {
        /// Market unique key (32-byte hex, e.g. 0xb323...)
        unique_key: String,
        /// Chain ID (default: 1 / Ethereum)
        #[arg(long, default_value = "1")]
        chain_id: u64,
    },

    /// List MetaMorpho vaults
    Vaults {
        /// Filter by chain (ethereum, base, arbitrum, optimism, polygon, or chain ID)
        #[arg(long)]
        chain: Option<String>,
        /// Max results (1–1000)
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Sort field: TotalAssetsUsd, TotalAssets, Apy, NetApy, Name
        #[arg(long, default_value = "TotalAssetsUsd")]
        order_by: String,
        /// Sort direction: Desc or Asc
        #[arg(long, default_value = "Desc")]
        direction: String,
    },

    /// Get details for a specific MetaMorpho vault by address
    Vault {
        /// Vault contract address
        address: String,
        /// Chain ID (default: 1 / Ethereum)
        #[arg(long, default_value = "1")]
        chain_id: u64,
    },

    /// Get supply/borrow positions for a wallet address
    Positions {
        /// Wallet address to query
        address: String,
        /// Filter by chain (ethereum, base, arbitrum, optimism, polygon, or chain ID)
        #[arg(long)]
        chain: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

pub async fn execute(cmd: MorphoCommand) -> Result<()> {
    match cmd {
        MorphoCommand::Markets {
            chain,
            limit,
            order_by,
            direction,
        } => markets(chain, limit, &order_by, &direction).await,
        MorphoCommand::Market {
            unique_key,
            chain_id,
        } => market(&unique_key, chain_id).await,
        MorphoCommand::Vaults {
            chain,
            limit,
            order_by,
            direction,
        } => vaults(chain, limit, &order_by, &direction).await,
        MorphoCommand::Vault { address, chain_id } => vault(&address, chain_id).await,
        MorphoCommand::Positions { address, chain } => positions(&address, chain).await,
    }
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

async fn markets(chain: Option<String>, limit: u32, order_by: &str, direction: &str) -> Result<()> {
    let client = MorphoClient::new()?;

    let chain_ids: Value = match chain.as_deref() {
        Some(c) => {
            let id = resolve_chain_id(c).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown chain: '{}'. Use ethereum, base, arbitrum, optimism, polygon, or a numeric chain ID.",
                    c
                )
            })?;
            json!([id])
        }
        None => Value::Null,
    };

    // Market type exposes chain info via morphoBlue { chain { ... } }
    let query = r#"
        query Markets($first: Int, $chainId: [Int!], $orderBy: MarketOrderBy, $orderDirection: OrderDirection) {
            markets(
                first: $first,
                where: { chainId_in: $chainId },
                orderBy: $orderBy,
                orderDirection: $orderDirection
            ) {
                items {
                    uniqueKey
                    lltv
                    oracleAddress
                    irmAddress
                    loanAsset { address symbol name decimals }
                    collateralAsset { address symbol name decimals }
                    state {
                        supplyAssets
                        supplyAssetsUsd
                        borrowAssets
                        borrowAssetsUsd
                        utilization
                        supplyApy
                        borrowApy
                        avgSupplyApy
                        avgBorrowApy
                    }
                    morphoBlue { chain { id network } }
                }
            }
        }
    "#;

    let mut vars = json!({
        "first": limit,
        "orderBy": order_by,
        "orderDirection": direction,
    });
    if !chain_ids.is_null() {
        vars["chainId"] = chain_ids;
    }

    let data = client.query(query, vars).await?;
    let items = data["markets"]["items"].clone();
    output::success(items);
    Ok(())
}

async fn market(unique_key: &str, chain_id: u64) -> Result<()> {
    let client = MorphoClient::new()?;

    let query = r#"
        query Market($uniqueKey: String!, $chainId: Int!) {
            marketByUniqueKey(uniqueKey: $uniqueKey, chainId: $chainId) {
                uniqueKey
                lltv
                oracleAddress
                irmAddress
                loanAsset { address symbol name decimals }
                collateralAsset { address symbol name decimals }
                state {
                    supplyAssets
                    supplyAssetsUsd
                    borrowAssets
                    borrowAssetsUsd
                    collateralAssets
                    collateralAssetsUsd
                    utilization
                    supplyApy
                    borrowApy
                    avgSupplyApy
                    avgBorrowApy
                    rewards {
                        asset { symbol }
                        supplyApr
                        borrowApr
                    }
                }
                morphoBlue { chain { id network } }
            }
        }
    "#;

    let vars = json!({
        "uniqueKey": unique_key,
        "chainId": chain_id,
    });

    let data = client.query(query, vars).await?;
    output::success(data["marketByUniqueKey"].clone());
    Ok(())
}

async fn vaults(chain: Option<String>, limit: u32, order_by: &str, direction: &str) -> Result<()> {
    let client = MorphoClient::new()?;

    let chain_ids: Value = match chain.as_deref() {
        Some(c) => {
            let id = resolve_chain_id(c).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown chain: '{}'. Use ethereum, base, arbitrum, optimism, polygon, or a numeric chain ID.",
                    c
                )
            })?;
            json!([id])
        }
        None => Value::Null,
    };

    // Vault type has a direct chain field
    let query = r#"
        query Vaults($first: Int, $chainId: [Int!], $orderBy: VaultOrderBy, $orderDirection: OrderDirection) {
            vaults(
                first: $first,
                where: { chainId_in: $chainId },
                orderBy: $orderBy,
                orderDirection: $orderDirection
            ) {
                items {
                    address
                    name
                    symbol
                    asset { address symbol name decimals }
                    state {
                        totalAssetsUsd
                        totalAssets
                        apy
                        netApy
                        fee
                    }
                    chain { id network }
                }
            }
        }
    "#;

    let mut vars = json!({
        "first": limit,
        "orderBy": order_by,
        "orderDirection": direction,
    });
    if !chain_ids.is_null() {
        vars["chainId"] = chain_ids;
    }

    let data = client.query(query, vars).await?;
    let items = data["vaults"]["items"].clone();
    output::success(items);
    Ok(())
}

async fn vault(address: &str, chain_id: u64) -> Result<()> {
    let client = MorphoClient::new()?;

    let query = r#"
        query Vault($address: String!, $chainId: Int!) {
            vaultByAddress(address: $address, chainId: $chainId) {
                address
                name
                symbol
                asset { address symbol name decimals }
                state {
                    totalAssetsUsd
                    totalAssets
                    apy
                    netApy
                    fee
                }
                chain { id network }
                metadata { description forumLink }
            }
        }
    "#;

    let vars = json!({
        "address": address,
        "chainId": chain_id,
    });

    let data = client.query(query, vars).await?;
    output::success(data["vaultByAddress"].clone());
    Ok(())
}

async fn positions(address: &str, chain: Option<String>) -> Result<()> {
    let client = MorphoClient::new()?;

    let chain_id: Option<u64> = match chain.as_deref() {
        Some(c) => Some(resolve_chain_id(c).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown chain: '{}'. Use ethereum, base, arbitrum, optimism, polygon, or a numeric chain ID.",
                c
            )
        })?),
        None => None,
    };

    // Market has no direct chain field; chain info comes via morphoBlue { chain { ... } }
    let query = r#"
        query Positions($address: String!, $chainId: Int) {
            userByAddress(address: $address, chainId: $chainId) {
                address
                marketPositions {
                    market {
                        uniqueKey
                        loanAsset { symbol }
                        collateralAsset { symbol }
                        morphoBlue { chain { network } }
                    }
                    state {
                        collateral
                        collateralUsd
                        borrowAssets
                        borrowAssetsUsd
                        supplyAssets
                        supplyAssetsUsd
                    }
                }
                vaultPositions {
                    vault {
                        address
                        name
                        symbol
                        chain { network }
                    }
                    state {
                        assets
                        assetsUsd
                    }
                }
            }
        }
    "#;

    let mut vars = json!({ "address": address });
    if let Some(id) = chain_id {
        vars["chainId"] = json!(id);
    }

    let data = client.query(query, vars).await?;
    output::success(data["userByAddress"].clone());
    Ok(())
}
