//! Kalshi market data helpers.
//!
//! Kalshi prices are expressed as integer cents (1–99), where 50 = 50% probability.
//! This module provides conversion utilities and field extraction helpers.

use serde_json::Value;

// ---------------------------------------------------------------------------
// Price conversions
// ---------------------------------------------------------------------------

/// Convert Kalshi integer cent price (1–99) to a 0–1 probability float.
///
/// Examples:
/// - `50` → `0.50`  (50 % probability)
/// - `72` → `0.72`
pub fn cents_to_probability(cents: i64) -> f64 {
    cents as f64 / 100.0
}

/// Convert a 0–1 probability float to Kalshi integer cent price (1–99).
///
/// The result is clamped to [1, 99] to stay within valid Kalshi range.
pub fn probability_to_cents(prob: f64) -> i64 {
    let raw = (prob * 100.0).round() as i64;
    raw.clamp(1, 99)
}

// ---------------------------------------------------------------------------
// Field extraction helpers
// ---------------------------------------------------------------------------

/// Extract the base path of a full Kalshi API URL for signing.
///
/// Removes query string: `/trade-api/v2/markets?status=open` → `/trade-api/v2/markets`
pub fn path_without_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

/// Derive a human-readable yes/no price display from a Kalshi market object.
///
/// Returns a JSON object: `{ "yes_price": 0.65, "no_price": 0.35 }`
pub fn extract_prices(market: &Value) -> Value {
    let yes_cents = market["yes_bid"].as_i64().unwrap_or(0);
    let no_cents = market["no_bid"].as_i64().unwrap_or(0);
    serde_json::json!({
        "yes_price": cents_to_probability(yes_cents),
        "no_price":  cents_to_probability(no_cents),
        "yes_cents": yes_cents,
        "no_cents":  no_cents,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_to_probability_midpoint() {
        assert!((cents_to_probability(50) - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn cents_to_probability_extremes() {
        assert!((cents_to_probability(1) - 0.01).abs() < f64::EPSILON);
        assert!((cents_to_probability(99) - 0.99).abs() < f64::EPSILON);
    }

    #[test]
    fn probability_to_cents_roundtrip() {
        for pct in [10, 25, 50, 75, 90] {
            let prob = pct as f64 / 100.0;
            assert_eq!(probability_to_cents(prob), pct);
        }
    }

    #[test]
    fn probability_to_cents_clamped() {
        assert_eq!(probability_to_cents(0.0), 1);
        assert_eq!(probability_to_cents(1.0), 99);
    }

    #[test]
    fn path_without_query_strips_correctly() {
        assert_eq!(
            path_without_query("/trade-api/v2/markets?status=open&limit=10"),
            "/trade-api/v2/markets"
        );
        assert_eq!(
            path_without_query("/trade-api/v2/portfolio/balance"),
            "/trade-api/v2/portfolio/balance"
        );
    }
}
