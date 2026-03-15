use anyhow::{bail, Context, Result};
use clap::Subcommand;

use crate::dapp::polymarket::clob_client::ClobClient;
use crate::dapp::polymarket::gamma_client::GammaClient;
use crate::output;

#[derive(Subcommand)]
pub enum PolymarketCommand {
    /// Search prediction markets
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// List popular/active markets
    Markets {
        /// Filter by tag (e.g. politics, crypto, sports)
        #[arg(long)]
        tag: Option<String>,
        /// Sort: volume, liquidity, newest, ending
        #[arg(long, default_value = "volume")]
        sort: String,
        /// Max results
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Get event details with related markets
    Event {
        /// Event ID or slug
        event_id: String,
    },
    /// Get outcome token price (Yes/No)
    Price {
        /// Outcome token ID
        token_id: String,
    },
    /// View orderbook depth
    Book {
        /// Outcome token ID
        token_id: String,
    },
    /// Price history
    History {
        /// Outcome token ID
        token_id: String,
        /// Interval: 1m, 1h, 6h, 1d, 1w, max
        #[arg(long, default_value = "1d")]
        interval: String,
    },
    /// Buy outcome shares
    Buy {
        /// Outcome token ID
        #[arg(long)]
        token: String,
        /// Amount in USDC
        #[arg(long)]
        amount: String,
        /// Limit price (0-1)
        #[arg(long)]
        price: String,
        /// Order type: GTC, FOK, GTD
        #[arg(long, default_value = "GTC")]
        order_type: String,
    },
    /// Sell outcome shares
    Sell {
        /// Outcome token ID
        #[arg(long)]
        token: String,
        /// Number of shares to sell
        #[arg(long)]
        amount: String,
        /// Limit price (0-1)
        #[arg(long)]
        price: String,
        /// Order type: GTC, FOK, GTD
        #[arg(long, default_value = "GTC")]
        order_type: String,
    },
    /// Cancel an order
    Cancel {
        /// Order ID
        order_id: String,
    },
    /// View open orders
    Orders {
        /// Filter by market condition_id
        #[arg(long)]
        market: Option<String>,
    },
    /// View current positions
    Positions,
    /// View USDC balance
    Balance,
}

pub async fn execute(cmd: PolymarketCommand) -> Result<()> {
    match cmd {
        PolymarketCommand::Search { query, limit } => search(&query, limit).await,
        PolymarketCommand::Markets { tag, sort, limit } => markets(tag, &sort, limit).await,
        PolymarketCommand::Event { event_id } => event(&event_id).await,
        PolymarketCommand::Price { token_id } => price(&token_id).await,
        PolymarketCommand::Book { token_id } => book(&token_id).await,
        PolymarketCommand::History { token_id, interval } => history(&token_id, &interval).await,
        PolymarketCommand::Buy {
            token,
            amount,
            price,
            order_type,
        } => buy(&token, &amount, &price, &order_type).await,
        PolymarketCommand::Sell {
            token,
            amount,
            price,
            order_type,
        } => sell(&token, &amount, &price, &order_type).await,
        PolymarketCommand::Cancel { order_id } => cancel(&order_id).await,
        PolymarketCommand::Orders { market } => orders(market).await,
        PolymarketCommand::Positions => positions().await,
        PolymarketCommand::Balance => balance().await,
    }
}

async fn search(query: &str, limit: u32) -> Result<()> {
    let client = GammaClient::new()?;
    let limit_str = limit.to_string();
    let data = client
        .get(
            "/markets",
            &[
                ("_q", query),
                ("_limit", &limit_str),
                ("active", "true"),
                ("closed", "false"),
            ],
        )
        .await?;
    // Gamma API may ignore _limit on search; truncate client-side to enforce it.
    let data = match data {
        serde_json::Value::Array(mut arr) => {
            arr.truncate(limit as usize);
            serde_json::Value::Array(arr)
        }
        other => other,
    };
    output::success(data);
    Ok(())
}

async fn markets(tag: Option<String>, sort: &str, limit: u32) -> Result<()> {
    let client = GammaClient::new()?;
    let limit_str = limit.to_string();
    let tag_str = tag.unwrap_or_default();
    let order = match sort {
        "newest" => "startDate",
        "ending" => "endDate",
        "liquidity" => "liquidityNum",
        _ => "volumeNum",
    };
    let data = client
        .get(
            "/markets",
            &[
                ("tag", &tag_str),
                ("_limit", &limit_str),
                ("active", "true"),
                ("closed", "false"),
                ("order", order),
                ("ascending", "false"),
            ],
        )
        .await?;
    output::success(data);
    Ok(())
}

async fn event(event_id: &str) -> Result<()> {
    let client = GammaClient::new()?;
    let path = format!("/events/{}", event_id);
    let data = client.get(&path, &[]).await?;
    output::success(data);
    Ok(())
}

async fn price(token_id: &str) -> Result<()> {
    let client = ClobClient::new()?;
    let buy = client
        .get("/price", &[("token_id", token_id), ("side", "buy")])
        .await?;
    let sell = client
        .get("/price", &[("token_id", token_id), ("side", "sell")])
        .await?;
    let mid = client.get("/midpoint", &[("token_id", token_id)]).await?;
    let spread = client.get("/spread", &[("token_id", token_id)]).await?;

    let result = serde_json::json!({
        "token_id": token_id,
        "buy": buy,
        "sell": sell,
        "midpoint": mid,
        "spread": spread,
    });
    output::success(result);
    Ok(())
}

async fn book(token_id: &str) -> Result<()> {
    let client = ClobClient::new()?;
    let data = client.get("/book", &[("token_id", token_id)]).await?;
    output::success(data);
    Ok(())
}

async fn history(token_id: &str, interval: &str) -> Result<()> {
    let client = ClobClient::new()?;
    let data = client
        .get(
            "/prices-history",
            &[("market", token_id), ("interval", interval)],
        )
        .await?;
    output::success(data);
    Ok(())
}

async fn buy(token_id: &str, amount: &str, price: &str, order_type: &str) -> Result<()> {
    let client = ClobClient::new_authenticated().await?;
    let tick_size = client.get("/tick-size", &[("token_id", token_id)]).await?;
    let tick = tick_size.as_str().unwrap_or("0.01");

    let amount_f: f64 = amount.parse().context("invalid amount")?;
    let price_f: f64 = price.parse().context("invalid price")?;
    if price_f <= 0.0 || price_f >= 1.0 {
        bail!("price must be between 0 and 1 (exclusive)");
    }
    let size = amount_f / price_f;

    let tick_f: f64 = tick.parse().unwrap_or(0.01);
    let rounded_price = (price_f / tick_f).round() * tick_f;

    let decimals = if tick.contains('.') {
        tick.split('.').nth(1).map_or(1, |d| d.len())
    } else {
        1
    };

    let order = serde_json::json!({
        "tokenID": token_id,
        "price": format!("{:.prec$}", rounded_price, prec = decimals),
        "size": format!("{:.2}", size),
        "side": "BUY",
        "type": order_type,
    });

    let data = client.auth_post("/order", &order).await?;
    output::success(data);
    Ok(())
}

async fn sell(token_id: &str, amount: &str, price: &str, order_type: &str) -> Result<()> {
    let client = ClobClient::new_authenticated().await?;
    let tick_size = client.get("/tick-size", &[("token_id", token_id)]).await?;
    let tick = tick_size.as_str().unwrap_or("0.01");

    let price_f: f64 = price.parse().context("invalid price")?;
    if price_f <= 0.0 || price_f >= 1.0 {
        bail!("price must be between 0 and 1 (exclusive)");
    }

    let tick_f: f64 = tick.parse().unwrap_or(0.01);
    let rounded_price = (price_f / tick_f).round() * tick_f;

    let decimals = if tick.contains('.') {
        tick.split('.').nth(1).map_or(1, |d| d.len())
    } else {
        1
    };

    let order = serde_json::json!({
        "tokenID": token_id,
        "price": format!("{:.prec$}", rounded_price, prec = decimals),
        "size": amount,
        "side": "SELL",
        "type": order_type,
    });

    let data = client.auth_post("/order", &order).await?;
    output::success(data);
    Ok(())
}

async fn cancel(order_id: &str) -> Result<()> {
    let client = ClobClient::new_authenticated().await?;
    let path = format!("/order/{}", order_id);
    let data = client.auth_delete(&path).await?;
    output::success(data);
    Ok(())
}

async fn orders(market: Option<String>) -> Result<()> {
    let client = ClobClient::new_authenticated().await?;
    let market_str = market.unwrap_or_default();
    let data = client
        .auth_get("/data/orders", &[("market", &market_str)])
        .await?;
    output::success(data);
    Ok(())
}

async fn positions() -> Result<()> {
    let client = ClobClient::new_authenticated().await?;
    let address = client.address().unwrap_or_default().to_string();
    let gamma = GammaClient::new()?;
    let data = gamma.get("/positions", &[("user", &address)]).await?;
    output::success(data);
    Ok(())
}

async fn balance() -> Result<()> {
    let client = ClobClient::new_authenticated().await?;
    let data = client
        .auth_get("/balance-allowance", &[("asset_type", "COLLATERAL")])
        .await?;
    output::success(data);
    Ok(())
}
