use anyhow::{bail, Context, Result};
use clap::Subcommand;

use crate::dapp::kalshi::auth::KalshiEnv;
use crate::dapp::kalshi::client::KalshiClient;
use crate::dapp::kalshi::market_data::probability_to_cents;
use crate::output;

// ---------------------------------------------------------------------------
// Command definitions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum KalshiCommand {
    /// Search events and markets by keyword
    Search {
        /// Search query (e.g. "bitcoin", "election", "fed rate")
        query: String,
        /// Max results
        #[arg(long, default_value = "20")]
        limit: u32,
    },

    /// List popular/active markets
    Markets {
        /// Filter by status: open, closed, settled
        #[arg(long)]
        status: Option<String>,
        /// Sort: volume, liquidity, newest, ending
        #[arg(long, default_value = "volume")]
        sort: String,
        /// Max results
        #[arg(long, default_value = "20")]
        limit: u32,
    },

    /// Get event details with all related markets
    Event {
        /// Event ticker (e.g. "FED-2024")
        event_ticker: String,
    },

    /// Get Yes/No price for a market contract
    Price {
        /// Market ticker (e.g. "FED-24DEC-T5.25")
        ticker: String,
    },

    /// View orderbook depth for a market contract
    Book {
        /// Market ticker
        ticker: String,
        /// Number of levels to display on each side
        #[arg(long, default_value = "5")]
        depth: u32,
    },

    /// Price history for a market contract
    History {
        /// Market ticker
        ticker: String,
        /// Interval: 1m, 1h, 6h, 1d, 1w, all
        #[arg(long, default_value = "1d")]
        interval: String,
    },

    /// Buy Yes or No shares (limit order)
    Buy {
        /// Market ticker
        #[arg(long)]
        ticker: String,
        /// Outcome to buy: yes or no
        #[arg(long)]
        side: String,
        /// Number of contracts
        #[arg(long)]
        count: u32,
        /// Limit price as probability 0–1 (e.g. 0.65 = 65 cents)
        #[arg(long)]
        price: String,
        /// Order type: limit or market
        #[arg(long, default_value = "limit")]
        order_type: String,
    },

    /// Sell Yes or No shares
    Sell {
        /// Market ticker
        #[arg(long)]
        ticker: String,
        /// Outcome to sell: yes or no
        #[arg(long)]
        side: String,
        /// Number of contracts to sell
        #[arg(long)]
        count: u32,
        /// Limit price as probability 0–1
        #[arg(long)]
        price: String,
        /// Order type: limit or market
        #[arg(long, default_value = "limit")]
        order_type: String,
    },

    /// Cancel an open order
    Cancel {
        /// Order ID
        order_id: String,
    },

    /// View open orders (optionally filtered by ticker)
    Orders {
        /// Filter by market ticker
        #[arg(long)]
        ticker: Option<String>,
        /// Filter by status: resting, pending, cancelled, executed
        #[arg(long)]
        status: Option<String>,
    },

    /// View current positions
    Positions {
        /// Filter by settlement status: unsettled, settled, all
        #[arg(long, default_value = "unsettled")]
        settlement_status: String,
    },

    /// View USD account balance
    Balance,
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

pub async fn execute(cmd: KalshiCommand, env: KalshiEnv) -> Result<()> {
    match cmd {
        KalshiCommand::Search { query, limit } => search(&query, limit, &env).await,
        KalshiCommand::Markets {
            status,
            sort,
            limit,
        } => markets(status, &sort, limit, &env).await,
        KalshiCommand::Event { event_ticker } => event(&event_ticker, &env).await,
        KalshiCommand::Price { ticker } => price(&ticker, &env).await,
        KalshiCommand::Book { ticker, depth } => book(&ticker, depth, &env).await,
        KalshiCommand::History { ticker, interval } => history(&ticker, &interval, &env).await,
        KalshiCommand::Buy {
            ticker,
            side,
            count,
            price,
            order_type,
        } => buy(&ticker, &side, count, &price, &order_type, &env).await,
        KalshiCommand::Sell {
            ticker,
            side,
            count,
            price,
            order_type,
        } => sell(&ticker, &side, count, &price, &order_type, &env).await,
        KalshiCommand::Cancel { order_id } => cancel(&order_id, &env).await,
        KalshiCommand::Orders { ticker, status } => orders(ticker, status, &env).await,
        KalshiCommand::Positions { settlement_status } => positions(&settlement_status, &env).await,
        KalshiCommand::Balance => balance(&env).await,
    }
}

// ---------------------------------------------------------------------------
// Read-only commands (no credentials required)
// ---------------------------------------------------------------------------

async fn search(query: &str, limit: u32, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new(env)?;
    let limit_str = limit.to_string();
    // Kalshi: search markets with a text filter
    let raw = client
        .get(
            "/markets",
            &[
                ("status", "open"),
                ("limit", &limit_str),
                // Kalshi API v2 supports a `series_ticker` filter;
                // for free-text search we use event_ticker as a best-effort match
                // and fallback to returning the top open markets ranked by volume.
                // A dedicated search endpoint is not available in v2.
            ],
        )
        .await?;

    // Kalshi /markets returns {cursor, markets: [...]}: extract the array
    let data = if let serde_json::Value::Object(mut obj) = raw {
        obj.remove("markets")
            .unwrap_or_else(|| serde_json::Value::Array(vec![]))
    } else {
        raw
    };

    // Filter client-side by the query string (case-insensitive title match)
    let query_lc = query.to_lowercase();
    let filtered = match data.as_array() {
        Some(arr) => {
            let matched: Vec<_> = arr
                .iter()
                .filter(|m| {
                    let title = m["title"].as_str().unwrap_or("").to_lowercase();
                    let ticker = m["ticker"].as_str().unwrap_or("").to_lowercase();
                    title.contains(&query_lc) || ticker.contains(&query_lc)
                })
                .cloned()
                .collect();
            serde_json::Value::Array(matched)
        }
        None => data,
    };

    output::success(filtered);
    Ok(())
}

async fn markets(status: Option<String>, sort: &str, limit: u32, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new(env)?;
    let limit_str = limit.to_string();
    let status_str = status.as_deref().unwrap_or("open");

    // Kalshi API does not support sort-by natively in v2;
    // we fetch and sort client-side.
    let raw = client
        .get("/markets", &[("status", status_str), ("limit", &limit_str)])
        .await?;

    // Kalshi /markets returns {cursor, markets: [...]}: extract the array
    let data = if let serde_json::Value::Object(mut obj) = raw {
        obj.remove("markets")
            .unwrap_or_else(|| serde_json::Value::Array(vec![]))
    } else {
        raw
    };

    let sorted = sort_markets(data, sort);
    output::success(sorted);
    Ok(())
}

/// Sort markets array client-side by the requested field.
fn sort_markets(data: serde_json::Value, sort: &str) -> serde_json::Value {
    let mut arr = match data {
        serde_json::Value::Array(a) => a,
        other => return other,
    };
    match sort {
        "liquidity" => arr.sort_by(|a, b| {
            let av = a["liquidity"].as_f64().unwrap_or(0.0);
            let bv = b["liquidity"].as_f64().unwrap_or(0.0);
            bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
        }),
        "newest" => arr.sort_by(|a, b| {
            let av = a["created_time"].as_str().unwrap_or("");
            let bv = b["created_time"].as_str().unwrap_or("");
            bv.cmp(av)
        }),
        "ending" => arr.sort_by(|a, b| {
            let av = a["close_time"].as_str().unwrap_or("");
            let bv = b["close_time"].as_str().unwrap_or("");
            av.cmp(bv)
        }),
        // Default: volume
        _ => arr.sort_by(|a, b| {
            let av = a["volume"].as_f64().unwrap_or(0.0);
            let bv = b["volume"].as_f64().unwrap_or(0.0);
            bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
        }),
    }
    serde_json::Value::Array(arr)
}

async fn event(event_ticker: &str, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new(env)?;
    let path = format!("/events/{}", event_ticker);
    let data = client.get(&path, &[]).await?;
    output::success(data);
    Ok(())
}

async fn price(ticker: &str, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new(env)?;
    let path = format!("/markets/{}", ticker);
    let data = client.get(&path, &[]).await?;

    // Extract and enrich price fields
    let yes_bid = data["yes_bid"].as_i64().unwrap_or(0);
    let yes_ask = data["yes_ask"].as_i64().unwrap_or(0);
    let no_bid = data["no_bid"].as_i64().unwrap_or(0);
    let no_ask = data["no_ask"].as_i64().unwrap_or(0);

    let result = serde_json::json!({
        "ticker": ticker,
        "title": data["title"],
        "yes_bid":       yes_bid,
        "yes_ask":       yes_ask,
        "no_bid":        no_bid,
        "no_ask":        no_ask,
        "yes_mid":       (yes_bid + yes_ask) / 2,
        "no_mid":        (no_bid + no_ask) / 2,
        "yes_probability": yes_bid as f64 / 100.0,
        "no_probability":  no_bid as f64 / 100.0,
        "status":        data["status"],
        "close_time":    data["close_time"],
    });
    output::success(result);
    Ok(())
}

async fn book(ticker: &str, depth: u32, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new(env)?;
    let depth_str = depth.to_string();
    let path = format!("/markets/{}/orderbook", ticker);
    let data = client.get(&path, &[("depth", &depth_str)]).await?;
    output::success(data);
    Ok(())
}

async fn history(ticker: &str, interval: &str, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new(env)?;

    // Derive series ticker (everything before the first '-')
    let series_ticker = ticker.split('-').next().unwrap_or(ticker);
    let path = format!("/series/{}/markets/{}/candlesticks", series_ticker, ticker);

    // Map interval string to period_interval (minutes) and a suitable lookback window
    let (period_interval, window_secs): (i64, i64) = match interval {
        "1m" => (1, 86_400),            // last 24 h of 1-min candles
        "1h" => (60, 7 * 86_400),       // last 7 d of 1-hour candles
        "6h" => (360, 30 * 86_400),     // last 30 d of 6-hour candles
        "1w" => (10_080, 365 * 86_400), // last year of weekly candles
        _ => (1_440, 90 * 86_400),      // "1d" / "all": last 90 d of daily candles
    };

    let end_ts = chrono::Utc::now().timestamp();
    let start_ts = end_ts - window_secs;
    let pi_str = period_interval.to_string();
    let start_str = start_ts.to_string();
    let end_str = end_ts.to_string();

    let data = client
        .get(
            &path,
            &[
                ("period_interval", &pi_str),
                ("start_ts", &start_str),
                ("end_ts", &end_str),
            ],
        )
        .await?;
    output::success(data);
    Ok(())
}

// ---------------------------------------------------------------------------
// Authenticated trading commands
// ---------------------------------------------------------------------------

async fn buy(
    ticker: &str,
    side: &str,
    count: u32,
    price: &str,
    order_type: &str,
    env: &KalshiEnv,
) -> Result<()> {
    let client = KalshiClient::new_authenticated(env)?;

    let side_lc = side.to_lowercase();
    if side_lc != "yes" && side_lc != "no" {
        bail!("--side must be 'yes' or 'no'");
    }

    let price_f: f64 = price
        .parse()
        .context("--price must be a number between 0 and 1")?;
    if price_f <= 0.0 || price_f >= 1.0 {
        bail!("--price must be strictly between 0 and 1 (e.g. 0.65)");
    }
    let price_cents = probability_to_cents(price_f);

    let mut order = serde_json::json!({
        "action": "buy",
        "ticker": ticker,
        "type":   order_type,
        "side":   side_lc,
        "count":  count,
    });
    if side_lc == "yes" {
        order["yes_price"] = serde_json::json!(price_cents);
    } else {
        order["no_price"] = serde_json::json!(price_cents);
    }

    let data = client.auth_post("/portfolio/orders", &order).await?;
    output::success(data);
    Ok(())
}

async fn sell(
    ticker: &str,
    side: &str,
    count: u32,
    price: &str,
    order_type: &str,
    env: &KalshiEnv,
) -> Result<()> {
    let client = KalshiClient::new_authenticated(env)?;

    let side_lc = side.to_lowercase();
    if side_lc != "yes" && side_lc != "no" {
        bail!("--side must be 'yes' or 'no'");
    }

    let price_f: f64 = price
        .parse()
        .context("--price must be a number between 0 and 1")?;
    if price_f <= 0.0 || price_f >= 1.0 {
        bail!("--price must be strictly between 0 and 1 (e.g. 0.65)");
    }
    let price_cents = probability_to_cents(price_f);

    let mut order = serde_json::json!({
        "action": "sell",
        "ticker": ticker,
        "type":   order_type,
        "side":   side_lc,
        "count":  count,
    });
    if side_lc == "yes" {
        order["yes_price"] = serde_json::json!(price_cents);
    } else {
        order["no_price"] = serde_json::json!(price_cents);
    }

    let data = client.auth_post("/portfolio/orders", &order).await?;
    output::success(data);
    Ok(())
}

async fn cancel(order_id: &str, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new_authenticated(env)?;
    let path = format!("/portfolio/orders/{}", order_id);
    let data = client.auth_delete(&path).await?;
    output::success(data);
    Ok(())
}

async fn orders(ticker: Option<String>, status: Option<String>, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new_authenticated(env)?;
    let ticker_str = ticker.unwrap_or_default();
    let status_str = status.unwrap_or_default();
    let data = client
        .auth_get(
            "/portfolio/orders",
            &[("ticker", &ticker_str), ("status", &status_str)],
        )
        .await?;
    output::success(data);
    Ok(())
}

async fn positions(settlement_status: &str, env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new_authenticated(env)?;
    let data = client
        .auth_get(
            "/portfolio/positions",
            &[("settlement_status", settlement_status)],
        )
        .await?;
    output::success(data);
    Ok(())
}

async fn balance(env: &KalshiEnv) -> Result<()> {
    let client = KalshiClient::new_authenticated(env)?;
    let data = client.auth_get("/portfolio/balance", &[]).await?;
    output::success(data);
    Ok(())
}
