//! TVL tracking and gas circuit breaker for the auto-rebalance strategy.

use std::collections::HashMap;

use anyhow::Result;

use crate::strategy::auto_rebalance::state::{StateData, TvlEntryState};
use crate::strategy::auto_rebalance::yield_monitor::{self, Protocol, YieldSnapshot};

// ── Constants ──────────────────────────────────────────────────────

/// Emergency withdrawal if TVL drops more than 30%.
const TVL_DROP_THRESHOLD: f64 = 30.0;

/// Alert (non-blocking) if TVL drops more than 20%.
const TVL_ALERT_THRESHOLD: f64 = 20.0;

/// Need at least 3 data points to confirm a trend.
const MIN_DATA_POINTS: usize = 3;

/// Prune entries older than 24 hours.
const MAX_ENTRY_AGE_SECS: u64 = 86400;

/// Maximum TVL history entries per protocol (24h at ~15min intervals).
const MAX_TVL_ENTRIES: usize = 96;

// ── Data models ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TvlEntry {
    pub tvl_usd: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct ProtocolHealth {
    pub protocol: Protocol,
    pub tvl_usd: f64,
    pub tvl_24h_change_percent: f64,
    pub is_healthy: bool,
    pub alerts: Vec<String>,
}

// ── SafetyMonitor ──────────────────────────────────────────────────

pub struct SafetyMonitor {
    tvl_history: HashMap<Protocol, Vec<TvlEntry>>,
}

impl SafetyMonitor {
    /// Create a new monitor with empty history.
    pub fn new() -> Self {
        Self {
            tvl_history: HashMap::new(),
        }
    }

    /// Add TVL entries from yield snapshots, skipping entries with tvl_usd <= 0.
    /// Caps each protocol's history at MAX_TVL_ENTRIES.
    pub fn update_tvl(&mut self, yields: &[YieldSnapshot]) {
        let now = chrono::Utc::now().timestamp() as u64;
        for y in yields {
            if y.tvl_usd <= 0.0 {
                continue;
            }
            let entries = self.tvl_history.entry(y.protocol).or_default();
            entries.push(TvlEntry {
                tvl_usd: y.tvl_usd,
                timestamp: now,
            });
            // Cap at MAX_TVL_ENTRIES — remove oldest
            if entries.len() > MAX_TVL_ENTRIES {
                let excess = entries.len() - MAX_TVL_ENTRIES;
                entries.drain(..excess);
            }
        }
    }

    /// Check health of all protocols. If `yields` is None, fetches live data.
    pub async fn check_all_protocols(
        &mut self,
        yields: Option<&[YieldSnapshot]>,
    ) -> Vec<ProtocolHealth> {
        let snapshots = match yields {
            Some(y) => y.to_vec(),
            None => match yield_monitor::fetch_all_yields().await {
                Ok(y) => y,
                Err(e) => {
                    eprintln!("[WARN] Failed to fetch yields for safety check: {e:#}");
                    return Vec::new();
                }
            },
        };

        self.update_tvl(&snapshots);
        self.prune_old_entries();

        let mut results = Vec::new();
        for snap in &snapshots {
            let emergency = self.should_emergency_withdraw(snap.protocol);
            let change = self.tvl_change_percent(snap.protocol);
            let mut alerts = Vec::new();

            if emergency {
                alerts.push(format!(
                    "TVL dropped {:.1}% — exceeds {:.0}% emergency threshold",
                    change.abs(),
                    TVL_DROP_THRESHOLD
                ));
            } else if change < -TVL_ALERT_THRESHOLD {
                // Non-blocking alert: TVL dropped >20% but <30%
                alerts.push(format!(
                    "TVL dropped {:.1}% — exceeds {:.0}% alert threshold (not blocking)",
                    change.abs(),
                    TVL_ALERT_THRESHOLD
                ));
            }

            // Match TS behavior: 20% drop sets is_healthy=false (triggers hold/emergency in decision engine),
            // 30% drop triggers emergency withdrawal.
            let is_healthy = !emergency && change >= -TVL_ALERT_THRESHOLD;

            results.push(ProtocolHealth {
                protocol: snap.protocol,
                tvl_usd: snap.tvl_usd,
                tvl_24h_change_percent: change,
                is_healthy,
                alerts,
            });
        }

        results
    }

    /// Returns true if TVL has dropped more than the threshold.
    /// Uses median of recent entries vs median of earlier entries to avoid
    /// false triggers from API returning different vaults with different TVLs.
    /// Requires at least MIN_DATA_POINTS * 2 non-zero entries.
    pub fn should_emergency_withdraw(&self, protocol: Protocol) -> bool {
        let entries = match self.tvl_history.get(&protocol) {
            Some(e) => e,
            None => return false,
        };

        // Filter out zero-TVL entries
        let valid: Vec<&TvlEntry> = entries.iter().filter(|e| e.tvl_usd > 0.0).collect();

        // Need enough data points to split into two halves
        if valid.len() < MIN_DATA_POINTS * 2 {
            return false;
        }

        // Split into earlier half and recent half
        let mid = valid.len() / 2;
        let earlier = &valid[..mid];
        let recent = &valid[mid..];

        let earlier_median = median_tvl(earlier);
        let recent_median = median_tvl(recent);

        if earlier_median <= 0.0 {
            return false;
        }

        let drop_percent = (earlier_median - recent_median) / earlier_median * 100.0;
        drop_percent > TVL_DROP_THRESHOLD
    }

    /// Remove entries older than 24 hours.
    pub fn prune_old_entries(&mut self) {
        let cutoff = chrono::Utc::now().timestamp() as u64 - MAX_ENTRY_AGE_SECS;
        for entries in self.tvl_history.values_mut() {
            entries.retain(|e| e.timestamp >= cutoff);
        }
    }

    /// Check if gas is currently spiking on Base (backward compat).
    pub async fn is_gas_spiking() -> Result<bool> {
        Self::is_gas_spiking_on(&crate::strategy::auto_rebalance::chains::BASE_CONFIG).await
    }

    /// Check if gas is currently spiking on the given chain.
    pub async fn is_gas_spiking_on(
        config: &crate::strategy::auto_rebalance::chains::AutoRebalanceConfig,
    ) -> Result<bool> {
        let rpc_url = crate::strategy::auto_rebalance::chains::rpc_url_for(config);
        let provider = alloy::providers::ProviderBuilder::new().connect_http(rpc_url.parse()?);
        use alloy::providers::Provider;
        let gas_price = provider.get_gas_price().await?;
        let gwei = gas_price as f64 / 1e9;
        Ok(gwei > config.gas_spike_gwei)
    }

    /// Load TVL history from persisted state data.
    pub fn load_tvl_history(&mut self, data: &StateData) {
        for (protocol_str, entries) in &data.tvl_history {
            let protocol = match protocol_str.to_lowercase().as_str() {
                "aave" | "aave v3" => Protocol::Aave,
                "compound" | "compound v3" => Protocol::Compound,
                "morpho" => Protocol::Morpho,
                _ => continue,
            };
            let tvl_entries: Vec<TvlEntry> = entries
                .iter()
                .map(|e| TvlEntry {
                    tvl_usd: e.tvl_usd,
                    timestamp: e.timestamp,
                })
                .collect();
            self.tvl_history
                .entry(protocol)
                .or_default()
                .extend(tvl_entries);
        }
    }

    /// Export TVL history for persistence.
    pub fn get_tvl_history(&self) -> HashMap<String, Vec<TvlEntryState>> {
        self.tvl_history
            .iter()
            .map(|(protocol, entries)| {
                let key = match protocol {
                    Protocol::Aave => "aave".to_string(),
                    Protocol::Compound => "compound".to_string(),
                    Protocol::Morpho => "morpho".to_string(),
                };
                let state_entries: Vec<TvlEntryState> = entries
                    .iter()
                    .map(|e| TvlEntryState {
                        tvl_usd: e.tvl_usd,
                        timestamp: e.timestamp,
                    })
                    .collect();
                (key, state_entries)
            })
            .collect()
    }

    /// Compute TVL change percent using median of earlier vs recent entries.
    fn tvl_change_percent(&self, protocol: Protocol) -> f64 {
        let entries = match self.tvl_history.get(&protocol) {
            Some(e) => e,
            None => return 0.0,
        };
        let valid: Vec<&TvlEntry> = entries.iter().filter(|e| e.tvl_usd > 0.0).collect();
        if valid.len() < 4 {
            return 0.0;
        }
        let mid = valid.len() / 2;
        let earlier_median = median_tvl(&valid[..mid]);
        let recent_median = median_tvl(&valid[mid..]);
        if earlier_median <= 0.0 {
            return 0.0;
        }
        (recent_median - earlier_median) / earlier_median * 100.0
    }
}

/// Compute median TVL from a slice of entries.
fn median_tvl(entries: &[&TvlEntry]) -> f64 {
    if entries.is_empty() {
        return 0.0;
    }
    let mut vals: Vec<f64> = entries.iter().map(|e| e.tvl_usd).collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = vals.len() / 2;
    #[allow(clippy::manual_is_multiple_of)]
    if vals.len() % 2 == 0 {
        (vals[mid - 1] + vals[mid]) / 2.0
    } else {
        vals[mid]
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        chrono::Utc::now().timestamp() as u64
    }

    #[test]
    fn tvl_drop_below_threshold_triggers_emergency() {
        let mut monitor = SafetyMonitor::new();
        let t = now();
        // 6 entries: earlier median ~950, recent median ~500 => ~47% drop
        monitor.tvl_history.insert(
            Protocol::Aave,
            vec![
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 6,
                },
                TvlEntry {
                    tvl_usd: 950.0,
                    timestamp: t - 3600 * 5,
                },
                TvlEntry {
                    tvl_usd: 900.0,
                    timestamp: t - 3600 * 4,
                },
                TvlEntry {
                    tvl_usd: 600.0,
                    timestamp: t - 3600 * 2,
                },
                TvlEntry {
                    tvl_usd: 500.0,
                    timestamp: t - 3600,
                },
                TvlEntry {
                    tvl_usd: 400.0,
                    timestamp: t,
                },
            ],
        );
        assert!(monitor.should_emergency_withdraw(Protocol::Aave));
    }

    #[test]
    fn tvl_stable_no_emergency() {
        let mut monitor = SafetyMonitor::new();
        let t = now();
        // 6 entries: earlier median ~950, recent median ~880 => ~7% drop (under 30%)
        monitor.tvl_history.insert(
            Protocol::Compound,
            vec![
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 6,
                },
                TvlEntry {
                    tvl_usd: 950.0,
                    timestamp: t - 3600 * 5,
                },
                TvlEntry {
                    tvl_usd: 920.0,
                    timestamp: t - 3600 * 4,
                },
                TvlEntry {
                    tvl_usd: 900.0,
                    timestamp: t - 3600 * 2,
                },
                TvlEntry {
                    tvl_usd: 880.0,
                    timestamp: t - 3600,
                },
                TvlEntry {
                    tvl_usd: 860.0,
                    timestamp: t,
                },
            ],
        );
        assert!(!monitor.should_emergency_withdraw(Protocol::Compound));
    }

    #[test]
    fn tvl_too_few_data_points_no_emergency() {
        let mut monitor = SafetyMonitor::new();
        let t = now();
        // Only 4 entries — need MIN_DATA_POINTS * 2 = 6
        monitor.tvl_history.insert(
            Protocol::Morpho,
            vec![
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 3,
                },
                TvlEntry {
                    tvl_usd: 900.0,
                    timestamp: t - 3600 * 2,
                },
                TvlEntry {
                    tvl_usd: 100.0,
                    timestamp: t - 3600,
                },
                TvlEntry {
                    tvl_usd: 50.0,
                    timestamp: t,
                },
            ],
        );
        assert!(!monitor.should_emergency_withdraw(Protocol::Morpho));
    }

    #[test]
    fn tvl_zero_entry_skipped() {
        let mut monitor = SafetyMonitor::new();
        let t = now();
        // 8 entries but 4 are zero — only 4 valid, need 6
        monitor.tvl_history.insert(
            Protocol::Aave,
            vec![
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 7,
                },
                TvlEntry {
                    tvl_usd: 0.0,
                    timestamp: t - 3600 * 6,
                },
                TvlEntry {
                    tvl_usd: 900.0,
                    timestamp: t - 3600 * 5,
                },
                TvlEntry {
                    tvl_usd: 0.0,
                    timestamp: t - 3600 * 4,
                },
                TvlEntry {
                    tvl_usd: 0.0,
                    timestamp: t - 3600 * 2,
                },
                TvlEntry {
                    tvl_usd: 500.0,
                    timestamp: t - 3600,
                },
                TvlEntry {
                    tvl_usd: 0.0,
                    timestamp: t - 1800,
                },
                TvlEntry {
                    tvl_usd: 400.0,
                    timestamp: t,
                },
            ],
        );
        // Only 4 non-zero entries, under MIN_DATA_POINTS * 2 = 6
        assert!(!monitor.should_emergency_withdraw(Protocol::Aave));
    }

    #[test]
    fn tvl_spike_in_single_entry_does_not_trigger() {
        let mut monitor = SafetyMonitor::new();
        let t = now();
        // Earlier: 1000, 1000, 10000 (one spike). Recent: 1000, 1000, 1000
        // Earlier median = 1000, recent median = 1000 => 0% drop
        monitor.tvl_history.insert(
            Protocol::Morpho,
            vec![
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 6,
                },
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 5,
                },
                TvlEntry {
                    tvl_usd: 10000.0,
                    timestamp: t - 3600 * 4,
                },
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600 * 2,
                },
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 3600,
                },
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t,
                },
            ],
        );
        assert!(!monitor.should_emergency_withdraw(Protocol::Morpho));
    }

    #[test]
    fn prune_old_entries_removes_stale() {
        let mut monitor = SafetyMonitor::new();
        let t = now();
        monitor.tvl_history.insert(
            Protocol::Aave,
            vec![
                // Old entry: 48h ago — should be pruned
                TvlEntry {
                    tvl_usd: 1000.0,
                    timestamp: t - 86400 * 2,
                },
                // Recent entry: 1h ago — should be kept
                TvlEntry {
                    tvl_usd: 900.0,
                    timestamp: t - 3600,
                },
            ],
        );
        monitor.prune_old_entries();
        let entries = monitor.tvl_history.get(&Protocol::Aave).unwrap();
        assert_eq!(entries.len(), 1);
        assert!((entries[0].tvl_usd - 900.0).abs() < f64::EPSILON);
    }

    #[test]
    fn update_tvl_from_yields() {
        let mut monitor = SafetyMonitor::new();
        let yields = vec![
            YieldSnapshot {
                protocol: Protocol::Aave,
                apy: 5.0,
                tvl_usd: 1_000_000.0,
                source: "test".to_string(),
                vault_address: None,
            },
            YieldSnapshot {
                protocol: Protocol::Compound,
                apy: 4.0,
                tvl_usd: 500_000.0,
                source: "test".to_string(),
                vault_address: None,
            },
            YieldSnapshot {
                protocol: Protocol::Morpho,
                apy: 3.0,
                tvl_usd: 0.0, // should be skipped
                source: "test".to_string(),
                vault_address: None,
            },
        ];
        monitor.update_tvl(&yields);

        assert_eq!(monitor.tvl_history.get(&Protocol::Aave).unwrap().len(), 1);
        assert_eq!(
            monitor.tvl_history.get(&Protocol::Compound).unwrap().len(),
            1
        );
        assert!(monitor.tvl_history.get(&Protocol::Morpho).is_none());

        let aave_entry = &monitor.tvl_history[&Protocol::Aave][0];
        assert!((aave_entry.tvl_usd - 1_000_000.0).abs() < f64::EPSILON);
    }
}
