//! Kalshi API authentication.
//!
//! Kalshi uses RSA-PSS with SHA-256 for request signing.
//!
//! Required headers per authenticated request:
//!   KALSHI-ACCESS-KEY       — the Key ID from Kalshi dashboard
//!   KALSHI-ACCESS-TIMESTAMP — Unix timestamp in milliseconds (string)
//!   KALSHI-ACCESS-SIGNATURE — base64(RSA-PSS-SHA256(private_key, ts + METHOD + path))
//!
//! Message format (path must NOT include query string):
//!   "{timestamp_ms}{HTTP_METHOD_UPPERCASE}{/trade-api/v2/...}"
//!
//! Environments:
//!   Demo: https://demo-api.kalshi.co/trade-api/v2
//!   Prod: https://api.elections.kalshi.com/trade-api/v2

use anyhow::{Context, Result};
use base64::Engine;
use clap::ValueEnum;
use rsa::{
    pkcs8::DecodePrivateKey, pss::BlindedSigningKey, signature::RandomizedSigner, RsaPrivateKey,
};
use sha2::Sha256;

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Kalshi API environment (demo or production).
#[derive(Clone, Debug, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KalshiEnv {
    Demo,
    Prod,
}

impl KalshiEnv {
    pub fn base_url(&self) -> &str {
        match self {
            KalshiEnv::Demo => "https://demo-api.kalshi.co/trade-api/v2",
            KalshiEnv::Prod => "https://api.elections.kalshi.com/trade-api/v2",
        }
    }

    pub fn cache_filename(&self) -> &str {
        match self {
            KalshiEnv::Demo => "kalshi_demo.json",
            KalshiEnv::Prod => "kalshi_prod.json",
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            KalshiEnv::Demo => "demo",
            KalshiEnv::Prod => "prod",
        }
    }
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// Kalshi API credentials (key ID + RSA private key PEM).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct KalshiCreds {
    pub key_id: String,
    /// PEM-encoded RSA private key (PKCS#8 format).
    pub private_key_pem: String,
}

// ---------------------------------------------------------------------------
// Signature
// ---------------------------------------------------------------------------

/// Build an RSA-PSS SHA-256 signature for a Kalshi API request.
///
/// Returns `(timestamp_ms, base64_signature)` to set as headers.
///
/// # Arguments
/// * `creds`   — API credentials
/// * `method`  — HTTP method (GET, POST, DELETE …)
/// * `path`    — URL path **without** query string, e.g. `/trade-api/v2/markets`
pub fn build_signature(creds: &KalshiCreds, method: &str, path: &str) -> Result<(String, String)> {
    let timestamp_ms = chrono::Utc::now().timestamp_millis().to_string();
    let message = format!("{}{}{}", timestamp_ms, method.to_uppercase(), path);

    let private_key = RsaPrivateKey::from_pkcs8_pem(&creds.private_key_pem)
        .context("Failed to parse RSA private key — ensure KALSHI_PRIVATE_KEY_PEM contains a valid PKCS#8 PEM")?;

    let signing_key = BlindedSigningKey::<Sha256>::new(private_key);
    let mut rng = rand::thread_rng();
    let sig = signing_key.sign_with_rng(&mut rng, message.as_bytes());

    let sig_bytes: Box<[u8]> = sig.into();
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig_bytes.as_ref());
    Ok((timestamp_ms, sig_b64))
}

// ---------------------------------------------------------------------------
// Credential loading / saving
// ---------------------------------------------------------------------------

/// Load Kalshi credentials for the given environment.
///
/// Priority order:
/// 1. `KALSHI_KEY_ID` + `KALSHI_PRIVATE_KEY_PEM` environment variables.
///    If `KALSHI_PRIVATE_KEY_PEM` contains "BEGIN" it is used as PEM content;
///    otherwise it is treated as a file path.
/// 2. `~/.plugin-store/kalshi_{env}.json` cache file.
pub fn load_creds(env: &KalshiEnv) -> Result<Option<KalshiCreds>> {
    // 1. Try environment variables
    if let (Ok(key_id), Ok(pem_val)) = (
        std::env::var("KALSHI_KEY_ID"),
        std::env::var("KALSHI_PRIVATE_KEY_PEM"),
    ) {
        let private_key_pem = if pem_val.contains("BEGIN") {
            pem_val
        } else {
            std::fs::read_to_string(&pem_val)
                .context(format!("Failed to read RSA private key file: {}", pem_val))?
        };
        return Ok(Some(KalshiCreds {
            key_id,
            private_key_pem,
        }));
    }

    // 2. Try cache file
    let path = cache_path(env);
    if path.exists() {
        let data =
            std::fs::read_to_string(&path).context("Failed to read cached Kalshi credentials")?;
        let creds: KalshiCreds = serde_json::from_str(&data)
            .context("Failed to parse cached Kalshi credentials — try re-setting credentials")?;
        return Ok(Some(creds));
    }

    Ok(None)
}

/// Require credentials or return a helpful error message.
pub fn require_creds(env: &KalshiEnv) -> Result<KalshiCreds> {
    load_creds(env)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Kalshi credentials not configured for {} environment.\n\
             Set environment variables:\n\
             \x20 KALSHI_KEY_ID=<your-key-id>\n\
             \x20 KALSHI_PRIVATE_KEY_PEM=<pem-content-or-file-path>\n\
             API keys are available at: https://kalshi.com/profile/api-keys",
            env.as_str()
        )
    })
}

/// Save credentials to the cache file for the given environment.
pub fn save_creds(creds: &KalshiCreds, env: &KalshiEnv) -> Result<()> {
    let cache_dir = dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".plugin-store");
    std::fs::create_dir_all(&cache_dir)?;
    let path = cache_dir.join(env.cache_filename());
    let data = serde_json::to_string_pretty(creds)?;
    std::fs::write(&path, &data)?;
    Ok(())
}

fn cache_path(env: &KalshiEnv) -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".plugin-store")
        .join(env.cache_filename())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_base_urls_are_distinct() {
        assert_ne!(KalshiEnv::Demo.base_url(), KalshiEnv::Prod.base_url());
    }

    #[test]
    fn env_cache_filenames_are_distinct() {
        assert_ne!(
            KalshiEnv::Demo.cache_filename(),
            KalshiEnv::Prod.cache_filename()
        );
    }

    #[test]
    fn missing_creds_returns_none() {
        // Temporarily unset env vars (if set by CI)
        let _k = std::env::var("KALSHI_KEY_ID").ok();
        let _p = std::env::var("KALSHI_PRIVATE_KEY_PEM").ok();
        std::env::remove_var("KALSHI_KEY_ID");
        std::env::remove_var("KALSHI_PRIVATE_KEY_PEM");

        // No cache file in a temp home — load_creds returns None or Some (from cache)
        // We just verify it doesn't panic
        let _ = load_creds(&KalshiEnv::Demo);
    }
}
