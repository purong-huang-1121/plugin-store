//! Integration tests for `plugin-store scanner` commands.

mod common;

use common::plugin_store;
use predicates::prelude::*;

// ─── status (works with or without state file) ─────────────────────

#[test]
fn scanner_status_returns_json_envelope() {
    let output = plugin_store()
        .args(["scanner", "status"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── report (works without state) ──────────────────────────────────

#[test]
fn scanner_report_returns_json_envelope() {
    let output = plugin_store()
        .args(["scanner", "report"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

// ─── history (works without state) ─────────────────────────────────

#[test]
fn scanner_history_returns_json_envelope() {
    let output = plugin_store()
        .args(["scanner", "history"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

#[test]
fn scanner_history_with_limit() {
    let output = plugin_store()
        .args(["scanner", "history", "--limit", "5"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── reset requires --force ────────────────────────────────────────

#[test]
fn scanner_reset_without_force_fails() {
    let output = plugin_store()
        .args(["scanner", "reset"])
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

// ─── stop with no running bot ──────────────────────────────────────

#[test]
fn scanner_stop_no_bot_returns_error() {
    let output = plugin_store()
        .args(["scanner", "stop"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

// ─── tick returns JSON (success or auth error) ─────────────────────

#[test]
fn scanner_tick_returns_json_envelope() {
    let output = plugin_store()
        .args(["scanner", "tick"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── analyze returns JSON (success or auth error) ──────────────────

#[test]
fn scanner_analyze_returns_json_envelope() {
    let output = plugin_store()
        .args(["scanner", "analyze"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert!(json.get("ok").is_some(), "expected JSON envelope: {stdout}");
}

// ─── help text ─────────────────────────────────────────────────────

#[test]
fn scanner_help_shows_subcommands() {
    plugin_store()
        .args(["scanner", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tick"))
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("report"))
        .stdout(predicate::str::contains("history"))
        .stdout(predicate::str::contains("analyze"));
}

// ─── missing subcommand ────────────────────────────────────────────

#[test]
fn scanner_missing_subcommand_fails() {
    plugin_store()
        .args(["scanner"])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());
}
