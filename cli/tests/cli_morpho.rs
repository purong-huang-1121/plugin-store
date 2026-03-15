//! Integration tests for `plugin-store morpho` commands.
//!
//! All Morpho API endpoints are public — no credentials needed.
//! Tests that make live API calls are marked with `run_with_retry` to handle
//! transient rate-limiting from the Morpho GraphQL API.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};

// ─── markets ─────────────────────────────────────────────────────────────────

#[test]
fn morpho_markets_returns_list() {
    let output = run_with_retry(&["morpho", "markets"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array of markets: {data}");
    let arr = data.as_array().unwrap();
    assert!(!arr.is_empty(), "expected at least one market");
}

#[test]
fn morpho_markets_with_limit() {
    let output = run_with_retry(&["morpho", "markets", "--limit", "3"]);
    let data = assert_ok_and_extract_data(&output);
    let arr = data.as_array().expect("expected array");
    assert!(
        arr.len() <= 3,
        "expected at most 3 results, got {}",
        arr.len()
    );
}

#[test]
fn morpho_markets_filter_by_chain() {
    let output = run_with_retry(&["morpho", "markets", "--chain", "base", "--limit", "5"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array: {data}");
}

#[test]
fn morpho_markets_invalid_chain_fails() {
    let output = plugin_store()
        .args(["morpho", "markets", "--chain", "notachain"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let is_failure = !output.status.success() || {
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
        json["ok"] == serde_json::Value::Bool(false)
    };
    assert!(is_failure, "expected failure for invalid chain: {stdout}");
}

// ─── market ──────────────────────────────────────────────────────────────────

#[test]
fn morpho_market_fetches_from_live_list() {
    // Fetch a real unique key from the markets list first
    let list_output = run_with_retry(&["morpho", "markets", "--limit", "1"]);
    let list_data = assert_ok_and_extract_data(&list_output);
    let unique_key = list_data
        .as_array()
        .and_then(|a| a.first())
        .and_then(|m| m["uniqueKey"].as_str())
        .map(String::from);
    // Chain info is nested under morphoBlue.chain in the Market type
    let chain_id = list_data
        .as_array()
        .and_then(|a| a.first())
        .and_then(|m| m["morphoBlue"]["chain"]["id"].as_u64())
        .unwrap_or(1);

    let Some(key) = unique_key else {
        eprintln!("SKIP morpho_market_fetches_from_live_list: could not extract uniqueKey");
        return;
    };

    let output = run_with_retry(&[
        "morpho",
        "market",
        &key,
        "--chain-id",
        &chain_id.to_string(),
    ]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected market object: {data}");
    assert!(
        data.get("uniqueKey").is_some(),
        "expected uniqueKey in response: {data}"
    );
}

#[test]
fn morpho_market_missing_key_fails() {
    plugin_store()
        .args(["morpho", "market"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

// ─── vaults ──────────────────────────────────────────────────────────────────

#[test]
fn morpho_vaults_returns_list() {
    let output = run_with_retry(&["morpho", "vaults"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array of vaults: {data}");
    let arr = data.as_array().unwrap();
    assert!(!arr.is_empty(), "expected at least one vault");
}

#[test]
fn morpho_vaults_with_limit() {
    let output = run_with_retry(&["morpho", "vaults", "--limit", "3"]);
    let data = assert_ok_and_extract_data(&output);
    let arr = data.as_array().expect("expected array");
    assert!(
        arr.len() <= 3,
        "expected at most 3 vaults, got {}",
        arr.len()
    );
}

#[test]
fn morpho_vaults_filter_by_chain() {
    let output = run_with_retry(&["morpho", "vaults", "--chain", "ethereum", "--limit", "5"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_array(), "expected array: {data}");
}

// ─── vault ───────────────────────────────────────────────────────────────────

#[test]
fn morpho_vault_fetches_from_live_list() {
    let list_output = run_with_retry(&["morpho", "vaults", "--limit", "1"]);
    let list_data = assert_ok_and_extract_data(&list_output);
    let address = list_data
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v["address"].as_str())
        .map(String::from);
    let chain_id = list_data
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v["chain"]["id"].as_u64())
        .unwrap_or(1);

    let Some(addr) = address else {
        eprintln!("SKIP morpho_vault_fetches_from_live_list: could not extract vault address");
        return;
    };

    let output = run_with_retry(&[
        "morpho",
        "vault",
        &addr,
        "--chain-id",
        &chain_id.to_string(),
    ]);
    let data = assert_ok_and_extract_data(&output);
    assert!(data.is_object(), "expected vault object: {data}");
}

#[test]
fn morpho_vault_missing_address_fails() {
    plugin_store()
        .args(["morpho", "vault"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

// ─── positions ───────────────────────────────────────────────────────────────

#[test]
fn morpho_positions_missing_address_fails() {
    plugin_store()
        .args(["morpho", "positions"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

#[test]
fn morpho_positions_returns_user_data() {
    // Use a known active Morpho user on Base (chain 8453) with positions
    let output = run_with_retry(&[
        "morpho",
        "positions",
        "0xcBa28b38103307Ec8dA98377ffF9816C164f9AFa",
        "--chain",
        "base",
    ]);
    // The API may return ok=false with "not found" if the user has no positions —
    // that is a valid protocol response, not an integration bug.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON in stdout: {e}\nraw: {stdout}"));
    assert!(
        json["ok"] == serde_json::Value::Bool(true)
            || json["error"]
                .as_str()
                .map(|e| e.contains("not found")
                    || e.contains("No results")
                    || e.contains("cannot find"))
                .unwrap_or(false),
        "unexpected error: {json}"
    );
}
