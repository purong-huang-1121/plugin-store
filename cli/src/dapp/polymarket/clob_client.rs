use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::Value;

use super::auth::{self, ApiCreds};

const CLOB_BASE_URL: &str = "https://clob.polymarket.com";
const POLYGON_CHAIN_ID: u64 = 137;

pub struct ClobClient {
    http: Client,
    base_url: String,
    creds: Option<AuthState>,
}

struct AuthState {
    creds: ApiCreds,
    address: String,
}

impl ClobClient {
    pub fn new() -> Result<Self> {
        let base_url =
            std::env::var("POLYMARKET_CLOB_URL").unwrap_or_else(|_| CLOB_BASE_URL.to_string());
        Ok(Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()?,
            base_url,
            creds: None,
        })
    }

    /// Create an authenticated client. Loads or derives API credentials.
    pub async fn new_authenticated() -> Result<Self> {
        let base_url =
            std::env::var("POLYMARKET_CLOB_URL").unwrap_or_else(|_| CLOB_BASE_URL.to_string());
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let signing_key = auth::load_signing_key()?;
        let address = auth::address_from_key(&signing_key);

        let creds = match auth::load_api_creds()? {
            Some(c) => c,
            None => {
                let timestamp = chrono::Utc::now().timestamp().to_string();
                let nonce: u64 = 0;
                let signature = auth::sign_clob_auth(
                    &signing_key,
                    &address,
                    &timestamp,
                    nonce,
                    POLYGON_CHAIN_ID,
                )?;

                let resp = http
                    .get(format!(
                        "{}/auth/derive-api-key",
                        base_url.trim_end_matches('/')
                    ))
                    .header("POLY_ADDRESS", &address)
                    .header("POLY_SIGNATURE", &signature)
                    .header("POLY_TIMESTAMP", &timestamp)
                    .header("POLY_NONCE", nonce.to_string())
                    .send()
                    .await
                    .context("derive-api-key request failed")?;

                if !resp.status().is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    bail!("Failed to derive API key: {}", body);
                }

                let body: Value = resp.json().await?;
                let c = ApiCreds {
                    api_key: body["apiKey"].as_str().unwrap_or_default().to_string(),
                    secret: body["secret"].as_str().unwrap_or_default().to_string(),
                    passphrase: body["passphrase"].as_str().unwrap_or_default().to_string(),
                };

                if c.api_key.is_empty() {
                    bail!("API key derivation returned empty credentials");
                }

                auth::save_api_creds(&c)?;
                c
            }
        };

        Ok(Self {
            http,
            base_url,
            creds: Some(AuthState { creds, address }),
        })
    }

    /// Get the wallet address (only available when authenticated).
    pub fn address(&self) -> Option<&str> {
        self.creds.as_ref().map(|a| a.address.as_str())
    }

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
            .context("CLOB API request failed")?;

        self.handle_response(resp).await
    }

    /// Authenticated GET request (L2 HMAC).
    pub async fn auth_get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let auth = self.require_auth()?;

        let filtered: Vec<(&str, &str)> = query
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .copied()
            .collect();

        let query_string = if filtered.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = filtered
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!("?{}", pairs.join("&"))
        };

        let timestamp = chrono::Utc::now().timestamp().to_string();
        let sig = auth::build_hmac_signature(&auth.creds.secret, &timestamp, "GET", path, None)?;

        let request_path = format!("{}{}", path, query_string);
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), request_path);
        let resp = self
            .http
            .get(&url)
            .header("POLY_ADDRESS", &auth.address)
            .header("POLY_SIGNATURE", &sig)
            .header("POLY_TIMESTAMP", &timestamp)
            .header("POLY_API_KEY", &auth.creds.api_key)
            .header("POLY_PASSPHRASE", &auth.creds.passphrase)
            .send()
            .await
            .context("CLOB API request failed")?;

        self.handle_response(resp).await
    }

    /// Authenticated POST request (L2 HMAC).
    pub async fn auth_post(&self, path: &str, body: &Value) -> Result<Value> {
        let auth = self.require_auth()?;

        let body_str = serde_json::to_string(body)?;
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let sig = auth::build_hmac_signature(
            &auth.creds.secret,
            &timestamp,
            "POST",
            path,
            Some(&body_str),
        )?;

        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .post(&url)
            .header("POLY_ADDRESS", &auth.address)
            .header("POLY_SIGNATURE", &sig)
            .header("POLY_TIMESTAMP", &timestamp)
            .header("POLY_API_KEY", &auth.creds.api_key)
            .header("POLY_PASSPHRASE", &auth.creds.passphrase)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .context("CLOB API request failed")?;

        self.handle_response(resp).await
    }

    /// Authenticated DELETE request (L2 HMAC).
    pub async fn auth_delete(&self, path: &str) -> Result<Value> {
        let auth = self.require_auth()?;

        let timestamp = chrono::Utc::now().timestamp().to_string();
        let sig = auth::build_hmac_signature(&auth.creds.secret, &timestamp, "DELETE", path, None)?;

        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .delete(&url)
            .header("POLY_ADDRESS", &auth.address)
            .header("POLY_SIGNATURE", &sig)
            .header("POLY_TIMESTAMP", &timestamp)
            .header("POLY_API_KEY", &auth.creds.api_key)
            .header("POLY_PASSPHRASE", &auth.creds.passphrase)
            .send()
            .await
            .context("CLOB API request failed")?;

        self.handle_response(resp).await
    }

    fn require_auth(&self) -> Result<&AuthState> {
        self.creds
            .as_ref()
            .context("EVM_PRIVATE_KEY not set — required for this command")
    }

    async fn handle_response(&self, resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();
        if status.as_u16() == 429 {
            bail!("Rate limited — retry with backoff");
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("CLOB API error (HTTP {}): {}", status.as_u16(), body);
        }
        let body: Value = resp.json().await.context("failed to parse CLOB response")?;
        Ok(body)
    }
}
