---
name: strategy-ranking-sniper
description: "Use when the user asks about SOL ranking sniper, Solana top token sniping, trending token bot, ranking-based auto-trading, 排行榜狙击, SOL sniper bot, momentum score trading, ranking exit strategy, or wants to run/configure/monitor the ranking sniper bot. Covers: automated sniping of SOL tokens entering the OKX trending ranking, 3-layer safety filter (Slot Guard + Advanced Safety + Holder Risk Scan), momentum scoring (0-125), 6-layer exit system (ranking exit + hard stop + fast stop + trailing + time stop + gradient TP), Telegram notifications, and configurable parameters via JSON config file. Do NOT use for manual token lookup — use okx-dex-token. Do NOT use for grid trading — use strategy-grid-trade. Do NOT use for memepump scanning — use strategy-memepump-scanner."
version: 1.0.0
updated: 2026-03-15
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# SOL Ranking Sniper v1.0.0

Automated Solana token sniper that monitors the OKX DEX trending ranking, applies a 3-layer safety filter with momentum scoring, and executes trades with a 6-layer exit system. Built into the `plugin-store` CLI as a native Rust module.

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

## Skill Routing

- For manual token lookup / analytics -> use `okx-dex-token`
- For DEX swap -> use `okx-dex-swap`
- For token prices / charts -> use `okx-dex-market`
- For wallet balances -> use `okx-wallet-portfolio`
- For grid trading -> use `strategy-grid-trade`
- For DeFi yield -> use `strategy-defi-yield`
- For memepump scanning -> use `strategy-memepump-scanner`

## Architecture Overview

```
+---------------------------------------------------------------------------+
|                     SOL Ranking Sniper v1.0.0                             |
|                                                                           |
|  Ranking API -> Slot Guard -> Advanced Safety -> Holder Risk -> Score ->  |
|  fetch_ranking  (13 checks)   (9 checks)        (3 checks)    0-125     |
|  (Top N by      change/liq/   risk_level/        suspicious/   buy if    |
|   price change)  MC/holders    top10/dev/bundler  phishing      >= 40    |
|                                                                           |
|  -> Buy -> 6-Layer Exit System -> Sell                                    |
|     OKX     ranking_exit / hard_stop / fast_stop /                        |
|     DEX     trailing_stop / time_stop / gradient_tp                       |
+---------------------------------------------------------------------------+
```

## Authentication

Requires two sets of credentials:

**OKX API (for ranking data + swap execution):**
```bash
OKX_API_KEY=...
OKX_SECRET_KEY=...
OKX_PASSPHRASE=...
```

**Solana Wallet (for on-chain signing and swap):**
```bash
SOL_ADDRESS=...          # Solana wallet public address
SOL_PRIVATE_KEY=...      # Solana wallet private key (base58, 32 or 64 bytes)
```

**Optional:**
```bash
TELEGRAM_BOT_TOKEN=...   # Telegram bot token for trade/error notifications
TELEGRAM_CHAT_ID=...     # Telegram chat ID for alerts
```

Telegram credentials can also be set in the config file (`~/.plugin-store/ranking_sniper_config.json`).

## Before Starting the Bot

**IMPORTANT:** Before running `plugin-store ranking-sniper start`, you MUST:

1. Run `plugin-store ranking-sniper config` to show the user their current parameters
2. Present the parameters in a readable table and ask if they want to adjust any
3. If the user wants to change parameters, edit the config file at `~/.plugin-store/ranking_sniper_config.json` directly
4. Parameters are persisted across restarts

Example flow:
```bash
# Show current config
plugin-store ranking-sniper config

# Start with custom budget and per-trade amount
plugin-store ranking-sniper start --budget 1.0 --per-trade 0.1

# Or start in dry-run mode first to observe
plugin-store ranking-sniper start --budget 0.5 --per-trade 0.05 --dry-run
```

## Quickstart

```bash
# View current ranking and market conditions
plugin-store ranking-sniper analyze

# View current state and positions
plugin-store ranking-sniper status

# Run a single tick (scan ranking, check exits, buy if signal)
plugin-store ranking-sniper tick --budget 0.5 --per-trade 0.05

# Dry-run tick (no real swaps)
plugin-store ranking-sniper tick --budget 0.5 --per-trade 0.05 --dry-run

# Start continuous bot (tick every 10 seconds)
plugin-store ranking-sniper start --budget 0.5 --per-trade 0.05

# Stop running bot
plugin-store ranking-sniper stop

# Emergency: sell all open positions
plugin-store ranking-sniper sell-all
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store ranking-sniper tick` | Yes | Execute one tick: fetch ranking, check exits, scan new entries |
| 2 | `plugin-store ranking-sniper start` | Yes | Start foreground bot loop (tick every 10s) |
| 3 | `plugin-store ranking-sniper stop` | No | Stop running bot via PID file |
| 4 | `plugin-store ranking-sniper status` | No | Show current state, positions, and PnL |
| 5 | `plugin-store ranking-sniper report` | No | Detailed PnL and performance report |
| 6 | `plugin-store ranking-sniper history` | No | Show trade history |
| 7 | `plugin-store ranking-sniper reset --force` | No | Clear all state data |
| 8 | `plugin-store ranking-sniper analyze` | No* | Market analysis (current ranking, top tokens) |
| 9 | `plugin-store ranking-sniper test-trade` | Yes | Buy+sell round-trip for a token (dev/debug) |
| 10 | `plugin-store ranking-sniper config` | No | Show all configurable parameters |
| 11 | `plugin-store ranking-sniper sell-all` | Yes | Force-sell all open positions immediately |
| 12 | `plugin-store ranking-sniper sell` | Yes | Sell a specific token by address |

*Analyze requires OKX API keys for ranking data but not SOL_PRIVATE_KEY.

## Core Strategy

### What It Does

1. Every 10 seconds, fetches the top N Solana tokens by 5-minute price change from OKX DEX ranking
2. For existing positions: checks 6-layer exit system (ranking exit, stops, trailing, TP)
3. For new tokens entering the ranking: applies 3-layer safety filter (25 total checks)
4. Calculates momentum score (0-125) — only buys if score >= threshold
5. Executes swap via OKX DEX aggregator with Solana on-chain signing
6. Sends Telegram notifications on buy/sell/error events

### What It Won't Do

| Rule | Reason |
|------|--------|
| No tokens already seen | `known_tokens` set prevents re-entry |
| No tokens in cooldown | Post-sell cooldown (default 30min) |
| No buying above max positions | Default 5 simultaneous positions |
| No buying below budget | Budget tracking prevents over-deployment |
| No buying with low score | Momentum score must meet threshold |
| No buying with high risk | 3-layer safety filter rejects unsafe tokens |
| No trading after daily loss limit | Auto-stops at configured % loss |
| No trading after circuit breaker | 5 consecutive errors triggers 1h cooldown |

---

## 3-Layer Safety Filter (25 Checks)

### Layer 1: Slot Guard (13 checks from ranking data)

| # | Check | Default Threshold | Production Suggestion |
|---|-------|-------------------|----------------------|
| 1 | Price change min | >= 1% | 15% |
| 2 | Price change max | <= 150% | 150% |
| 3 | Liquidity | >= $1,000 | $5,000 |
| 4 | Market cap min | >= $1,000 | $5,000 |
| 5 | Market cap max | <= $50M | $10M |
| 6 | Holders | >= 5 | 30 |
| 7 | Buy ratio (buy/total TX) | >= 40% | 55% |
| 8 | Unique traders | >= 5 | 20 |
| 9 | Skip system tokens | SOL, USDC, etc. | - |
| 10 | Cooldown check | Not in cooldown | - |
| 11 | Position limit | < max_positions | - |
| 12 | Already holding | Not holding | - |
| 13 | Daily loss limit | Not exceeded | - |

### Layer 2: Advanced Safety (9 checks from advanced-info API)

| # | Check | Default Threshold | Production Suggestion |
|---|-------|-------------------|----------------------|
| 14 | Risk control level | <= 3 | 1 |
| 15 | Honeypot tag | Not present | - |
| 16 | Top 10 concentration | <= 80% | 50% |
| 17 | Dev holding | <= 50% | 20% |
| 18 | Bundler holding | <= 50% | 15% |
| 19 | LP burned | >= 0% | 80% |
| 20 | Dev rug pull count | <= 100 | 10 |
| 21 | Sniper holding | <= 50% | 20% |
| 22 | Block internal (PumpFun) | false | true |

### Layer 3: Holder Risk Scan (3 checks from holder API)

| # | Check | Default Threshold | Production Suggestion |
|---|-------|-------------------|----------------------|
| 23 | Suspicious holder total % | <= 50% | 10% |
| 24 | Suspicious holder count | <= 50 | 5 |
| 25 | Phishing holders | allowed | blocked |

**Note:** Default thresholds are relaxed for testing. For production use, update the config file with the "Production Suggestion" values.

---

## Momentum Score (0-125)

### Base Score (0-100)

| Component | Max Points | Formula |
|-----------|-----------|---------|
| Buy Score | 40 | buy_ratio x 40 |
| Change Penalty | 20 | if change > 100%: 20 - (change-100)/10; else: change/5 |
| Trader Score | 20 | min(traders/50, 1) x 20 |
| Liquidity Score | 20 | min(liquidity/50000, 1) x 20 |

### Bonus Score (0-25, capped)

| Bonus | Points | Condition |
|-------|--------|-----------|
| Smart Money | +8 | `smartMoneyBuy` tag present |
| Low Concentration | +5/+2 | Top10 < 30% / < 50% |
| DS Paid | +3 | `dsPaid` tag present |
| Community | +2 | `dexScreenerTokenCommunityTakeOver` tag |
| Low Sniper | +4/+2 | Sniper < 5% / < 10% |
| Dev Clean | +3 | Dev hold 0% AND rug count < 3 |
| Zero Suspicious | +2 | No active suspicious holders |

**Buy threshold:** Default 10 (testing), production suggestion 40.

---

## 6-Layer Exit System

Priority order (first match exits):

| Layer | Exit Type | Condition | Action |
|-------|-----------|-----------|--------|
| 1 | Ranking Exit | Token drops off top N ranking (after 60s) | FULL sell |
| 2 | Hard Stop | PnL <= -25% | FULL sell |
| 3 | Fast Stop | PnL <= -8% after 5 minutes | FULL sell |
| 4 | Trailing Stop | Drawdown >= 12% from peak (activates at +8%) | FULL sell |
| 5 | Time Stop | Elapsed >= time_stop_secs (default 120s test / 6h prod) | FULL sell |
| 6 | Gradient TP | PnL >= TP level | PARTIAL sell (25%/35%/40%) |

### Gradient Take-Profit Levels

| Level | Default Trigger | Sell Portion |
|-------|----------------|-------------|
| TP1 | +5% | 25% |
| TP2 | +15% | 35% |
| TP3 | +30% | 40% |

---

## Configurable Parameters

Parameters are persisted at `~/.plugin-store/ranking_sniper_config.json`. View with `plugin-store ranking-sniper config`. Edit the JSON file directly to change values.

### Money Management

| Parameter | Default | Description |
|-----------|---------|-------------|
| `budget_sol` | 0.5 | Total SOL budget for the strategy |
| `per_trade_sol` | 0.05 | SOL amount per buy trade |
| `max_positions` | 5 | Maximum simultaneous positions |
| `gas_reserve_sol` | 0.01 | SOL reserved for gas fees |
| `min_wallet_balance` | 0.1 | Minimum wallet balance to maintain |
| `daily_loss_limit_pct` | 15.0 | Daily loss limit (% of budget) |
| `dry_run` | false | Simulate without executing swaps |

### Trading Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `slippage_pct` | "15" | DEX slippage tolerance (%) |
| `score_buy_threshold` | 10 | Momentum score threshold (0-125) |
| `tick_interval_secs` | 10 | Polling interval (seconds) |
| `cooldown_minutes` | 30 | Post-sell cooldown per token (minutes) |
| `top_n` | 20 | Number of ranking entries to scan |

### Exit System

| Parameter | Default | Description |
|-----------|---------|-------------|
| `hard_stop_pct` | -25.0 | Hard stop-loss (%) |
| `fast_stop_time_secs` | 300 | Fast stop window (seconds) |
| `fast_stop_pct` | -8.0 | Fast stop threshold (%) |
| `trailing_activate_pct` | 8.0 | Trailing stop activation (%) |
| `trailing_drawdown_pct` | 12.0 | Trailing stop drawdown (%) |
| `time_stop_secs` | 120 | Time stop (seconds), production: 21600 |
| `tp_levels` | [5, 15, 30] | Gradient take-profit levels (%) |

### Circuit Breaker

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_consecutive_errors` | 5 | Errors before circuit breaker trips |
| `cooldown_after_errors` | 3600 | Cooldown after breaker (seconds) |

---

## CLI Command Reference

### plugin-store ranking-sniper tick

Execute one tick cycle: fetch ranking, check exits for existing positions, scan for new entry signals, execute trades.

```bash
plugin-store ranking-sniper tick [--budget <sol>] [--per-trade <sol>] [--dry-run]
```

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `--budget` | No | 0.5 | Total SOL budget |
| `--per-trade` | No | 0.05 | SOL per buy trade |
| `--dry-run` | No | false | Simulate without real swaps |

**Return fields:**

| Field | Description |
|-------|-------------|
| `tick_time` | ISO 8601 timestamp |
| `positions` | Number of open positions |
| `remaining_budget_sol` | Remaining SOL budget |
| `daily_pnl_sol` | Daily PnL in SOL |
| `actions` | Array of actions taken (buy/exit/skip/buy_failed) |
| `dry_run` | Whether this was a dry-run |

**Action types in output:**
- `buy` — New position opened (symbol, price, amount_sol, score, tx_hash)
- `exit` — Position closed (symbol, reason, exit_type, pnl_pct, pnl_sol, tx_hash)
- `skip` — Token rejected by safety filter or score (symbol, reason)
- `buy_failed` — Buy swap failed (symbol, error)
- `exit_failed` — Sell swap failed (symbol, reason, error)
- `no_ranking_data` — No ranking data available

### plugin-store ranking-sniper start

Start the bot in foreground, executing `tick` every 10 seconds. Creates a PID file at `~/.plugin-store/ranking_sniper.pid`. Use Ctrl+C or `ranking-sniper stop` to terminate. Logs to `~/.plugin-store/ranking_sniper.log`.

```bash
plugin-store ranking-sniper start [--budget <sol>] [--per-trade <sol>] [--dry-run]
```

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `--budget` | No | 0.5 | Total SOL budget |
| `--per-trade` | No | 0.05 | SOL per buy trade |
| `--dry-run` | No | false | Simulate without real swaps |

Prints the full parameter summary before starting. Sends Telegram notification on start/stop if configured.

### plugin-store ranking-sniper stop

Stop a running bot by sending SIGTERM to the process in the PID file.

```bash
plugin-store ranking-sniper stop
```

### plugin-store ranking-sniper status

Show current bot state, open positions, PnL overview, and whether the bot is running.

```bash
plugin-store ranking-sniper status
```

**Return fields:**

| Field | Description |
|-------|-------------|
| `bot_running` | Whether a bot process is active |
| `stopped` | Whether the bot was stopped by a limit |
| `stop_reason` | Reason for stop (if applicable) |
| `positions` | Array of open position details |
| `position_count` | Number of open positions |
| `remaining_budget_sol` | Remaining SOL budget |
| `daily_pnl_sol` | Daily PnL in SOL |
| `known_tokens_count` | Total tokens seen |
| `consecutive_errors` | Current error count |

### plugin-store ranking-sniper report

Detailed performance report with win/loss stats.

```bash
plugin-store ranking-sniper report
```

**Return fields:**

| Field | Description |
|-------|-------------|
| `total_buys` | Total buy trades |
| `total_sells` | Total sell trades |
| `successful_trades` | Successful trade count |
| `failed_trades` | Failed trade count |
| `total_invested_sol` | Total SOL invested |
| `total_returned_sol` | Total SOL returned from sells |
| `total_pnl_sol` | Total realized PnL in SOL |
| `daily_pnl_sol` | Today's PnL |
| `win_count` | Winning trades |
| `loss_count` | Losing trades |
| `win_rate` | Win percentage |

### plugin-store ranking-sniper history

Show trade history (most recent first).

```bash
plugin-store ranking-sniper history [--limit <n>]
```

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `--limit` | No | 50 | Number of trades to show |

Each trade includes: time, symbol, token_address, action (BUY/SELL), price, amount_sol, score, exit_reason, pnl_pct, pnl_sol, tx_hash.

### plugin-store ranking-sniper reset

Delete all sniper state. Requires `--force` flag for safety.

```bash
plugin-store ranking-sniper reset --force
```

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `--force` | Yes | - | Required to confirm destructive action |

### plugin-store ranking-sniper analyze

Market analysis showing the current trending ranking, top tokens, and bot state summary.

```bash
plugin-store ranking-sniper analyze
```

**Return fields:**

| Field | Description |
|-------|-------------|
| `ranking_count` | Number of tokens in current ranking |
| `top_tokens` | Array of top tokens (symbol, address, change_24h, market_cap, volume, holders) |
| `known_tokens_count` | Total tokens the bot has seen |
| `active_positions` | Number of open positions |

### plugin-store ranking-sniper test-trade

Execute a buy+sell round-trip for a specific token. For development and debugging only. Buys a small amount of the token, waits a few seconds, then sells it back.

```bash
plugin-store ranking-sniper test-trade <token_address> [--amount <sol>]
```

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `<token_address>` | Yes | - | Token contract address on Solana |
| `--amount` | No | 0.01 | SOL amount to buy |

**Return fields:**

| Field | Description |
|-------|-------------|
| `token` | Token address |
| `amount_sol` | SOL amount used |
| `buy.tx_hash` | Buy transaction hash |
| `buy.price` | Price at buy time |
| `buy.amount_out` | Token amount received |
| `sell.tx_hash` | Sell transaction hash |
| `sell.amount_out` | SOL amount received back |
| `price_before` | Price before buy |
| `price_after` | Price after sell |

### plugin-store ranking-sniper config

Show all configurable parameters and their current values, organized by category. Also shows the config file and log file paths.

```bash
plugin-store ranking-sniper config
```

Displays parameter groups:
- Money management (budget, per_trade, max_positions, etc.)
- Trading parameters (slippage, score threshold, tick interval, etc.)
- Layer 1 Slot Guard thresholds
- Layer 2 Advanced Safety thresholds
- Layer 3 Holder Risk Scan thresholds
- Exit system (stops, trailing, take-profit levels)
- Circuit breaker
- Logging
- Telegram notifications

### plugin-store ranking-sniper sell-all

Force-sell all open positions immediately. Retries with halved amounts if liquidity is insufficient (up to 4 attempts per position).

```bash
plugin-store ranking-sniper sell-all
```

**Return fields:**

| Field | Description |
|-------|-------------|
| `sold` | Number of positions successfully sold |
| `failed` | Number of positions that failed to sell |
| `results` | Array of per-position results (symbol, token, status, tx_hash, sol_out, error) |

### plugin-store ranking-sniper sell

Sell a specific token by contract address with a raw token amount.

```bash
plugin-store ranking-sniper sell <token_address> --amount <raw_amount>
```

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `<token_address>` | Yes | - | Token contract address |
| `--amount` | Yes | - | Raw token amount (in smallest units) |

**Return fields:**

| Field | Description |
|-------|-------------|
| `token` | Token address |
| `tx_hash` | Transaction hash |
| `sol_out` | SOL received (human-readable) |
| `amount_out_lamports` | SOL received in lamports |

---

## OKX API Endpoints Used

### Market Data APIs

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/v6/dex/market/token/toplist` | GET | Fetch trending token ranking by price change |
| `/api/v6/dex/market/token/advanced-info` | GET | Token safety data (risk level, tags, dev/bundler/sniper %) |
| `/api/v6/dex/market/price-info` | POST | Real-time token price |
| `/api/v6/dex/market/token/holder` | GET | Holder data filtered by tag (suspicious/phishing) |

### Trade Execution APIs

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/v6/dex/aggregator/swap` | GET | Get swap transaction data |
| `/api/v6/dex/pre-transaction/broadcast-transaction` | POST | Broadcast signed transaction via OKX |
| `/api/v6/dex/post-transaction/orders` | GET | Poll transaction confirmation status |

### Solana RPC (Direct)

| Method | Purpose |
|--------|---------|
| `getLatestBlockhash` | Fresh blockhash for transaction signing |
| `sendTransaction` | Direct broadcast to Solana network |
| `getSignatureStatuses` | Verify transaction confirmation |
| `getTokenAccountsByOwner` | Check wSOL ATA balance |

---

## Execution Pipeline

```
fetch_ranking(top_n=20)              <- OKX /token/toplist (sort by 5m change)
    |
    +-> For each existing position:
    |     fetch_price(token)          <- OKX /price-info
    |     check_exits(6 layers)       <- engine.rs pure function
    |     If exit signal -> sell      <- OKX /aggregator/swap + sign + broadcast
    |
    +-> For each new token in ranking:
          known_tokens check           <- skip if already seen
          budget + position check      <- skip if insufficient
          fetch_advanced_info()        <- OKX /token/advanced-info
          run_slot_guard(13 checks)    <- engine.rs pure function
          run_advanced_safety(9 checks)<- engine.rs pure function
          fetch_holder_risk()          <- OKX /token/holder (tag 6 + 8)
          run_holder_risk_scan(3 chks) <- engine.rs pure function
          calc_momentum_score()        <- engine.rs (0-125)
          If score >= threshold:
            fetch_price()              <- OKX /price-info
            buy_token()                <- OKX /aggregator/swap + sign + broadcast
```

---

## Cross-Skill Workflows

### Workflow A: Analyze Then Snipe

> User: "What's trending on Solana right now? Start sniping if it looks good."

```
1. plugin-store ranking-sniper analyze          -> see current ranking + top tokens
2. plugin-store ranking-sniper config           -> review parameters
       | user adjusts thresholds if needed
3. plugin-store ranking-sniper tick --dry-run   -> dry-run to see what passes filters
       | looks good
4. plugin-store ranking-sniper start --budget 0.5 --per-trade 0.05  -> go live
5. plugin-store ranking-sniper status           -> monitor positions
```

### Workflow B: Monitor and Emergency Exit

> User: "Check my sniper positions. Sell everything if it's losing."

```
1. plugin-store ranking-sniper status           -> see positions + PnL
       | user sees losses
2. plugin-store ranking-sniper sell-all         -> emergency exit all positions
3. plugin-store ranking-sniper report           -> review final stats
```

### Workflow C: Research a Specific Token

> User: "The sniper bought TOKEN, tell me more about it."

```
1. plugin-store ranking-sniper status                              -> get token address
2. okx-dex-token    plugin-store token search TOKEN --chain solana -> token details
3. okx-dex-market   plugin-store market kline --address <addr> --chain solana -> chart
4. okx-wallet-portfolio  plugin-store portfolio balance --chain solana -> wallet balance
```

### Workflow D: Test Before Deploying

> User: "I want to test the sniper on a specific token before going live."

```
1. okx-dex-token    plugin-store token search HYPE --chain solana              -> find token
2. plugin-store ranking-sniper test-trade <token_address> --amount 0.01        -> round-trip test
3. plugin-store ranking-sniper start --budget 0.5 --per-trade 0.05 --dry-run  -> dry-run session
       | verify actions look correct
4. plugin-store ranking-sniper start --budget 0.5 --per-trade 0.05            -> go live
```

---

## State Persistence

State is stored at `~/.plugin-store/ranking_sniper_state.json` with atomic writes (write to `.tmp`, rename).

| File | Purpose |
|------|---------|
| `~/.plugin-store/ranking_sniper_state.json` | Full bot state (positions, trades, stats, known tokens) |
| `~/.plugin-store/ranking_sniper_config.json` | User-configurable parameters |
| `~/.plugin-store/ranking_sniper.pid` | PID file for running bot |
| `~/.plugin-store/ranking_sniper.log` | Execution log |

State includes:
- `known_tokens` — Set of all token addresses ever seen (prevents re-entry)
- `positions` — Map of token_address -> Position (with buy price, time, peak PnL, trailing state)
- `trades` — Trade history (last 100 entries)
- `stats` — Cumulative stats (buys, sells, invested, returned, daily PnL)
- `errors` — Consecutive error tracking for circuit breaker
- `last_sell_times` — Per-token cooldown timestamps

---

## Key Concepts

- **Ranking-based entry**: Unlike signal-based scanners, this bot only considers tokens that appear in the OKX top-N trending list. Entry is triggered by a token being new to the ranking AND passing all safety checks.
- **Ranking-based exit**: The primary exit signal is a token dropping off the ranking entirely. This is Layer 1 of the exit system and takes priority over most other exit conditions.
- **Momentum Score**: A composite score (0-125) combining buy pressure, price change, trader count, liquidity, and bonus signals (smart money, low concentration, etc.). Prevents buying tokens that pass safety but lack momentum.
- **Known tokens set**: Once a token is seen in the ranking, it enters the `known_tokens` set permanently (within a session). The bot will never re-enter the same token. Use `reset --force` to clear.
- **Gradient take-profit**: Sells in 3 tranches (25%/35%/40%) at increasing profit levels, rather than all-at-once. Locks in partial profit while allowing further upside.
- **Trailing stop**: Activates when PnL reaches a threshold (default +8%), then sells if price drops by the drawdown percentage (default 12%) from the peak.
- **Dry-run mode**: Executes the full pipeline (ranking fetch, safety checks, scoring) but skips actual swap execution. Actions are logged as `DRY_RUN`. Useful for validating filter parameters.

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| No ranking data available | Saves state, outputs `no_ranking_data` |
| Circuit breaker tripped (5 errors) | Rejects all ticks for cooldown period (default 1h) |
| Daily loss limit exceeded | Bot stops, requires `reset --force` to restart |
| Bot previously stopped by limit | Rejects ticks with reason, requires reset |
| Budget exhausted | Skips all new buys, continues monitoring exits |
| Max positions reached | Skips new buys, continues monitoring exits |
| Token already in known_tokens | Skipped silently (no re-entry) |
| Sell fails (insufficient liquidity) | `sell-all` retries with halved amounts (up to 4x) |
| Advanced-info API fails | Token skipped with reason |
| Price fetch fails | Position exit check skipped for that token |
| SOL_ADDRESS not set | Error on tick/start/sell commands |
| SOL_PRIVATE_KEY not set | Error on swap execution (buy/sell) |
| Bot already running | `start` rejects with existing PID warning |
| No running bot | `stop` returns error |
| Reset without --force | Returns error, requires confirmation |
| wSOL ATA missing | Auto-created and funded before buy swap |
| Blockhash expired | Fresh blockhash fetched from Solana RPC |
| Solana RPC broadcast fails | Falls back to OKX broadcast endpoint |
| Transaction not confirmed | Polled for up to 60 seconds before failing |

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "SOL_ADDRESS not set" | Missing env var | Set `SOL_ADDRESS` to your Solana wallet address |
| "SOL_PRIVATE_KEY not set" | Missing env var | Set `SOL_PRIVATE_KEY` (base58, 32 or 64 bytes) |
| Circuit breaker trips | Repeated API/swap failures | Check logs at `~/.plugin-store/ranking_sniper.log`, fix root cause, wait 1h or reset |
| No buys happening | Score threshold too high, or safety filters too strict | Try `--dry-run` to see skip reasons, adjust config thresholds |
| All tokens skipped by slot_guard | Thresholds set to production values | For testing, lower `min_change_pct`, `min_liquidity`, `min_holders`, etc. |
| Sell fails repeatedly | Low liquidity token | Use `sell-all` (auto-retries with halved amounts) or manual `sell` |
| "Bot stopped" on tick | Daily loss limit or prior stop | Run `ranking-sniper reset --force` to clear state |
| High slippage on swaps | Default 15% may be too high or too low | Adjust `slippage_pct` in config (3-5% for production, higher for memes) |
| Telegram not working | Missing or incorrect bot token/chat ID | Set in config file or env vars, verify with Telegram BotFather |

---

## Security Notes

- **Private key**: Loaded from `SOL_PRIVATE_KEY` env var, used only for transaction signing, never logged
- **API auth**: HMAC-SHA256 via OKX ApiClient, keys only in HTTP headers
- **Atomic state writes**: Write to `.tmp` file then rename to prevent corruption
- **Fail-closed**: API failures result in skipping the token, not proceeding with partial data
- **Capital controls**: Budget tracking, position limits, daily loss limits, and circuit breaker prevent runaway losses
