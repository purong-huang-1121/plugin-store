use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::Value;

const GAMMA_BASE_URL: &str = "https://gamma-api.polymarket.com";

pub struct GammaClient {
    http: Client,
    base_url: String,
}

impl GammaClient {
    pub fn new() -> Result<Self> {
        let base_url =
            std::env::var("POLYMARKET_GAMMA_URL").unwrap_or_else(|_| GAMMA_BASE_URL.to_string());
        Ok(Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()?,
            base_url,
        })
    }

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
            .context("Gamma API request failed")?;

        let status = resp.status();
        if status.as_u16() == 429 {
            bail!("Rate limited — retry with backoff");
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Gamma API error (HTTP {}): {}", status.as_u16(), body);
        }

        let body: Value = resp
            .json()
            .await
            .context("failed to parse Gamma response")?;
        Ok(body)
    }
}
