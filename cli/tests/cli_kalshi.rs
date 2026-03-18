//! Integration tests for `plugin-store kalshi` commands.
//!
//! Read-only tests (search, markets, event, price, book, history) run against
//! the **demo** environment by default — no credentials needed.
//!
//! Trading tests (buy, sell, cancel, orders, positions, balance) require
//! KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM to be set and are marked #[ignore]
//! to avoid unintended execution in CI.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};
use predicates::prelude::*;
use serde_json::Value;

// ─── Helper: fetch a real contract ticker from the demo API ─────────────────

/// Try to fetch one active market ticker from the demo environment.
/// Returns None if the API is unreachable or returns no results.
fn fetch_demo_ticker() -> Option<String> {
    let output = plugin_store()
        .args(["kalshi", "--env", "demo", "markets", "--limit", "1"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).ok()?;
    if json["ok"] != Value::Bool(true) {
        return None;
    }
    let markets = json["data"].as_array()?;
    let first = markets.first()?;
    first["ticker"].as_str().map(String::from)
}

fn fetch_demo_event_ticker() -> Option<String> {
    let output = plugin_store()
        .args(["kalshi", "--env", "demo", "markets", "--limit", "1"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).ok()?;
    if json["ok"] != Value::Bool(true) {
        return None;
    }
    let markets = json["data"].as_array()?;
    let first = markets.first()?;
    first["event_ticker"].as_str().map(String::from)
}

// ─── markets ────────────────────────────────────────────────────────────────

#[test]
fn kalshi_markets_demo_returns_list() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "markets"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array of markets: {data}");
    let arr = data.as_array().unwrap();
    assert!(!arr.is_empty(), "expected at least one market");
}

#[test]
fn kalshi_markets_with_limit() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "markets", "--limit", "3"]);
    let data = assert_ok_and_extract_data(&output);
    let arr = data.as_array().expect("expected array");
    assert!(
        arr.len() <= 3,
        "expected at most 3 results, got {}",
        arr.len()
    );
}

#[test]
fn kalshi_markets_open_status() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "markets", "--status", "open"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array());
}

#[test]
fn kalshi_markets_invalid_env_fails() {
    plugin_store()
        .args(["kalshi", "--env", "staging", "markets"])
        .assert()
        .failure();
}

// ─── search ─────────────────────────────────────────────────────────────────

#[test]
fn kalshi_search_returns_results() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "search", "fed"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array: {data}");
}

#[test]
fn kalshi_search_with_limit() {
    let output = run_with_retry(&[
        "kalshi", "--env", "demo", "search", "bitcoin", "--limit", "5",
    ]);
    let data = assert_ok_and_extract_data(&output);
    let arr = data.as_array().expect("expected array");
    assert!(arr.len() <= 5);
}

#[test]
fn kalshi_search_missing_query_fails() {
    plugin_store()
        .args(["kalshi", "--env", "demo", "search"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── event ───────────────────────────────────────────────────────────────────

#[test]
fn kalshi_event_with_real_ticker() {
    let event_ticker = match fetch_demo_event_ticker() {
        Some(t) => t,
        None => {
            eprintln!("SKIP kalshi_event_with_real_ticker: could not fetch a demo event ticker");
            return;
        }
    };
    let output = run_with_retry(&["kalshi", "--env", "demo", "event", &event_ticker]);
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data.is_object() || data.is_array(),
        "expected event data: {data}"
    );
}

#[test]
fn kalshi_event_missing_ticker_fails() {
    plugin_store()
        .args(["kalshi", "--env", "demo", "event"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── price ───────────────────────────────────────────────────────────────────

#[test]
fn kalshi_price_with_real_ticker() {
    let ticker = match fetch_demo_ticker() {
        Some(t) => t,
        None => {
            eprintln!("SKIP kalshi_price_with_real_ticker: could not fetch a demo ticker");
            return;
        }
    };
    let output = run_with_retry(&["kalshi", "--env", "demo", "price", &ticker]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected price object: {data}");
    assert!(
        data.get("yes_bid").is_some() || data.get("ticker").is_some(),
        "expected price fields in response: {data}"
    );
}

#[test]
fn kalshi_price_missing_ticker_fails() {
    plugin_store()
        .args(["kalshi", "--env", "demo", "price"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── book ─────────────────────────────────────────────────────────────────────

#[test]
fn kalshi_book_with_real_ticker() {
    let ticker = match fetch_demo_ticker() {
        Some(t) => t,
        None => {
            eprintln!("SKIP kalshi_book_with_real_ticker: could not fetch a demo ticker");
            return;
        }
    };
    let output = run_with_retry(&["kalshi", "--env", "demo", "book", &ticker]);
    // Kalshi's orderbook service is occasionally unavailable (HTTP 503).
    // Treat service_unavailable as a skip rather than a test failure.
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if json["ok"] == serde_json::Value::Bool(false) {
            let err = json["error"].as_str().unwrap_or("");
            if err.contains("503") || err.contains("service_unavailable") {
                eprintln!("SKIP kalshi_book_with_real_ticker: orderbook service unavailable");
                return;
            }
        }
    }
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data.is_object() || data.is_array(),
        "expected orderbook data: {data}"
    );
}

#[test]
fn kalshi_book_missing_ticker_fails() {
    plugin_store()
        .args(["kalshi", "--env", "demo", "book"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── history ──────────────────────────────────────────────────────────────────

#[test]
fn kalshi_history_with_real_ticker() {
    let ticker = match fetch_demo_ticker() {
        Some(t) => t,
        None => {
            eprintln!("SKIP kalshi_history_with_real_ticker: could not fetch a demo ticker");
            return;
        }
    };
    let output = run_with_retry(&["kalshi", "--env", "demo", "history", &ticker]);
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data.is_array() || data.is_object(),
        "expected history data: {data}"
    );
}

#[test]
fn kalshi_history_with_interval() {
    let ticker = match fetch_demo_ticker() {
        Some(t) => t,
        None => {
            eprintln!("SKIP kalshi_history_with_interval: could not fetch a demo ticker");
            return;
        }
    };
    let output = run_with_retry(&[
        "kalshi",
        "--env",
        "demo",
        "history",
        &ticker,
        "--interval",
        "1h",
    ]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array() || data.is_object());
}

// ─── buy / sell — invalid input validation (no credentials needed) ───────────

#[test]
fn kalshi_buy_invalid_side_fails() {
    // Even without credentials, invalid --side should error before auth check
    let output = plugin_store()
        .args([
            "kalshi",
            "--env",
            "demo",
            "buy",
            "--ticker",
            "FAKE-24DEC",
            "--side",
            "maybe",
            "--count",
            "10",
            "--price",
            "0.5",
        ])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should fail — either CLI parse error or runtime validation error
    let is_failure = !output.status.success() || {
        let json: Value = serde_json::from_str(&stdout).unwrap_or_default();
        json["ok"] == Value::Bool(false)
    };
    assert!(is_failure, "expected failure for invalid --side: {stdout}");
}

#[test]
fn kalshi_buy_price_out_of_range_fails() {
    let output = plugin_store()
        .args([
            "kalshi",
            "--env",
            "demo",
            "buy",
            "--ticker",
            "FAKE-24DEC",
            "--side",
            "yes",
            "--count",
            "10",
            "--price",
            "1.5",
        ])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let is_failure = !output.status.success() || {
        let json: Value = serde_json::from_str(&stdout).unwrap_or_default();
        json["ok"] == Value::Bool(false)
    };
    assert!(is_failure, "expected failure for price > 1: {stdout}");
}

#[test]
fn kalshi_buy_missing_params_fails() {
    plugin_store()
        .args(["kalshi", "--env", "demo", "buy"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn kalshi_sell_missing_params_fails() {
    plugin_store()
        .args(["kalshi", "--env", "demo", "sell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── authenticated commands — require credentials (ignored in CI) ─────────────

#[test]
fn kalshi_buy_missing_credentials_returns_error() {
    // Skip if credentials are available via env var OR cached credentials file.
    let cache_path = dirs::home_dir()
        .map(|h| h.join(".plugin-store/kalshi_demo.json"))
        .filter(|p| p.exists());
    if std::env::var("KALSHI_KEY_ID").is_ok() || cache_path.is_some() {
        eprintln!("SKIP: credentials are set — skipping missing-key test");
        return;
    }
    let output = plugin_store()
        .args([
            "kalshi",
            "--env",
            "demo",
            "buy",
            "--ticker",
            "FAKE-24DEC",
            "--side",
            "yes",
            "--count",
            "10",
            "--price",
            "0.5",
        ])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(
        json["ok"],
        Value::Bool(false),
        "expected ok=false when no credentials: {json}"
    );
    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.contains("KALSHI_KEY_ID")
            || err.contains("KALSHI_PRIVATE_KEY_PEM")
            || err.contains("credentials"),
        "expected error about missing credentials, got: {err}"
    );
}

#[test]
#[ignore = "requires KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM for demo environment"]
fn kalshi_balance_demo_authenticated() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "balance"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected balance object: {data}");
}

#[test]
#[ignore = "requires KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM for demo environment"]
fn kalshi_orders_demo_authenticated() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "orders"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data.is_array() || data.is_object(),
        "expected orders: {data}"
    );
}

#[test]
#[ignore = "requires KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM for demo environment"]
fn kalshi_positions_demo_authenticated() {
    let output = run_with_retry(&["kalshi", "--env", "demo", "positions"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data.is_array() || data.is_object(),
        "expected positions: {data}"
    );
}
