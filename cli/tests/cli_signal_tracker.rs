//! Integration tests for `plugin-store signal-tracker` commands.

mod common;

use common::{assert_ok_and_extract_data, plugin_store, run_with_retry};
use predicates::prelude::*;

// ─── status on empty state ──────────────────────────────────────────

#[test]
fn signal_tracker_status_empty_state() {
    let output = plugin_store()
        .args(["signal-tracker", "status"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── report on empty state ──────────────────────────────────────────

#[test]
fn signal_tracker_report_empty_state() {
    let output = plugin_store()
        .args(["signal-tracker", "report"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── history on empty state ─────────────────────────────────────────

#[test]
fn signal_tracker_history_empty_state() {
    let output = plugin_store()
        .args(["signal-tracker", "history"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── analyze fetches signal data ────────────────────────────────────

#[test]
fn signal_tracker_analyze_returns_data() {
    let output = run_with_retry(&["signal-tracker", "analyze"]);
    let data = assert_ok_and_extract_data(&output);
    // Field may be "signal_count" or "signals_total" depending on implementation
    let has_count = data["signal_count"].is_number() || data["signals_total"].is_number();
    assert!(has_count, "expected signal count field: {data}");
    assert!(data["signals"].is_array(), "expected signals array: {data}");
}

// ─── tick with dry-run ──────────────────────────────────────────────

#[test]
fn signal_tracker_tick_dry_run() {
    let output = run_with_retry(&["signal-tracker", "tick", "--dry-run"]);
    let data = assert_ok_and_extract_data(&output);
    assert_eq!(data["dry_run"], serde_json::Value::Bool(true));
}

// ─── tick without SOL_ADDRESS ───────────────────────────────────────

#[test]
fn signal_tracker_tick_missing_address() {
    let output = plugin_store()
        .env_remove("SOL_ADDRESS")
        .args(["signal-tracker", "tick"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(
        json.get("ok").is_some(),
        "expected valid JSON output: {stdout}"
    );
}

// ─── reset without force ────────────────────────────────────────────

#[test]
fn signal_tracker_reset_without_force_warns() {
    let output = plugin_store()
        .args(["signal-tracker", "reset"])
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
fn signal_tracker_missing_subcommand_fails() {
    plugin_store()
        .args(["signal-tracker"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// ─── stop with no running bot ───────────────────────────────────────

#[test]
fn signal_tracker_stop_no_bot_returns_error() {
    let output = plugin_store()
        .args(["signal-tracker", "stop"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}
