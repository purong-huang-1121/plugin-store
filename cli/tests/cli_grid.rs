//! Integration tests for `plugin-store grid` commands.

mod common;

use common::plugin_store;
use predicates::prelude::*;

// ─── status (works with or without state file) ─────────────────────

#[test]
fn grid_status_returns_json_envelope() {
    let output = plugin_store()
        .args(["grid", "status"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── report (works without state) ──────────────────────────────────

#[test]
fn grid_report_returns_json_envelope() {
    let output = plugin_store()
        .args(["grid", "report"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── history (works without state) ─────────────────────────────────

#[test]
fn grid_history_returns_json_envelope() {
    let output = plugin_store()
        .args(["grid", "history"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

#[test]
fn grid_history_with_limit() {
    let output = plugin_store()
        .args(["grid", "history", "--limit", "5"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── reset requires --force ────────────────────────────────────────

#[test]
fn grid_reset_without_force_fails() {
    let output = plugin_store()
        .args(["grid", "reset"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(
        json["ok"],
        serde_json::Value::Bool(false),
        "reset without --force should fail"
    );
    assert!(
        json["error"].as_str().unwrap_or("").contains("--force"),
        "expected error about --force flag: {json}"
    );
}

// ─── deposit requires params ───────────────────────────────────────

#[test]
fn grid_deposit_missing_params_fails() {
    plugin_store()
        .args(["grid", "deposit"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ─── analyze returns JSON (success or auth error) ──────────────────

#[test]
fn grid_analyze_returns_json_envelope() {
    let output = plugin_store()
        .args(["grid", "analyze"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    // Either ok:true (API keys set) or ok:false (missing keys)
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── tick returns JSON (success or auth error) ─────────────────────

#[test]
fn grid_tick_returns_json_envelope() {
    let output = plugin_store()
        .args(["grid", "tick"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── retry (no failed trades → error) ──────────────────────────────

#[test]
fn grid_retry_returns_json_envelope() {
    let output = plugin_store()
        .args(["grid", "retry"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── help text ─────────────────────────────────────────────────────

#[test]
fn grid_help_shows_subcommands() {
    plugin_store()
        .args(["grid", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tick"))
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("report"))
        .stdout(predicate::str::contains("history"));
}
