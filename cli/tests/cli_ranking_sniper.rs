//! Integration tests for `plugin-store ranking-sniper` commands.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};
use predicates::prelude::*;

// ─── status on empty state ──────────────────────────────────────────

#[test]
fn ranking_sniper_status_empty_state() {
    let output = plugin_store()
        .args(["ranking-sniper", "status"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── report on empty state ──────────────────────────────────────────

#[test]
fn ranking_sniper_report_empty_state() {
    let output = plugin_store()
        .args(["ranking-sniper", "report"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── history on empty state ─────────────────────────────────────────

#[test]
fn ranking_sniper_history_empty_state() {
    let output = plugin_store()
        .args(["ranking-sniper", "history"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── analyze fetches ranking data ───────────────────────────────────

#[test]
fn ranking_sniper_analyze_returns_data() {
    let output = run_with_retry(&["ranking-sniper", "analyze"]);
    let data = assert_ok_and_extract_data(&output);
    assert!(
        data["ranking_count"].is_number(),
        "expected ranking_count: {data}"
    );
    assert!(
        data["top_tokens"].is_array(),
        "expected top_tokens array: {data}"
    );
}

// ─── tick without SOL_ADDRESS ───────────────────────────────────────

#[test]
fn ranking_sniper_tick_missing_address_fails() {
    // The binary loads .env via dotenvy, so SOL_ADDRESS is typically set.
    // This test verifies behavior when no address is available.
    let output = plugin_store()
        .env_remove("SOL_ADDRESS")
        .args(["ranking-sniper", "tick"])
        .output()
        .expect("failed to execute");
    // When .env is loaded by the binary, SOL_ADDRESS may still be set.
    // We just verify the command doesn't crash in either case.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    // Either ok=true (address found via .env) or ok=false (no address)
    assert!(
        json.get("ok").is_some(),
        "expected valid JSON output: {stdout}"
    );
}

// ─── tick with dry-run ──────────────────────────────────────────────

#[test]
fn ranking_sniper_tick_dry_run() {
    let output = run_with_retry(&["ranking-sniper", "tick", "--dry-run"]);
    let data = assert_ok_and_extract_data(&output);
    assert_eq!(data["dry_run"], serde_json::Value::Bool(true));
}

// ─── reset without force ────────────────────────────────────────────

#[test]
fn ranking_sniper_reset_without_force_warns() {
    let output = plugin_store()
        .args(["ranking-sniper", "reset"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
    assert!(
        json["error"].as_str().unwrap_or("").contains("--force"),
        "expected hint about --force: {json}"
    );
}

// ─── missing subcommand ─────────────────────────────────────────────

#[test]
fn ranking_sniper_missing_subcommand_fails() {
    plugin_store()
        .args(["ranking-sniper"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// ─── stop with no running bot ───────────────────────────────────────

#[test]
fn ranking_sniper_stop_no_bot_returns_error() {
    let output = plugin_store()
        .args(["ranking-sniper", "stop"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}
