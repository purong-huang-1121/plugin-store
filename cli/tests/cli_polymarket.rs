//! Integration tests for `plugin-store polymarket` commands.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};
use predicates::prelude::*;

// ─── search ─────────────────────────────────────────────────────────

#[test]
fn polymarket_search_returns_results() {
    let output = run_with_retry(&["polymarket", "search", "bitcoin"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array of markets: {data}");
    let arr = data.as_array().unwrap();
    assert!(!arr.is_empty(), "expected at least one market result");
}

#[test]
fn polymarket_search_with_limit() {
    let output = run_with_retry(&["polymarket", "search", "election", "--limit", "3"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array: {data}");
    let arr = data.as_array().unwrap();
    assert!(
        arr.len() <= 3,
        "expected at most 3 results, got {}",
        arr.len()
    );
}

#[test]
fn polymarket_search_missing_query_fails() {
    plugin_store()
        .args(["polymarket", "search"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── markets ────────────────────────────────────────────────────────

#[test]
fn polymarket_markets_returns_list() {
    let output = run_with_retry(&["polymarket", "markets"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array of markets: {data}");
    let arr = data.as_array().unwrap();
    assert!(!arr.is_empty(), "expected at least one market");
}

#[test]
fn polymarket_markets_with_tag() {
    let output = run_with_retry(&["polymarket", "markets", "--tag", "crypto"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array: {data}");
}

// ─── event ──────────────────────────────────────────────────────────

#[test]
fn polymarket_event_missing_id_fails() {
    plugin_store()
        .args(["polymarket", "event"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── helpers ────────────────────────────────────────────────────────

/// Fetch a real token_id from Gamma API for testing CLOB endpoints.
fn fetch_active_token_id() -> Option<String> {
    let output = plugin_store()
        .args(["polymarket", "markets", "--limit", "1"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).ok()?;
    if json["ok"] != serde_json::Value::Bool(true) {
        return None;
    }
    let markets = json["data"].as_array()?;
    let market = markets.first()?;
    let clob_token_ids_str = market.get("clobTokenIds")?.as_str()?;
    let token_ids: Vec<String> = serde_json::from_str(clob_token_ids_str).ok()?;
    token_ids.into_iter().next()
}

// ─── price ──────────────────────────────────────────────────────────

#[test]
fn polymarket_price_with_real_token() {
    let token_id = match fetch_active_token_id() {
        Some(id) => id,
        None => {
            eprintln!("SKIP: could not fetch a live token_id");
            return;
        }
    };
    let output = run_with_retry(&["polymarket", "price", &token_id]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected price object: {data}");
}

#[test]
fn polymarket_price_missing_token_fails() {
    plugin_store()
        .args(["polymarket", "price"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── book ───────────────────────────────────────────────────────────

#[test]
fn polymarket_book_with_real_token() {
    let token_id = match fetch_active_token_id() {
        Some(id) => id,
        None => {
            eprintln!("SKIP: could not fetch a live token_id");
            return;
        }
    };
    let output = run_with_retry(&["polymarket", "book", &token_id]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected orderbook object: {data}");
}

// ─── history ────────────────────────────────────────────────────────

#[test]
fn polymarket_history_with_real_token() {
    let token_id = match fetch_active_token_id() {
        Some(id) => id,
        None => {
            eprintln!("SKIP: could not fetch a live token_id");
            return;
        }
    };
    let output = run_with_retry(&["polymarket", "history", &token_id]);
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data.is_array() || data.is_object(),
        "expected history data: {data}"
    );
}

// ─── trading commands (require EVM_PRIVATE_KEY) ──────────────

#[test]
fn polymarket_buy_missing_private_key_fails() {
    if std::env::var("EVM_PRIVATE_KEY").is_ok() {
        eprintln!("SKIP: EVM_PRIVATE_KEY is set");
        return;
    }
    let output = plugin_store()
        .args([
            "polymarket",
            "buy",
            "--token",
            "fake",
            "--amount",
            "10",
            "--price",
            "0.5",
        ])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .contains("EVM_PRIVATE_KEY"),
        "expected error about missing private key: {json}"
    );
}

#[test]
fn polymarket_buy_missing_params_fails() {
    plugin_store()
        .args(["polymarket", "buy"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn polymarket_sell_missing_params_fails() {
    plugin_store()
        .args(["polymarket", "sell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn polymarket_cancel_missing_id_fails() {
    plugin_store()
        .args(["polymarket", "cancel"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn polymarket_orders_missing_private_key_fails() {
    if std::env::var("EVM_PRIVATE_KEY").is_ok() {
        return;
    }
    let output = plugin_store()
        .args(["polymarket", "orders"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

#[test]
fn polymarket_balance_missing_private_key_fails() {
    if std::env::var("EVM_PRIVATE_KEY").is_ok() {
        return;
    }
    let output = plugin_store()
        .args(["polymarket", "balance"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}
