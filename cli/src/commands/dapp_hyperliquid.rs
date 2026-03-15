use anyhow::{bail, Context as _, Result};
use clap::Subcommand;
use serde_json::{json, Value};

use crate::dapp::hyperliquid::client::HyperliquidClient;
use crate::output;

#[derive(Subcommand)]
pub enum HyperliquidCommand {
    /// List perpetual futures markets
    Markets,
    /// List spot markets
    SpotMarkets,
    /// Get real-time mid price for a symbol
    Price { symbol: String },
    /// View L2 order book
    Orderbook { symbol: String },
    /// View funding rate (current and historical)
    Funding { symbol: String },
    /// Place a buy/long order
    Buy {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        size: String,
        #[arg(long)]
        price: String,
        #[arg(long)]
        leverage: Option<u32>,
    },
    /// Place a sell/short order
    Sell {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        size: String,
        #[arg(long)]
        price: String,
    },
    /// Cancel an open order
    Cancel {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        order_id: u64,
    },
    /// View perpetual positions
    Positions,
    /// View account balances (USDC margin + spot)
    Balances,
    /// List open orders
    Orders {
        #[arg(long)]
        symbol: Option<String>,
    },
}

pub async fn execute(cmd: HyperliquidCommand) -> Result<()> {
    match cmd {
        HyperliquidCommand::Markets => cmd_markets().await,
        HyperliquidCommand::SpotMarkets => cmd_spot_markets().await,
        HyperliquidCommand::Price { symbol } => cmd_price(&symbol).await,
        HyperliquidCommand::Orderbook { symbol } => cmd_orderbook(&symbol).await,
        HyperliquidCommand::Funding { symbol } => cmd_funding(&symbol).await,
        HyperliquidCommand::Buy {
            symbol,
            size,
            price,
            leverage,
        } => cmd_buy(&symbol, &size, &price, leverage).await,
        HyperliquidCommand::Sell {
            symbol,
            size,
            price,
        } => cmd_sell(&symbol, &size, &price).await,
        HyperliquidCommand::Cancel { symbol, order_id } => cmd_cancel(&symbol, order_id).await,
        HyperliquidCommand::Positions => cmd_positions().await,
        HyperliquidCommand::Balances => cmd_balances().await,
        HyperliquidCommand::Orders { symbol } => cmd_orders(symbol).await,
    }
}

// ---------------------------------------------------------------------------
// Data commands (read-only)
// ---------------------------------------------------------------------------

async fn cmd_markets() -> Result<()> {
    let client = HyperliquidClient::new()?;

    let meta = client.info(json!({"type": "meta"})).await?;
    let mids = client.info(json!({"type": "allMids"})).await?;

    let universe = meta
        .get("universe")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut markets: Vec<Value> = Vec::new();
    for asset in &universe {
        let symbol = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let mid_price = mids.get(symbol).and_then(|v| v.as_str()).unwrap_or("0");
        markets.push(json!({
            "symbol": symbol,
            "mid_price": mid_price,
            "szDecimals": asset.get("szDecimals"),
            "maxLeverage": asset.get("maxLeverage"),
        }));
    }

    output::success(json!({ "markets": markets }));
    Ok(())
}

async fn cmd_spot_markets() -> Result<()> {
    let client = HyperliquidClient::new()?;
    let data = client.info(json!({"type": "spotMeta"})).await?;

    let universe = data
        .get("universe")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let tokens = data
        .get("tokens")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut markets: Vec<Value> = Vec::new();
    for (i, pair) in universe.iter().enumerate() {
        let name = pair.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let base_idx = pair
            .get("tokens")
            .and_then(|v| v.get(0))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let quote_idx = pair
            .get("tokens")
            .and_then(|v| v.get(1))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let base = tokens
            .get(base_idx)
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let quote = tokens
            .get(quote_idx)
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        markets.push(json!({
            "name": name,
            "base": base,
            "quote": quote,
            "index": i,
        }));
    }

    output::success(json!({ "markets": markets }));
    Ok(())
}

async fn cmd_price(symbol: &str) -> Result<()> {
    let client = HyperliquidClient::new()?;
    let mids = client.info(json!({"type": "allMids"})).await?;

    let mid_price = mids
        .get(symbol)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("symbol '{}' not found in allMids", symbol))?;

    output::success(json!({
        "symbol": symbol,
        "mid_price": mid_price,
    }));
    Ok(())
}

async fn cmd_orderbook(symbol: &str) -> Result<()> {
    let client = HyperliquidClient::new()?;
    let data = client
        .info(json!({"type": "l2Book", "coin": symbol}))
        .await?;
    output::success(data);
    Ok(())
}

async fn cmd_funding(symbol: &str) -> Result<()> {
    let client = HyperliquidClient::new()?;

    let meta = client.info(json!({"type": "meta"})).await?;
    let universe = meta
        .get("universe")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let current_funding = universe
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some(symbol))
        .and_then(|a| a.get("funding").cloned())
        .unwrap_or(Value::Null);

    let day_ago = chrono::Utc::now().timestamp_millis() - 86_400_000;
    let history = client
        .info(json!({
            "type": "fundingHistory",
            "coin": symbol,
            "startTime": day_ago,
        }))
        .await?;

    output::success(json!({
        "symbol": symbol,
        "current_funding": current_funding,
        "history_24h": history,
    }));
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalize a decimal string by removing trailing zeros after the decimal point.
/// Hyperliquid server normalizes price/size strings before hashing, so our
/// signed action must use the same normalized form.
/// Examples: "0.170" → "0.17", "58.00" → "58", "100" → "100"
fn normalize_decimal(s: &str) -> String {
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        if trimmed.is_empty() {
            "0".to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        s.to_string()
    }
}

async fn resolve_asset_index(client: &HyperliquidClient, symbol: &str) -> Result<u32> {
    let meta = client.info(json!({"type": "meta"})).await?;
    let universe = meta["universe"]
        .as_array()
        .context("failed to get universe from meta")?;
    for (i, asset) in universe.iter().enumerate() {
        if asset["name"].as_str() == Some(symbol) {
            return Ok(i as u32);
        }
    }
    bail!(
        "Symbol '{}' not found in Hyperliquid markets. Use 'hyperliquid markets' to see available symbols.",
        symbol
    )
}

// ---------------------------------------------------------------------------
// Trading commands
// ---------------------------------------------------------------------------

async fn cmd_buy(symbol: &str, size: &str, price: &str, leverage: Option<u32>) -> Result<()> {
    let client = HyperliquidClient::new_with_signer()?;
    let asset_index = resolve_asset_index(&client, symbol).await?;

    // Set leverage if requested
    if let Some(lev) = leverage {
        let nonce = chrono::Utc::now().timestamp_millis() as u64;
        client
            .exchange(
                json!({
                    "type": "updateLeverage",
                    "asset": asset_index,
                    "isCross": true,
                    "leverage": lev,
                }),
                nonce,
                None,
            )
            .await?;
    }

    let nonce = chrono::Utc::now().timestamp_millis() as u64;
    let norm_price = normalize_decimal(price);
    let norm_size = normalize_decimal(size);
    let result = client
        .exchange(
            json!({
                "type": "order",
                "orders": [{
                    "a": asset_index,
                    "b": true,
                    "p": norm_price,
                    "s": norm_size,
                    "r": false,
                    "t": {"limit": {"tif": "Gtc"}}
                }],
                "grouping": "na"
            }),
            nonce,
            None,
        )
        .await?;

    output::success(json!({
        "action": "buy",
        "symbol": symbol,
        "size": size,
        "price": price,
        "leverage": leverage,
        "result": result,
    }));
    Ok(())
}

async fn cmd_sell(symbol: &str, size: &str, price: &str) -> Result<()> {
    let client = HyperliquidClient::new_with_signer()?;
    let asset_index = resolve_asset_index(&client, symbol).await?;

    let nonce = chrono::Utc::now().timestamp_millis() as u64;
    let norm_price = normalize_decimal(price);
    let norm_size = normalize_decimal(size);
    let result = client
        .exchange(
            json!({
                "type": "order",
                "orders": [{
                    "a": asset_index,
                    "b": false,
                    "p": norm_price,
                    "s": norm_size,
                    "r": false,
                    "t": {"limit": {"tif": "Gtc"}}
                }],
                "grouping": "na"
            }),
            nonce,
            None,
        )
        .await?;

    output::success(json!({
        "action": "sell",
        "symbol": symbol,
        "size": size,
        "price": price,
        "result": result,
    }));
    Ok(())
}

async fn cmd_cancel(symbol: &str, order_id: u64) -> Result<()> {
    let client = HyperliquidClient::new_with_signer()?;
    let asset_index = resolve_asset_index(&client, symbol).await?;

    let nonce = chrono::Utc::now().timestamp_millis() as u64;
    let result = client
        .exchange(
            json!({
                "type": "cancel",
                "cancels": [{
                    "a": asset_index,
                    "o": order_id
                }]
            }),
            nonce,
            None,
        )
        .await?;

    output::success(json!({
        "action": "cancel",
        "symbol": symbol,
        "order_id": order_id,
        "result": result,
    }));
    Ok(())
}

async fn cmd_positions() -> Result<()> {
    let client = HyperliquidClient::new_with_signer()?;
    let addr = client.address()?;
    let data = client
        .info(json!({
            "type": "clearinghouseState",
            "user": addr
        }))
        .await?;

    let positions = &data["assetPositions"];
    let margin_summary = &data["marginSummary"];

    output::success(json!({
        "positions": positions,
        "margin_summary": margin_summary,
        "cross_margin_summary": &data["crossMarginSummary"],
    }));
    Ok(())
}

async fn cmd_balances() -> Result<()> {
    let client = HyperliquidClient::new_with_signer()?;
    let addr = client.address()?;

    let perps = client
        .info(json!({
            "type": "clearinghouseState",
            "user": addr
        }))
        .await?;

    let spot = client
        .info(json!({
            "type": "spotClearinghouseState",
            "user": addr
        }))
        .await?;

    output::success(json!({
        "perps_margin": perps.get("marginSummary"),
        "spot_balances": spot.get("balances"),
    }));
    Ok(())
}

async fn cmd_orders(symbol: Option<String>) -> Result<()> {
    let client = HyperliquidClient::new_with_signer()?;
    let addr = client.address()?;
    let data = client
        .info(json!({
            "type": "openOrders",
            "user": addr
        }))
        .await?;

    let orders = if let Some(ref sym) = symbol {
        let filtered: Vec<&Value> = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|o| o["coin"].as_str() == Some(sym))
                    .collect()
            })
            .unwrap_or_default();
        json!(filtered)
    } else {
        data
    };

    output::success(json!({ "orders": orders }));
    Ok(())
}
