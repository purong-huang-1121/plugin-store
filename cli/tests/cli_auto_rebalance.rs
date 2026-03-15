//! Integration tests for `plugin-store auto-rebalance` commands.

mod common;

use common::plugin_store;

/// Mutex to serialize tests that read/write the shared daemon PID file,
/// preventing race conditions when tests run in parallel.
static PID_FILE_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

fn pid_file_lock() -> &'static std::sync::Mutex<()> {
    PID_FILE_LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

fn daemon_pid_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".plugin-store")
        .join("auto-rebalance-daemon.pid")
}

// ─── start help ─────────────────────────────────────────────────────

#[test]
fn auto_rebalance_start_help() {
    let output = plugin_store()
        .args(["auto-rebalance", "start", "--help"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("interval") && combined.contains("min-spread"),
        "expected --interval and --min-spread in help output"
    );
}

// ─── status (works without daemon running) ──────────────────────────

#[test]
fn auto_rebalance_status_returns_json() {
    let _guard = pid_file_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::remove_file(&daemon_pid_path());

    let output = plugin_store()
        .args(["auto-rebalance", "status"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        panic!("status produced no output");
    }
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("expected valid JSON output");
    assert!(
        json.get("ok").is_some(),
        "expected 'ok' field in response: {json}"
    );
}

// ─── stop (no daemon running) ───────────────────────────────────────

#[test]
fn auto_rebalance_stop_no_daemon() {
    let _guard = pid_file_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::remove_file(&daemon_pid_path());

    let output = plugin_store()
        .args(["auto-rebalance", "stop"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
}

// ─── config ──────────────────────────────────────────────────────────

#[test]
fn auto_rebalance_config_returns_json() {
    let output = plugin_store()
        .args(["auto-rebalance", "config"])
        .output()
        .expect("failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("expected valid JSON output");
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
    let data = &json["data"];
    assert!(data["config_file"].is_string(), "expected config_file path");
    assert!(data["log_file"].is_string(), "expected log_file path");
    assert!(data["parameters"].is_object(), "expected parameters object");
}

// ─── already running (PID file conflict) ────────────────────────────

#[test]
fn auto_rebalance_already_running() {
    let _guard = pid_file_lock().lock().unwrap_or_else(|e| e.into_inner());
    let pid_path = daemon_pid_path();
    std::fs::create_dir_all(pid_path.parent().unwrap()).ok();

    // Spawn a long-lived sentinel process so check_running() sees it as alive.
    let mut sentinel = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("failed to spawn sentinel process");
    let sentinel_pid = sentinel.id();
    std::fs::write(&pid_path, sentinel_pid.to_string()).ok();

    let output = plugin_store()
        .args(["auto-rebalance", "start", "--interval", "1", "--yes"])
        .output()
        .expect("failed to execute");

    // Clean up before asserting so the lock release leaves a clean state.
    let _ = sentinel.kill();
    let _ = sentinel.wait();
    let _ = std::fs::remove_file(&pid_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    assert_eq!(json["ok"], serde_json::Value::Bool(false));
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .contains("already running"),
        "expected 'already running' error: {json}"
    );
}
