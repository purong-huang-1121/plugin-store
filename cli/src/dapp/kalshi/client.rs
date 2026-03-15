//! Kalshi HTTP client.
//!
//! Provides unauthenticated and authenticated HTTP methods targeting either
//! the demo or production Kalshi API environment.

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use super::auth::{self, KalshiCreds, KalshiEnv};

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct KalshiClient {
    http: Client,
    base_url: String,
    /// Present only for authenticated operations.
    creds: Option<KalshiCreds>,
}

impl KalshiClient {
    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Create an unauthenticated client (read-only market data).
    pub fn new(env: &KalshiEnv) -> Result<Self> {
        let base_url =
            std::env::var("KALSHI_API_URL").unwrap_or_else(|_| env.base_url().to_string());
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(15)).build()?,
            base_url,
            creds: None,
        })
    }

    /// Create an authenticated client (trading / portfolio commands).
    ///
    /// Loads RSA credentials from env vars or cache. Fails with a descriptive
    /// error if credentials are not configured.
    pub fn new_authenticated(env: &KalshiEnv) -> Result<Self> {
        let base_url =
            std::env::var("KALSHI_API_URL").unwrap_or_else(|_| env.base_url().to_string());
        let creds = auth::require_creds(env)?;
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(15)).build()?,
            base_url,
            creds: Some(creds),
        })
    }

    // -----------------------------------------------------------------------
    // Request helpers
    // -----------------------------------------------------------------------

    /// Unauthenticated GET request.
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let filtered: Vec<(&str, &str)> = query
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .copied()
            .collect();

        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .get(&url)
            .query(&filtered)
            .send()
            .await
            .context("Kalshi API request failed")?;

        self.handle_response(resp).await
    }

    /// Authenticated GET request (RSA-PSS signed).
    pub async fn auth_get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let creds = self.require_auth()?;
        let sign_path = format!("/trade-api/v2{}", path);
        let (ts, sig) = auth::build_signature(creds, "GET", &sign_path)?;

        let filtered: Vec<(&str, &str)> = query
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .copied()
            .collect();

        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .get(&url)
            .header("KALSHI-ACCESS-KEY", &creds.key_id)
            .header("KALSHI-ACCESS-TIMESTAMP", &ts)
            .header("KALSHI-ACCESS-SIGNATURE", &sig)
            .query(&filtered)
            .send()
            .await
            .context("Kalshi API request failed")?;

        self.handle_response(resp).await
    }

    /// Authenticated POST request (RSA-PSS signed).
    pub async fn auth_post(&self, path: &str, body: &Value) -> Result<Value> {
        let creds = self.require_auth()?;
        let sign_path = format!("/trade-api/v2{}", path);
        let (ts, sig) = auth::build_signature(creds, "POST", &sign_path)?;

        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .post(&url)
            .header("KALSHI-ACCESS-KEY", &creds.key_id)
            .header("KALSHI-ACCESS-TIMESTAMP", &ts)
            .header("KALSHI-ACCESS-SIGNATURE", &sig)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Kalshi API request failed")?;

        self.handle_response(resp).await
    }

    /// Authenticated DELETE request (RSA-PSS signed).
    pub async fn auth_delete(&self, path: &str) -> Result<Value> {
        let creds = self.require_auth()?;
        let sign_path = format!("/trade-api/v2{}", path);
        let (ts, sig) = auth::build_signature(creds, "DELETE", &sign_path)?;

        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .delete(&url)
            .header("KALSHI-ACCESS-KEY", &creds.key_id)
            .header("KALSHI-ACCESS-TIMESTAMP", &ts)
            .header("KALSHI-ACCESS-SIGNATURE", &sig)
            .send()
            .await
            .context("Kalshi API request failed")?;

        self.handle_response(resp).await
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    fn require_auth(&self) -> Result<&KalshiCreds> {
        self.creds.as_ref().context(
            "KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM not set — required for trading commands",
        )
    }

    async fn handle_response(&self, resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();
        match status.as_u16() {
            429 => bail!("Rate limited — please retry after a moment"),
            401 => bail!(
                "Kalshi authentication failed — verify KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM"
            ),
            403 => bail!("Kalshi access denied — check KYC status and account permissions"),
            s if !status.is_success() => {
                let body = resp.text().await.unwrap_or_default();
                bail!("Kalshi API error (HTTP {}): {}", s, body);
            }
            _ => {}
        }
        let body: Value = resp
            .json()
            .await
            .context("Failed to parse Kalshi API response as JSON")?;
        Ok(body)
    }
}
