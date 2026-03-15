//! Integration tests for `plugin-store uniswap` commands.

mod common;

use common::{assert_ok_and_extract_data, plugin_store};
use predicates::prelude::*;

// ─── tokens ─────────────────────────────────────────────────────────

#[test]
fn uniswap_tokens_arbitrum() {
    let output = plugin_store()
        .args(["uniswap", "tokens", "--chain", "arbitrum"])
        .output()
        .expect("failed to execute");
    let data = assert_ok_and_extract_data(&output);
    let tokens = data["tokens"].as_array().expect("expected tokens array");
    assert!(!tokens.is_empty(), "expected at least one token");
    let has_weth = tokens.iter().any(|t| t["symbol"].as_str() == Some("WETH"));
    assert!(has_weth, "expected WETH in token list");
}

#[test]
fn uniswap_tokens_ethereum() {
    let output = plugin_store()
        .args(["uniswap", "tokens", "--chain", "ethereum"])
        .output()
        .expect("failed to execute");
    let data = assert_ok_and_extract_data(&output);
    let tokens = data["tokens"].as_array().expect("expected tokens array");
    let has_susde = tokens.iter().any(|t| t["symbol"].as_str() == Some("sUSDe"));
    assert!(has_susde, "expected sUSDe in Ethereum token list");
}

#[test]
fn uniswap_tokens_unsupported_chain() {
    let output = plugin_store()
        .args(["uniswap", "tokens", "--chain", "fantom"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

// ─── swap missing params ────────────────────────────────────────────

#[test]
fn uniswap_swap_missing_params_fails() {
    plugin_store()
        .args(["uniswap", "swap"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn uniswap_quote_missing_params_fails() {
    plugin_store()
        .args(["uniswap", "quote"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── swap without private key ───────────────────────────────────────

#[test]
fn uniswap_swap_without_key_or_balance_fails() {
    let output = plugin_store()
        .args([
            "uniswap", "swap", "--from", "WETH", "--to", "wstETH", "--amount", "0.01", "--chain",
            "arbitrum",
        ])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    // Should fail: either missing key or insufficient balance
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}
