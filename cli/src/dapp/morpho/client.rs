//! Morpho Protocol GraphQL client.
//!
//! All Morpho API endpoints are public and require no authentication.
//! The GraphQL API is available at <https://api.morpho.org/graphql>.
//! Rate limit: 5,000 requests per 5 minutes.

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

const GRAPHQL_URL: &str = "https://api.morpho.org/graphql";

pub struct MorphoClient {
    http: Client,
    base_url: String,
}

impl MorphoClient {
    /// Create a new unauthenticated Morpho client.
    pub fn new() -> Result<Self> {
        let base_url = std::env::var("MORPHO_API_URL").unwrap_or_else(|_| GRAPHQL_URL.to_string());
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(30)).build()?,
            base_url,
        })
    }

    /// Execute a GraphQL query and return the `data` field from the response.
    pub async fn query(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({
            "query": query,
            "variables": variables,
        });

        let resp = self
            .http
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Morpho API request failed")?;

        let status = resp.status();
        if status.as_u16() == 429 {
            bail!(
                "Morpho API rate limited — max 5,000 requests per 5 minutes. Please retry after a moment."
            );
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Morpho API error (HTTP {}): {}", status, text);
        }

        let json: Value = resp
            .json()
            .await
            .context("Failed to parse Morpho API response as JSON")?;

        if let Some(errors) = json.get("errors") {
            bail!("Morpho GraphQL error: {}", errors);
        }

        Ok(json["data"].clone())
    }
}
