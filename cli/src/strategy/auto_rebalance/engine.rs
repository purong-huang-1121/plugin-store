//! Rule-based decision engine for yield rebalancing.

use super::safety_monitor::ProtocolHealth;
use super::yield_monitor::{Protocol, YieldSnapshot};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "decision")]
pub enum Decision {
    Hold {
        reason: String,
    },
    Rebalance {
        from: Protocol,
        to: Protocol,
        yield_spread: f64,
        break_even_days: f64,
    },
    EmergencyWithdraw {
        reason: String,
    },
}

pub struct EngineConfig {
    pub min_yield_spread: f64,
    pub max_break_even_days: u32,
    pub max_gas_cost_usd: f64,
    pub min_rebalance_interval_secs: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            min_yield_spread: 0.5,
            max_break_even_days: 7,
            max_gas_cost_usd: 0.50,
            min_rebalance_interval_secs: 86400, // 24 hours
        }
    }
}

/// Decide whether to rebalance based on yield snapshots.
///
/// Returns Hold or Rebalance with details.
pub fn decide(
    yields: &[YieldSnapshot],
    current_protocol: Option<Protocol>,
    capital_usd: f64,
    gas_cost_usd: f64,
    config: &EngineConfig,
) -> Decision {
    if yields.is_empty() {
        return Decision::Hold {
            reason: "No yield data available".to_string(),
        };
    }

    // yields are sorted by APY descending (from yield_monitor)
    let best = &yields[0];

    // If no current position, recommend the best
    let current = match current_protocol {
        Some(p) => p,
        None => {
            return Decision::Rebalance {
                from: best.protocol, // No source, initial deposit
                to: best.protocol,
                yield_spread: best.apy,
                break_even_days: 0.0,
            };
        }
    };

    // Already in best protocol
    if current == best.protocol {
        return Decision::Hold {
            reason: format!(
                "Already in best protocol ({} @ {:.2}% APY)",
                best.protocol, best.apy
            ),
        };
    }

    // Find current protocol's APY
    let current_apy = yields
        .iter()
        .find(|y| y.protocol == current)
        .map(|y| y.apy)
        .unwrap_or(0.0);

    let spread = best.apy - current_apy;

    // Check minimum spread
    if spread < config.min_yield_spread {
        return Decision::Hold {
            reason: format!(
                "Yield spread {:.2}% below minimum {:.2}%",
                spread, config.min_yield_spread
            ),
        };
    }

    // Check gas cost
    if gas_cost_usd > config.max_gas_cost_usd {
        return Decision::Hold {
            reason: format!(
                "Gas cost ${:.4} exceeds max ${:.2}",
                gas_cost_usd, config.max_gas_cost_usd
            ),
        };
    }

    // Break-even calculation
    let daily_yield_diff = capital_usd * (spread / 100.0) / 365.0;
    let break_even_days = if daily_yield_diff > 0.0 {
        gas_cost_usd / daily_yield_diff
    } else {
        f64::INFINITY
    };

    if break_even_days > config.max_break_even_days as f64 {
        return Decision::Hold {
            reason: format!(
                "Break-even {:.1} days exceeds max {} days",
                break_even_days, config.max_break_even_days
            ),
        };
    }

    Decision::Rebalance {
        from: current,
        to: best.protocol,
        yield_spread: spread,
        break_even_days,
    }
}

/// Decide with safety checks: emergency withdrawal, gas spike detection, then normal rules.
pub fn decide_with_safety(
    yields: &[YieldSnapshot],
    current_protocol: Option<Protocol>,
    capital_usd: f64,
    gas_cost_usd: f64,
    config: &EngineConfig,
    health: &[ProtocolHealth],
    gas_spiking: bool,
) -> Decision {
    // Check emergency for current protocol
    if let Some(current) = current_protocol {
        if let Some(h) = health.iter().find(|h| h.protocol == current) {
            if !h.is_healthy {
                return Decision::EmergencyWithdraw {
                    reason: h.alerts.join(", "),
                };
            }
        }
    }
    // Gas spike → hold
    if gas_spiking {
        return Decision::Hold {
            reason: "Gas spike detected, pausing rebalance".into(),
        };
    }
    // Delegate to existing rule engine
    let decision = decide(yields, current_protocol, capital_usd, gas_cost_usd, config);

    // Before executing a rebalance, verify the target protocol is healthy (matches TS isSafeToRebalance)
    if let Decision::Rebalance { to, .. } = &decision {
        if let Some(target_health) = health.iter().find(|h| h.protocol == *to) {
            if !target_health.is_healthy {
                return Decision::Hold {
                    reason: format!(
                        "Target protocol {} is unhealthy: {}",
                        to,
                        target_health.alerts.join(", ")
                    ),
                };
            }
        }
    }

    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emergency_withdraw_when_protocol_unhealthy() {
        let yields = vec![YieldSnapshot {
            protocol: Protocol::Aave,
            apy: 5.0,
            tvl_usd: 1_000_000.0,
            source: "test".into(),
            vault_address: None,
        }];
        let health = vec![ProtocolHealth {
            protocol: Protocol::Aave,
            tvl_usd: 500_000.0,
            tvl_24h_change_percent: -35.0,
            is_healthy: false,
            alerts: vec!["TVL dropped 35.0% — exceeds 30% threshold".into()],
        }];
        let config = EngineConfig::default();
        let decision = decide_with_safety(
            &yields,
            Some(Protocol::Aave),
            5000.0,
            0.03,
            &config,
            &health,
            false,
        );
        match decision {
            Decision::EmergencyWithdraw { reason } => {
                assert!(reason.contains("TVL dropped"));
            }
            other => panic!("expected EmergencyWithdraw, got {:?}", other),
        }
    }

    #[test]
    fn hold_when_gas_spiking() {
        let yields = vec![
            YieldSnapshot {
                protocol: Protocol::Compound,
                apy: 6.0,
                tvl_usd: 1_000_000.0,
                source: "test".into(),
                vault_address: None,
            },
            YieldSnapshot {
                protocol: Protocol::Aave,
                apy: 3.0,
                tvl_usd: 1_000_000.0,
                source: "test".into(),
                vault_address: None,
            },
        ];
        let health = vec![ProtocolHealth {
            protocol: Protocol::Aave,
            tvl_usd: 1_000_000.0,
            tvl_24h_change_percent: 0.0,
            is_healthy: true,
            alerts: vec![],
        }];
        let config = EngineConfig::default();
        let decision = decide_with_safety(
            &yields,
            Some(Protocol::Aave),
            5000.0,
            0.03,
            &config,
            &health,
            true,
        );
        match decision {
            Decision::Hold { reason } => {
                assert!(reason.contains("Gas spike"));
            }
            other => panic!("expected Hold for gas spike, got {:?}", other),
        }
    }

    #[test]
    fn delegates_to_decide_when_healthy_and_no_gas_spike() {
        let yields = vec![
            YieldSnapshot {
                protocol: Protocol::Compound,
                apy: 6.0,
                tvl_usd: 1_000_000.0,
                source: "test".into(),
                vault_address: None,
            },
            YieldSnapshot {
                protocol: Protocol::Aave,
                apy: 3.0,
                tvl_usd: 1_000_000.0,
                source: "test".into(),
                vault_address: None,
            },
        ];
        let health = vec![
            ProtocolHealth {
                protocol: Protocol::Aave,
                tvl_usd: 1_000_000.0,
                tvl_24h_change_percent: 0.0,
                is_healthy: true,
                alerts: vec![],
            },
            ProtocolHealth {
                protocol: Protocol::Compound,
                tvl_usd: 1_000_000.0,
                tvl_24h_change_percent: 0.0,
                is_healthy: true,
                alerts: vec![],
            },
        ];
        let config = EngineConfig::default();
        let decision = decide_with_safety(
            &yields,
            Some(Protocol::Aave),
            5000.0,
            0.03,
            &config,
            &health,
            false,
        );
        // With 3% spread (6.0 - 3.0), should recommend Rebalance
        match decision {
            Decision::Rebalance { from, to, .. } => {
                assert_eq!(from, Protocol::Aave);
                assert_eq!(to, Protocol::Compound);
            }
            other => panic!("expected Rebalance, got {:?}", other),
        }
    }
}
