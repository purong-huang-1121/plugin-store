//! Concurrent APY fetching from Aave V3, Compound V3, and Morpho on Base.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use super::chains::{self, AutoRebalanceConfig};
use crate::dapp::aave::client::AaveClient;
use crate::dapp::compound::CompoundClient;
use crate::dapp::morpho::client::MorphoClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Aave,
    Compound,
    Morpho,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Aave => write!(f, "Aave V3"),
            Protocol::Compound => write!(f, "Compound V3"),
            Protocol::Morpho => write!(f, "Morpho"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct YieldSnapshot {
    pub protocol: Protocol,
    pub apy: f64,
    pub tvl_usd: f64,
    pub source: String,
    /// For Morpho: the best vault address on this chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_address: Option<String>,
}

/// Fetch APY from all 3 protocols concurrently for the given chain.
/// Falls back to DeFiLlama if on-chain query fails.
pub async fn fetch_all_yields_for(
    config: &'static AutoRebalanceConfig,
) -> Result<Vec<YieldSnapshot>> {
    let rpc = chains::rpc_url_for(config);

    let timeout = Duration::from_secs(30);
    let (aave_result, compound_result, morpho_result) = tokio::join!(
        async {
            tokio::time::timeout(timeout, fetch_aave_yield_for(config))
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Aave fetch timed out")))
        },
        async {
            tokio::time::timeout(timeout, fetch_compound_yield_for(config, &rpc))
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Compound fetch timed out")))
        },
        async {
            tokio::time::timeout(timeout, fetch_morpho_yield_for(config))
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Morpho fetch timed out")))
        },
    );

    let mut yields = Vec::with_capacity(3);

    match aave_result {
        Ok(y) => yields.push(y),
        Err(e) => {
            eprintln!("[WARN] Aave on-chain query failed: {e:#}, trying DeFiLlama");
            if let Ok(y) = fetch_defillama_yield(Protocol::Aave, config).await {
                yields.push(y);
            }
        }
    }

    match compound_result {
        Ok(y) => yields.push(y),
        Err(e) => {
            eprintln!("[WARN] Compound on-chain query failed: {e:#}, trying DeFiLlama");
            if let Ok(y) = fetch_defillama_yield(Protocol::Compound, config).await {
                yields.push(y);
            }
        }
    }

    match morpho_result {
        Ok(y) => yields.push(y),
        Err(e) => {
            eprintln!("[WARN] Morpho query failed: {e:#}, trying DeFiLlama");
            if let Ok(y) = fetch_defillama_yield(Protocol::Morpho, config).await {
                yields.push(y);
            }
        }
    }

    if yields.is_empty() {
        anyhow::bail!("Failed to fetch APY from any protocol");
    }

    // Sort by APY descending
    yields.sort_by(|a, b| {
        b.apy
            .partial_cmp(&a.apy)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(yields)
}

/// Fetch APY from all 3 protocols on Base (backward compat).
pub async fn fetch_all_yields() -> Result<Vec<YieldSnapshot>> {
    fetch_all_yields_for(&chains::BASE_CONFIG).await
}

async fn fetch_aave_yield_for(config: &AutoRebalanceConfig) -> Result<YieldSnapshot> {
    let client = AaveClient::new(config.aave_chain_key)?;
    let data = client.get_reserves_data().await?;

    let markets = data["markets"]
        .as_array()
        .context("no markets in Aave response")?;
    for market in markets {
        if market["symbol"].as_str() == Some("USDC") {
            let apy_str = market["supply_apy_percent"].as_str().unwrap_or("0");
            let apy: f64 = apy_str.trim_end_matches('%').parse().unwrap_or(0.0);
            let tvl_str = market["total_supplied"].as_str().unwrap_or("0");
            let tvl: f64 = tvl_str.replace(',', "").parse().unwrap_or(0.0);
            return Ok(YieldSnapshot {
                protocol: Protocol::Aave,
                apy,
                tvl_usd: tvl,
                source: "on-chain".to_string(),
                vault_address: None,
            });
        }
    }
    anyhow::bail!(
        "USDC not found in Aave reserves on {}",
        config.defillama_chain
    )
}

async fn fetch_compound_yield_for(
    config: &AutoRebalanceConfig,
    rpc: &str,
) -> Result<YieldSnapshot> {
    let client = CompoundClient::new(config.compound_comet, config.usdc, rpc)?;

    let apy = client.get_supply_apy().await?;
    let total_supply = client.get_total_supply().await?;
    let tvl_usd = total_supply.to_string().parse::<f64>().unwrap_or(0.0) / 1e6;

    Ok(YieldSnapshot {
        protocol: Protocol::Compound,
        apy,
        tvl_usd,
        source: "on-chain".to_string(),
        vault_address: None,
    })
}

/// Fetch the best Morpho USDC vault on this chain by querying all vaults via GraphQL.
async fn fetch_morpho_yield_for(config: &AutoRebalanceConfig) -> Result<YieldSnapshot> {
    let client = MorphoClient::new()?;

    let query = r#"
        query Vaults($chainId: Int!, $assetAddress: String!) {
            vaults(where: { chainId_in: [$chainId], assetAddress_in: [$assetAddress] }) {
                items {
                    address
                    name
                    asset {
                        symbol
                    }
                    state {
                        apy
                        netApy
                        totalAssetsUsd
                    }
                }
            }
        }
    "#;

    let vars = json!({
        "chainId": config.chain_id,
        "assetAddress": config.usdc,
    });

    let data = client.query(query, vars).await?;
    let vaults = data["vaults"]["items"]
        .as_array()
        .context("no vaults in Morpho response")?;

    // Find the vault with highest net APY and reasonable TVL (> $100k)
    let mut best_apy = 0.0f64;
    let mut best_tvl = 0.0f64;
    let mut best_address: Option<String> = None;

    for vault in vaults {
        let asset_symbol = vault["asset"]["symbol"].as_str().unwrap_or("");
        if asset_symbol != "USDC" {
            continue;
        }
        let tvl = vault["state"]["totalAssetsUsd"].as_f64().unwrap_or(0.0);
        if tvl < 100_000.0 {
            continue; // skip tiny vaults
        }
        let apy = vault["state"]["netApy"]
            .as_f64()
            .or_else(|| vault["state"]["apy"].as_f64())
            .unwrap_or(0.0)
            * 100.0;
        if apy > best_apy {
            best_apy = apy;
            best_tvl = tvl;
            best_address = vault["address"].as_str().map(|s| s.to_string());
        }
    }

    let address = best_address.context("no USDC vaults found on this chain")?;

    Ok(YieldSnapshot {
        protocol: Protocol::Morpho,
        apy: best_apy,
        tvl_usd: best_tvl,
        source: "graphql".to_string(),
        vault_address: Some(address),
    })
}

/// Known DeFiLlama pool IDs for accurate matching (matches TS defillama.ts KNOWN_POOL_IDS).
/// These avoid false matches like SYRUPUSDC, LP tokens, etc.
const KNOWN_POOL_IDS: &[(&str, &str)] = &[
    // Protocol::Aave (aave-v3 USDC on Base)
    ("aave-v3", "7e0661bf-8cf3-45e6-9424-31916d4c7b84"),
    // Protocol::Compound (compound-v3 USDC on Base)
    ("compound-v3", "0c8567f8-ba5b-41ad-80de-00a71895eb19"),
];

/// DeFiLlama fallback for any protocol on the given chain.
/// Matches TS defillama.ts: known pool IDs, apyBase preference, Morpho LP exclusion + TVL filter.
async fn fetch_defillama_yield(
    protocol: Protocol,
    config: &AutoRebalanceConfig,
) -> Result<YieldSnapshot> {
    let http = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let resp = http
        .get("https://yields.llama.fi/pools")
        .send()
        .await
        .context("DeFiLlama request failed")?;
    let body: Value = resp
        .json()
        .await
        .context("failed to parse DeFiLlama response")?;

    let pools = body["data"]
        .as_array()
        .context("no data in DeFiLlama response")?;

    let (project, symbol_match) = match protocol {
        Protocol::Aave => ("aave-v3", "USDC"),
        Protocol::Compound => ("compound-v3", "USDC"),
        Protocol::Morpho => ("morpho-blue", "USDC"),
    };

    let chain_name = config.defillama_chain;

    // Fix #10: Try known pool ID first (most accurate, matches TS KNOWN_POOL_IDS)
    if let Some((_, pool_id)) = KNOWN_POOL_IDS.iter().find(|(proj, _)| *proj == project) {
        if let Some(pool) = pools.iter().find(|p| p["pool"].as_str() == Some(pool_id)) {
            // Fix #8: Prefer apyBase over apy (base APY without reward tokens)
            let apy = pool["apyBase"]
                .as_f64()
                .unwrap_or_else(|| pool["apy"].as_f64().unwrap_or(0.0));
            let tvl = pool["tvlUsd"].as_f64().unwrap_or(0.0);
            return Ok(YieldSnapshot {
                protocol,
                apy,
                tvl_usd: tvl,
                source: "defillama".to_string(),
                vault_address: None,
            });
        }
    }

    // Fix #9: Special handling for Morpho — exclude LP pairs, require >$1M TVL, sort by TVL
    if project == "morpho-blue" {
        let morpho_projects = ["morpho-v1", "morpho-blue"];
        let mut morpho_pools: Vec<&Value> = pools
            .iter()
            .filter(|p| {
                let pool_project = p["project"].as_str().unwrap_or("");
                let pool_chain = p["chain"].as_str().unwrap_or("");
                let pool_symbol = p["symbol"].as_str().unwrap_or("");
                let tvl = p["tvlUsd"].as_f64().unwrap_or(0.0);
                let apy = p["apyBase"]
                    .as_f64()
                    .or_else(|| p["apy"].as_f64())
                    .unwrap_or(0.0);

                morpho_projects.contains(&pool_project)
                    && pool_chain == chain_name
                    && pool_symbol.to_uppercase().contains(symbol_match)
                    && !pool_symbol.contains('-') // Exclude LP pairs (matches TS)
                    && apy > 0.0
                    && tvl > 1_000_000.0 // Min $1M TVL (matches TS)
            })
            .collect();

        // Sort by TVL descending (matches TS: pick highest TVL)
        morpho_pools.sort_by(|a, b| {
            let tvl_a = a["tvlUsd"].as_f64().unwrap_or(0.0);
            let tvl_b = b["tvlUsd"].as_f64().unwrap_or(0.0);
            tvl_b
                .partial_cmp(&tvl_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(best) = morpho_pools.first() {
            let apy = best["apyBase"]
                .as_f64()
                .unwrap_or_else(|| best["apy"].as_f64().unwrap_or(0.0));
            let tvl = best["tvlUsd"].as_f64().unwrap_or(0.0);
            return Ok(YieldSnapshot {
                protocol,
                apy,
                tvl_usd: tvl,
                source: "defillama".to_string(),
                vault_address: None,
            });
        }
    }

    // Generic match fallback
    for pool in pools {
        let pool_project = pool["project"].as_str().unwrap_or("");
        let pool_chain = pool["chain"].as_str().unwrap_or("");
        let pool_symbol = pool["symbol"].as_str().unwrap_or("");

        if pool_project == project
            && pool_chain == chain_name
            && pool_symbol.contains(symbol_match)
            && !pool_symbol.contains('-')
        // Exclude LP pairs
        {
            // Fix #8: Prefer apyBase over apy
            let apy = pool["apyBase"]
                .as_f64()
                .unwrap_or_else(|| pool["apy"].as_f64().unwrap_or(0.0));
            let tvl = pool["tvlUsd"].as_f64().unwrap_or(0.0);
            return Ok(YieldSnapshot {
                protocol,
                apy,
                tvl_usd: tvl,
                source: "defillama".to_string(),
                vault_address: None,
            });
        }
    }

    anyhow::bail!(
        "DeFiLlama: no matching pool found for {} on {}",
        protocol,
        chain_name
    )
}
