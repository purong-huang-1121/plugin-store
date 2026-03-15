//! Integration tests for `plugin-store hyperliquid` commands.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};
use predicates::prelude::*;

// ─── markets ────────────────────────────────────────────────────────

#[test]
fn hyperliquid_markets_returns_data() {
    let output = run_with_retry(&["hyperliquid", "markets"]);
    let data = assert_ok_and_extract_data(&output);
    let markets = data["markets"].as_array().expect("expected markets array");
    assert!(!markets.is_empty(), "expected at least one market");
    let has_btc = markets.iter().any(|m| m["symbol"].as_str() == Some("BTC"));
    assert!(has_btc, "expected BTC in markets");
}

// ─── spot-markets ───────────────────────────────────────────────────

#[test]
fn hyperliquid_spot_markets_returns_data() {
    let output = run_with_retry(&["hyperliquid", "spot-markets"]);
    let data = assert_ok_and_extract_data(&output);
    let markets = data["markets"].as_array().expect("expected markets array");
    assert!(!markets.is_empty(), "expected at least one spot market");
}

// ─── price ──────────────────────────────────────────────────────────

#[test]
fn hyperliquid_price_btc() {
    let output = run_with_retry(&["hyperliquid", "price", "BTC"]);
    let data = assert_ok_and_extract_data(&output);
    assert_eq!(data["symbol"].as_str(), Some("BTC"));
    let price: f64 = data["mid_price"]
        .as_str()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0.0);
    assert!(price > 0.0, "expected positive BTC price, got {}", price);
}

// ─── orderbook ──────────────────────────────────────────────────────

#[test]
fn hyperliquid_orderbook_btc() {
    let output = run_with_retry(&["hyperliquid", "orderbook", "BTC"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected orderbook object: {data}");
}

// ─── funding ────────────────────────────────────────────────────────

#[test]
fn hyperliquid_funding_btc() {
    let output = run_with_retry(&["hyperliquid", "funding", "BTC"]);
    let data = assert_ok_and_extract_data(&output);
    assert_eq!(data["symbol"].as_str(), Some("BTC"));
}

// ─── error cases ────────────────────────────────────────────────────

#[test]
fn hyperliquid_buy_missing_params_fails() {
    plugin_store()
        .args(["hyperliquid", "buy"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn hyperliquid_sell_missing_params_fails() {
    plugin_store()
        .args(["hyperliquid", "sell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}
