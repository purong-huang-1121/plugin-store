---
name: strategy-auto-rebalance
description: "Use when the user asks about USDC yield optimization, 'auto-rebalance daemon', 'start yield monitor', 'yield farming automation', or mentions running an automated USDC rebalancer across Aave V3, Compound V3, and Morpho. Supports Base and Ethereum mainnet. The daemon periodically checks APY, detects optimal rebalancing opportunities, and executes trades with Telegram alerts. Do NOT use for single-protocol operations — use dapp-aave instead. Do NOT use for one-off yield checks — the daemon handles everything automatically."
license: Apache-2.0
metadata:
  author: okx
  version: "4.0.0"
  homepage: "https://web3.okx.com"
---

# USDC Auto-Rebalance Daemon

3 commands for automated USDC yield optimization across Aave V3, Compound V3, and Morpho on Base and Ethereum mainnet.

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

## Pre-Start Confirmation

**IMPORTANT**: Before executing `auto-rebalance start`, you MUST present the following summary to the user and ask for explicit confirmation. Do NOT start the daemon until the user approves.

Display a table like this:

```
Ready to start Auto-Rebalancer. Please confirm:

  Chain:           Base (8453)
  Interval:        300s (5 min)
  Min Spread:      0.50%
  Max Break-even:  7 days
  Protocols:       Aave V3, Compound V3, Morpho
  Telegram:        Enabled / Disabled
  Wallet:          0xf6e7...4572

  Gas threshold:   5 gwei (Base) / 50 gwei (Ethereum)
  TVL safety:      Emergency withdraw if TVL drops >30%
  State file:      ~/.plugin-store/auto-rebalance-state.json

Proceed? (y/n)
```

Key points to verify:
- Wallet address derived from `EVM_PRIVATE_KEY` — confirm it's the intended wallet
- Chain — confirm it matches user intent (Base vs Ethereum have very different gas costs)
- Interval — explain what it means in practical terms ("checks every X minutes")
- Min spread — lower = more frequent rebalancing; higher = fewer but more meaningful moves
- If wallet has idle USDC (not deposited in any protocol), the daemon will auto-deposit into the best protocol on its first cycle

## Skill Routing

- For single-protocol Aave operations → use `dapp-aave`
- For Morpho vault operations → use `dapp-morpho` (CLI: `plugin-store morpho`)
- For grid trading → use `strategy-grid-trade`
- For prediction markets → use `dapp-polymarket` / `dapp-kalshi`
- For perpetual trading → use `dapp-hyperliquid`

## Authentication

**All commands require an EVM wallet private key** (except `stop` and `status`):

```bash
EVM_PRIVATE_KEY=0x...
```

**Optional — Telegram notifications (recommended):**
```bash
TELEGRAM_BOT_TOKEN=...
TELEGRAM_CHAT_ID=...
```

All env vars can be set in `cli/.env` (auto-loaded via dotenvy).

## Multi-Chain Support

| Chain | ID | Gas Spike Threshold | Recommended Interval |
|-------|-----|-------------------|---------------------|
| Base | 8453 | 0.5 gwei | 300s (5 min) |
| Ethereum | 1 | 50 gwei | 3600s (60 min) |

Base has low gas (~$0.001-0.05 gwei per tx, ~$0.01-0.03 cost), so shorter intervals and lower spread thresholds make sense. Ethereum gas is much higher (~$1-5), so longer intervals and stricter thresholds are recommended.

**Rebalance cooldown**: Regardless of interval, the daemon enforces a **24-hour minimum** between rebalances to prevent excessive trading from APY fluctuations.

## Quickstart

```bash
# Base — low gas, check every 5 minutes, rebalance if spread > 0.3%
plugin-store auto-rebalance start --chain base --interval 300 --min-spread 0.3

# Ethereum — higher gas, check every hour, rebalance if spread > 1.0%
plugin-store auto-rebalance start --chain ethereum --interval 3600 --min-spread 1.0

# With Telegram notifications
plugin-store auto-rebalance start --chain base --interval 300 --min-spread 0.3 \
  --telegram-token <BOT_TOKEN> --telegram-chat <CHAT_ID>

# Check daemon status
plugin-store auto-rebalance status

# Stop daemon
plugin-store auto-rebalance stop
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `auto-rebalance start` | Yes | Start auto-rebalance daemon (foreground) |
| 2 | `auto-rebalance stop` | No | Stop running daemon via PID file |
| 3 | `auto-rebalance status` | No | Show daemon status and recent activity |

## CLI Command Reference

### plugin-store auto-rebalance start

```bash
plugin-store auto-rebalance start [--chain <chain>] [--interval <seconds>] [--min-spread <pct>] [--max-break-even <days>] [--telegram-token <token>] [--telegram-chat <id>]
```

| Param | Default | Description |
|---|---|---|
| `--chain` | `base` | Chain: `base`, `ethereum` |
| `--interval` | `3600` | Check interval in **seconds** (e.g. 300 = 5 min, 3600 = 1 hour) |
| `--min-spread` | `0.5` | Minimum APY spread (%) to trigger rebalance |
| `--max-break-even` | `7` | Maximum break-even days (gas cost / daily yield improvement) |
| `--telegram-token` | env | Telegram Bot API token (or `TELEGRAM_BOT_TOKEN` env var) |
| `--telegram-chat` | env | Telegram chat ID (or `TELEGRAM_CHAT_ID` env var) |

**Recommended configurations:**

| Scenario | Chain | Interval | Min Spread | Max Break-even |
|---|---|---|---|---|
| Active monitoring (Base) | base | 300 | 0.3 | 14 |
| Conservative (Base) | base | 3600 | 0.5 | 7 |
| Active monitoring (ETH) | ethereum | 1800 | 0.5 | 7 |
| Conservative (ETH) | ethereum | 3600 | 1.0 | 3 |
| Testing | base | 60 | 0.1 | 9999 |

**Daemon capabilities:**
- Periodic yield checks across Aave V3, Compound V3, and Morpho
- Smart decision engine: Hold / Rebalance / Emergency Withdraw
- **Auto-deposit**: If wallet has idle USDC and no protocol position, deposits into the best protocol automatically
- **Dynamic vault discovery**: Morpho vault selection via GraphQL API — picks the highest APY vault with TVL > $100k
- TVL safety monitoring — median-based comparison triggers emergency withdraw if TVL drops >30%
- Gas spike circuit breaker — pauses when gas exceeds chain threshold
- Telegram notifications (🤖 Auto-Rebalancer) for all events
- State persistence at `~/.plugin-store/auto-rebalance-state.json`
- PID management — prevents duplicate instances

### plugin-store auto-rebalance stop

Sends SIGTERM to the running daemon via PID file (`~/.plugin-store/auto-rebalance-daemon.pid`).

### plugin-store auto-rebalance status

Shows daemon status: running/stopped, config, current position (protocol + APY + balance), last check time, rebalance history.

## Decision Logic

Each cycle, the daemon:

1. **Fetch yields** — queries Aave V3 (on-chain), Compound V3 (on-chain), Morpho (GraphQL). Falls back to DeFiLlama if on-chain fails.
2. **Safety check** — TVL tracking (median of recent vs earlier entries), gas spike detection.
   - TVL drop >20% → alert notification (non-blocking)
   - TVL drop >30% → emergency withdrawal
3. **Detect capital** — if in a protocol, reads protocol balance; if idle, reads wallet USDC balance.
4. **Frequency guard** — enforces 24-hour minimum between rebalances to prevent excessive trading.
5. **Decide**:
   - **Emergency Withdraw** — if current protocol's TVL dropped >30% (median comparison)
   - **Hold** — if gas spiking, cooldown active, already in best protocol, spread too small, or break-even too long
   - **Rebalance** — withdraw from current → deposit into best; or initial deposit from wallet if protocol=none
6. **Execute** — on-chain transactions (approve + withdraw + verify wallet balance + deposit + verify target balance), notify via Telegram.

## Cross-Skill Workflows

### Workflow A: Research → Start Daemon

```
1. plugin-store aave markets --chain base             → check current Aave rates
2. plugin-store morpho vaults --chain base            → see Morpho vault options
3. plugin-store auto-rebalance start --chain base ...     → let the daemon auto-optimize
```

### Workflow B: Check Status → Manual Intervention

```
1. plugin-store auto-rebalance status                     → review position and PnL
2. plugin-store morpho positions <address> --chain base → verify on-chain state
3. plugin-store auto-rebalance stop                       → stop if needed
```

### Workflow C: Multi-Chain

```
# Terminal 1
plugin-store auto-rebalance start --chain base --interval 300 --min-spread 0.3

# Terminal 2
plugin-store auto-rebalance start --chain ethereum --interval 3600 --min-spread 1.0
```

Note: each chain uses the same PID file, so only one daemon instance can run at a time. For multi-chain, run in separate terminals with separate working directories, or stop one before starting the other.

## Key Concepts

- **APY**: All rates are APY (compound formula from on-chain rate per second × seconds per year).
- **Break-even**: Days for yield improvement to cover gas costs. Formula: `gas_cost / (capital × spread% / 365)`. Lower = better.
- **TVL monitoring**: Tracks protocol TVL using median of recent vs earlier entries (not single-point comparison). A >20% drop triggers an alert; >30% triggers emergency withdrawal. History capped at 96 entries (~24h at 15min intervals).
- **Gas circuit breaker**: Base threshold 0.5 gwei, Ethereum 50 gwei. Exceeding pauses all trading.
- **Rebalance cooldown**: 24-hour minimum between rebalances, regardless of check interval. Prevents excessive trading from short-term APY fluctuations.
- **Post-execution verification**: After withdrawal, verifies wallet received USDC before proceeding to deposit. Aborts if wallet balance is zero.
- **Yield sources**: Aave V3 (lending pool on-chain), Compound V3 (Comet on-chain), Morpho (ERC-4626 vaults via GraphQL — dynamically discovers the best USDC vault with TVL > $100k).
- **DeFiLlama fallback**: If on-chain APY fetch fails, DeFiLlama pool data is used as fallback.
- **Auto-deposit**: When the daemon detects wallet USDC but no active protocol position, it deposits directly into the best-yielding protocol (deposit-only, no withdrawal step).

## Edge Cases

| Scenario | Behavior |
|---|---|
| All protocols same APY | Hold — no benefit to rebalancing |
| On-chain query fails | Falls back to DeFiLlama API data |
| Gas spike above threshold | Rebalance paused regardless of yield spread |
| Wallet has idle USDC, no position | Auto-deposits into best protocol on first cycle |
| Break-even too long | Hold — not worth the gas cost |
| Rebalanced recently (<24h ago) | Hold — cooldown enforced to prevent over-trading |
| EVM_PRIVATE_KEY not set | Error on start |
| Daemon already running | Start rejects with existing PID warning |
| No running daemon | Stop returns error |
| TVL drops 20-30% (median) | Alert notification sent, rebalancing NOT blocked |
| TVL drops >30% (median) | Emergency withdrawal to wallet |
| Post-withdraw wallet balance is 0 | Rebalance aborted — supply step skipped to prevent loss |
| Morpho vault changes between cycles | Dynamic discovery — always picks highest APY vault |
| Chain switch (e.g. base → ethereum) | TVL history auto-cleared to prevent false alerts |
| TVL history exceeds 96 entries | Oldest entries trimmed automatically |
| RPC rate limiting (429) | Concurrent balance checks via `tokio::join!`, resilient to individual failures |
