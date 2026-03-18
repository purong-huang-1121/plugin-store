//! Background update checker.
//!
//! Checks GitHub releases once every 12 hours (cached to ~/.plugin-store/update_check/).
//! Prints a one-line notice to stderr if a newer version is available.
//! Never blocks the main command — runs in a detached thread with a 3s timeout.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CHECK_INTERVAL_SECS: u64 = 43_200; // 12 hours
const REPO: &str = "purong-huang-1121/plugin-store";
const REINSTALL_URL: &str =
    "https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/reinstall.sh";
const INSTALL_STRATEGY_URL: &str =
    "https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/install_strategy.sh";

fn update_command(binary_name: &str) -> String {
    if binary_name == "plugin-store" {
        format!("curl -sSL {} | sh", REINSTALL_URL)
    } else {
        // Update binary + refresh strategy skill
        format!(
            "curl -sSL {} | sh -s -- {} && npx skills add purong-huang-1121/plugin-store --skill {} --yes",
            INSTALL_STRATEGY_URL, binary_name, binary_name
        )
    }
}

fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".plugin-store").join("update_check")
}

fn is_cache_fresh(binary_name: &str) -> bool {
    let path = cache_dir().join(binary_name);
    let Ok(s) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(ts) = s.trim().parse::<u64>() else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.saturating_sub(ts) < CHECK_INTERVAL_SECS
}

fn write_cache(binary_name: &str) {
    let dir = cache_dir();
    let _ = fs::create_dir_all(&dir);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = fs::write(dir.join(binary_name), now.to_string());
}

/// Parse `"tag_name": "v1.0.21"` from a GitHub releases JSON response.
fn parse_tag(json: &str) -> Option<&str> {
    let after = json.split("\"tag_name\"").nth(1)?;
    let after_colon = after.splitn(2, ':').nth(1)?.trim();
    let inner = after_colon.trim_start_matches('"');
    let tag = inner.split('"').next()?;
    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

/// Returns true if `a` is strictly newer than `b` (simple semver, ignores pre-release labels).
fn is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let s = s.trim_start_matches('v');
        let mut it = s.splitn(3, '.');
        let maj = it.next().and_then(|x| x.split('-').next()).and_then(|x| x.parse().ok()).unwrap_or(0);
        let min = it.next().and_then(|x| x.split('-').next()).and_then(|x| x.parse().ok()).unwrap_or(0);
        let pat = it.next().and_then(|x| x.split('-').next()).and_then(|x| x.parse().ok()).unwrap_or(0);
        (maj, min, pat)
    };
    parse(a) > parse(b)
}

/// Spawn a background thread that checks for a newer release on GitHub.
/// Prints a notice to stderr if an update is available.
/// Skips if already checked within the last 12 hours.
pub fn check(binary_name: &str, current_version: &str) {
    if is_cache_fresh(binary_name) {
        return;
    }
    // Write cache immediately so parallel invocations don't all hit GitHub.
    write_cache(binary_name);

    let binary_name = binary_name.to_string();
    let current_version = current_version.to_string();

    std::thread::spawn(move || {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let Ok(output) = std::process::Command::new("curl")
            .args([
                "-sSf",
                "--max-time",
                "3",
                "-H",
                "User-Agent: plugin-store-cli",
                &url,
            ])
            .output()
        else {
            return;
        };

        if !output.status.success() {
            return;
        }

        let body = String::from_utf8_lossy(&output.stdout);
        let Some(tag) = parse_tag(&body) else { return };

        if is_newer(tag, &current_version) {
            eprintln!(
                "\n[{}] 新版本可用: {} → {}  更新: {}\n",
                binary_name,
                current_version,
                tag.trim_start_matches('v'),
                update_command(&binary_name),
            );
        }
    });
}
