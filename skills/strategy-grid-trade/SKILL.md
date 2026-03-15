---
name: strategy-grid-trade
description: "Use when the user asks about grid trading, ETH/USDC bot, automated trading on Base, grid bot status, trade history, PnL report, or mentions running/stopping/monitoring the grid bot. Covers: grid tick execution, start/stop daemon, status/report/history, market analysis, deposit tracking, retry failed trades. Do NOT use for manual token swaps or DeFi lending — use strategy-auto-rebalance for yield optimization."
license: Apache-2.0
metadata:
  author: okx
  version: "2.0.0"
  homepage: "https://web3.okx.com"
---

# ETH/USDC Grid Trading Bot

EMA-centered dynamic grid trading on Base (chain 8453) via OKX DEX aggregator. Built into the `plugin-store` CLI.

## Pre-flight Checks

Every time before running any `plugin-store` command, always follow these steps in order. Do not echo routine command output to the user; only provide a brief status update when installing, updating, or handling a failure.

1. **Confirm installed**: Run `which plugin-store`. If not found, install it:
   ```bash
   curl -sSL https://raw.githubusercontent.com/okx/plugin-store/main/install.sh | sh
   ```

2. **Check for updates**: Read `~/.plugin-store/last_check` and compare it with the current timestamp:
   ```bash
   cached_ts=$(cat ~/.plugin-store/last_check 2>/dev/null || true)
   now=$(date +%s)
   ```
   - If `cached_ts` is non-empty and `(now - cached_ts) < 43200` (12 hours), skip the update.
   - Otherwise, run the installer to check for updates.

## Authentication

Requires two sets of credentials:

**OKX API (for price quotes and swap execution):**
```bash
OKX_API_KEY=...
OKX_SECRET_KEY=...
OKX_PASSPHRASE=...
```

**EVM Wallet (for on-chain signing):**
```bash
EVM_PRIVATE_KEY=0x...   # Base wallet with ETH + USDC
```

**Optional:**
```bash
BASE_RPC_URL=...        # Custom Base RPC (default: public endpoint)
TELEGRAM_BOT_TOKEN=...  # Telegram notifications on trade/error
TELEGRAM_CHAT_ID=...    # Telegram chat ID for alerts
```

## Before Starting the Bot

**IMPORTANT:** Before running `plugin-store grid start`, you MUST:

1. Run `plugin-store grid config` to show the user their current parameters
2. Present the parameters in a readable table and ask if they want to adjust any
3. If the user wants to change a parameter, use `plugin-store grid set --key <name> --value <value>`
4. Parameters are saved to `grid_config.json` in the same directory as the `plugin-store` executable and persist across restarts — no need to set them every time

Example flow:
```bash
# Show current config
plugin-store grid config

# User wants to change tick interval
plugin-store grid set --key tick_interval_secs --value 120

# User wants wider position limits
plugin-store grid set --key position_max_pct --value 70
plugin-store grid set --key position_min_pct --value 30

# Now start
plugin-store grid start
```

## Quickstart

```bash
# Check market conditions
plugin-store grid analyze

# View current state and PnL
plugin-store grid status

# Run a single tick (fetch price, detect crossing, trade if needed)
plugin-store grid tick

# Start continuous bot (tick every 60 seconds)
plugin-store grid start

# Stop running bot
plugin-store grid stop
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store grid tick` | Yes | Execute one grid cycle |
| 2 | `plugin-store grid start` | Yes | Start foreground bot loop (60s ticks) |
| 3 | `plugin-store grid stop` | No | Stop running bot via PID file |
| 4 | `plugin-store grid status` | No | Show grid state, balances, PnL |
| 5 | `plugin-store grid report` | No | Detailed PnL and performance stats |
| 6 | `plugin-store grid history` | No | Show trade history |
| 7 | `plugin-store grid reset --force` | No | Clear all grid state |
| 8 | `plugin-store grid retry` | Yes | Re-execute last failed trade |
| 9 | `plugin-store grid analyze` | Yes | Market analysis (EMA, volatility, trend) |
| 10 | `plugin-store grid deposit` | No | Record manual deposit/withdrawal |
| 11 | `plugin-store grid config` | No | Show current bot configuration |
| 12 | `plugin-store grid set` | No | Set a config parameter |

## Core Algorithm

```
1. Fetch ETH price (OKX DEX quote API)
2. Read on-chain balances (ETH + USDC on Base)
3. Check circuit breaker (consecutive errors)
4. Recalibrate grid if needed (price breakout / vol shift / age)
5. Map price → grid level
6. If level changed:
   a. Direction: BUY if level dropped, SELL if rose
   b. Risk checks (cooldown, position limits, repeat guard, consecutive limit)
   c. Calculate trade size (% of portfolio, capped)
   d. Execute swap via OKX DEX aggregator
   e. Update level ONLY on success
7. Save state and report
```

## Tunable Parameters

Parameters are persisted at `grid_config.json` in the same directory as the `plugin-store` executable. View with `plugin-store grid config`, modify with `plugin-store grid set --key <key> --value <value>`. Changes take effect on next tick (no rebuild needed). If no config file exists, defaults below are used.

The **Key** column shows the exact key name to use with `plugin-store grid set`.

### Grid Structure

| Key | Default | Description |
|---|---|---|
| `grid_levels` | `6` | Number of grid levels |
| `ema_period` | `20` | EMA lookback periods for grid center calculation |
| `volatility_multiplier` | `2.5` | Grid width = multiplier × stddev |
| `grid_recalibrate_hours` | `12.0` | Max hours before forced recalibration |
| `tick_interval_secs` | `60` | Seconds between each tick cycle (restart bot to apply) |

### Adaptive Step Sizing

Step scales linearly with real-time volatility:

```
step = (volatility_multiplier × stddev) / (grid_levels / 2)
step = clamp(step, price × step_min_pct, price × step_max_pct)
step = max(step, step_floor)
```

| Key | Default | Description |
|---|---|---|
| `step_min_pct` | `0.008` | Step floor (0.8% of price) |
| `step_max_pct` | `0.060` | Step cap (6% of price) |
| `step_floor` | `5.0` | Absolute minimum step in USD |

### Trade Sizing

| Key | Default | Description |
|---|---|---|
| `max_trade_pct` | `0.12` | Max 12% of portfolio per trade |
| `min_trade_usd` | `5.0` | Minimum trade size in USD |
| `slippage_pct` | `1` | Slippage tolerance % for DEX swap. Increase to 2-3 if trades revert |
| `gas_reserve_eth` | `0.003` | ETH reserved for gas, not available for SELL |

### Risk Controls

| Key | Default | Description |
|---|---|---|
| `min_trade_interval` | `1800` | 30min cooldown between same-direction trades |
| `max_same_dir_trades` | `3` | Max consecutive same-direction trades |
| `position_max_pct` | `65.0` | Block BUY when ETH% exceeds this |
| `position_min_pct` | `35.0` | Block SELL when ETH% drops below this |
| `max_consecutive_errors` | `5` | Circuit breaker threshold |
| `cooldown_after_errors` | `3600` | Seconds cooldown after circuit breaker trips |

### Common Parameter Adjustments

**Slippage (trades reverting on-chain):**
```bash
plugin-store grid set --key slippage_pct --value 2
```

**Wider position limits (allow more one-sided exposure):**
```bash
plugin-store grid set --key position_max_pct --value 75
plugin-store grid set --key position_min_pct --value 25
```

**Faster/slower tick interval:**
```bash
plugin-store grid set --key tick_interval_secs --value 120   # 2 minutes
```
Note: Restart the bot after changing `tick_interval_secs`.

**Larger trade sizes:**
```bash
plugin-store grid set --key max_trade_pct --value 0.20       # 20% per trade
plugin-store grid set --key min_trade_usd --value 10         # $10 minimum
```

## CLI Command Reference

### plugin-store grid tick

Execute one grid cycle: fetch price, detect grid crossing, execute trade if needed.

**Output actions:**
- `grid_calibrated` — Grid was recalibrated (first tick or recalibration trigger)
- `no_crossing` — Price stayed within same grid level
- `trade_executed` — Swap executed successfully
- `trade_failed` — Swap attempted but failed (retriable)
- `blocked` — Risk check prevented trade (cooldown, position limit, etc.)
- `skipped` — Trade amount below minimum

### plugin-store grid start

Start the bot in foreground, executing `tick` every 60 seconds. Creates a PID file at `~/.plugin-store/grid_bot.pid`. Use Ctrl+C or `grid stop` to terminate.

### plugin-store grid stop

Stop a running bot by sending SIGTERM to the process in the PID file.

### plugin-store grid status

Show current grid state, balances, PnL overview, and whether the bot is running.

### plugin-store grid report

Detailed performance report: success rate, buy/sell counts, total volume, grid profit, deposits, and portfolio PnL.

### plugin-store grid history [--limit N]

Show trade history (default: last 50 trades). Each trade includes direction, price, amount, tx hash, and grid levels.

### plugin-store grid reset --force

Delete all grid state. Requires `--force` flag for safety.

### plugin-store grid retry

Re-execute the last failed trade. Validates that price hasn't moved >5% since failure.

### plugin-store grid analyze

Market analysis showing current price, EMA-20, volatility, trend direction, and grid utilization.

### plugin-store grid deposit --amount N [--note "..."]

Record a manual deposit (positive) or withdrawal (negative) for accurate PnL tracking.

### plugin-store grid config

Show all current bot parameters and their values. Indicates whether a custom config file exists.

### plugin-store grid set --key NAME --value VALUE

Set a single parameter. Saved to `grid_config.json` in the executable's directory. Takes effect on next tick (restart bot if already running to apply tick_interval changes).

Available keys: `grid_levels`, `tick_interval_secs`, `max_trade_pct`, `min_trade_usd`, `slippage_pct`, `ema_period`, `volatility_multiplier`, `step_min_pct`, `step_max_pct`, `step_floor`, `grid_recalibrate_hours`, `min_trade_interval`, `max_same_dir_trades`, `position_max_pct`, `position_min_pct`, `gas_reserve_eth`, `max_consecutive_errors`, `cooldown_after_errors`.

## Level Update Rule (Critical)

| Outcome | Update level? | Rationale |
|---|---|---|
| Trade succeeded | Yes | Grid crossing consumed |
| Trade failed | No | Retry on next tick |
| Trade skipped (cooldown/limit) | No | Don't lose the crossing |

## PnL Tracking

```
total_pnl    = current_portfolio_value - initial_value - deposits
grid_profit += estimated spread capture on SELL trades
```

## State Persistence

State is stored at `~/.plugin-store/grid_state.json` with atomic writes (write to .tmp, rename). Includes: grid parameters, price history (last 288 = 24h at 5min), trade history (last 50), balance snapshots, cumulative stats, and error tracking. PID file at `~/.plugin-store/grid_bot.pid`.

## Cross-Skill Workflows

| Need | Skill |
|---|---|
| USDC yield optimization (Aave/Compound/Morpho) | `strategy-auto-rebalance` |
| Aave V3 supply/withdraw/markets | `dapp-aave` |
| Morpho vault operations | `dapp-morpho` (CLI: `plugin-store morpho`) |
| Hyperliquid perpetual trading | `dapp-hyperliquid` |
| Prediction markets | `dapp-polymarket` / `dapp-kalshi` |

## Edge Cases

| Scenario | Behavior |
|---|---|
| First tick (no grid) | Calibrates grid from current price + history, sets initial level |
| Price exits grid range | Triggers recalibration (breakout detected) |
| Volatility shifts >30% | Triggers recalibration |
| Grid age > 12 hours | Triggers recalibration |
| 5 consecutive errors | Circuit breaker trips, 1-hour cooldown |
| Trade amount < $5 | Skipped (below minimum) |
| ETH balance < 0.003 | Gas reserve protected, SELL blocked |
| No EVM_PRIVATE_KEY | Error on tick/start/retry commands |
| Bot already running | `start` rejects with existing PID warning |
| No running bot | `stop` returns error |
| Reset without --force | Returns error, requires confirmation |

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Trade reverts on-chain (`trade_failed`) | Slippage too low for the DEX route | `plugin-store grid set --key slippage_pct --value 2` (or 3 for volatile periods) |
| RPC 429 / rate limit errors | Public Base RPC rate limited | Set `BASE_RPC_URL` env var to a private RPC endpoint |
| Circuit breaker trips (5 errors) | Repeated failures (RPC, slippage, gas) | Check logs, fix root cause, then wait 1h or `plugin-store grid reset --force` |
| Bot not trading (no_crossing) | Price within same grid level | Normal — bot only trades when price crosses a grid boundary |
| Trade blocked: position limit | ETH% too high/low | Adjust `position_max_pct` / `position_min_pct` or manually rebalance |
| Trade blocked: cooldown | Same-direction trade too soon (30min default) | Lower `min_trade_interval` if you want faster trading |
| Trade blocked: repeat guard | Same crossing as last trade | Normal — prevents oscillation. Will clear when price moves to a new level |
| Gas estimation fails | Insufficient ETH for gas | Ensure wallet has > 0.003 ETH (adjust via `gas_reserve_eth`) |

## Anti-Patterns

| Pattern | Problem |
|---|---|
| Recalibrate every tick | Grid oscillates, no stable levels |
| Update level on failure/skip | Silently loses grid crossings |
| No position limits | Trending market → 100% one-sided |
| Fixed step in volatile market | Too small → over-trades; too large → never triggers |
| `sell - buy` as PnL | Net cash flow ≠ profit |
| No cooldown | Rapid swings cause burst of trades eating slippage |
