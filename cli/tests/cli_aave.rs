//! Integration tests for `plugin-store aave` commands.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};
use predicates::prelude::*;

// ─── markets ────────────────────────────────────────────────────────

#[test]
fn aave_markets_returns_data() {
    let output = run_with_retry(&["aave", "markets", "--chain", "ethereum"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected object: {data}");
    let markets = data["markets"].as_array().expect("expected markets array");
    assert!(!markets.is_empty(), "expected at least one market");
    let has_usdc = markets.iter().any(|m| m["symbol"].as_str() == Some("USDC"));
    assert!(has_usdc, "expected USDC in markets");
}

#[test]
fn aave_markets_invalid_chain() {
    let output = plugin_store()
        .args(["aave", "markets", "--chain", "fantom"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

// ─── reserve ────────────────────────────────────────────────────────

#[test]
fn aave_reserve_usdc() {
    let output = run_with_retry(&["aave", "reserve", "USDC", "--chain", "ethereum"]);
    let data = assert_ok_and_extract_data(&output);
    assert_eq!(data["symbol"].as_str(), Some("USDC"));
    assert!(data["supply_apy_percent"].is_string());
    assert!(data["variable_borrow_apy_percent"].is_string());
}

#[test]
fn aave_reserve_not_found() {
    let output = plugin_store()
        .args(["aave", "reserve", "FAKECOIN", "--chain", "ethereum"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

// ─── account ────────────────────────────────────────────────────────

#[test]
fn aave_account_invalid_address() {
    let output = plugin_store()
        .args(["aave", "account", "not-an-address", "--chain", "ethereum"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

// ─── supply/withdraw (require onchainos wallet login) ──────────────────────

#[test]
fn aave_supply_missing_wallet_fails() {
    let output = plugin_store()
        .args([
            "aave", "supply", "--token", "USDC", "--amount", "100", "--chain", "ethereum",
        ])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
    assert!(
        !json["error"].as_str().unwrap_or("").is_empty(),
        "expected error message: {json}"
    );
}

#[test]
fn aave_supply_missing_params_fails() {
    plugin_store()
        .args(["aave", "supply"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn aave_withdraw_missing_params_fails() {
    plugin_store()
        .args(["aave", "withdraw"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}
