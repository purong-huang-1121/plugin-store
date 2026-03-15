---
name: dapp-kalshi
description: "This skill should be used when the user asks about Kalshi, US prediction markets, regulated event contracts, 'what are the odds', 'bet on', 'buy Yes/No shares', 'Kalshi positions', 'Kalshi balance', 'will X happen', or mentions Kalshi, federally authorized prediction markets, regulated event contracts, or outcome trading under US regulation. Covers market search, price queries, orderbook, price history, and trading (buy/sell outcome shares). Do NOT use for Polymarket (decentralized, on-chain) — use okx-dapp-polymarket instead. Do NOT use for general DEX swaps — use okx-dex-swap instead."
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Kalshi Prediction Markets CLI

12 commands for US-regulated prediction market search, pricing, and trading across demo and production environments.

## Pre-flight Checks

Every time before running any `plugin-store` command, always follow these steps in order. Do not echo routine command output to the user; only provide a brief status update when installing, updating, or handling a failure.

1. **Confirm installed**: Run `which plugin-store`. If not found, install it:
   ```bash
   curl -sSL https://raw.githubusercontent.com/okx/plugin-store/main/install.sh | sh
   ```
   If the install script fails, ask the user to install manually following the instructions at: https://github.com/okx/plugin-store

2. **Check for updates**: Read `~/.plugin-store/last_check` and compare it with the current timestamp:
   ```bash
   cached_ts=$(cat ~/.plugin-store/last_check 2>/dev/null || true)
   now=$(date +%s)
   ```
   - If `cached_ts` is non-empty and `(now - cached_ts) < 43200` (12 hours), skip the update and proceed.
   - Otherwise (file missing or older than 12 hours), run the installer to check for updates:
     ```bash
     curl -sSL https://raw.githubusercontent.com/okx/plugin-store/main/install.sh | sh
     ```
3. If any `plugin-store` command fails with an unexpected error during this session, try reinstalling before giving up.

## Skill Routing

- For token search / analytics → use `okx-dex-token`
- For DEX swap → use `okx-dex-swap`
- For token prices / charts → use `okx-dex-market`
- For wallet balances → use `okx-wallet-portfolio`
- For transaction broadcasting → use `okx-onchain-gateway`
- For Polymarket (decentralized, on-chain USDC, no KYC) → use `okx-dapp-polymarket`

**Kalshi vs Polymarket — when the user mentions both:** present the key difference:
- Kalshi: US federally licensed, KYC required, USD bank account, US residents only
- Polymarket: Decentralized, no KYC, USDC on Polygon, global access

## Authentication

**Data commands (search, markets, event, price, book, history):** No authentication needed. Work immediately in both `demo` and `prod`.

**Trading commands (buy, sell, cancel, orders, positions, balance):** Require Kalshi RSA API credentials.

```bash
# Option A: environment variables (recommended)
KALSHI_KEY_ID=your-key-id
KALSHI_PRIVATE_KEY_PEM=/path/to/private_key.pem   # file path OR raw PEM content

# Option B: auto-cached after first use
~/.plugin-store/kalshi_demo.json   # demo environment
~/.plugin-store/kalshi_prod.json   # production environment
```

API keys are available at: https://kalshi.com/profile/api-keys

**Important:**
- Kalshi uses **RSA-PSS with SHA-256** signing — no blockchain wallet or EVM private key needed
- Default environment is **demo** (safe for testing, no real money). Use `--env prod` for real trades.
- **KYC required** for production (US residents only; some states excluded)

## Quickstart

### Browse and Research (demo — no credentials)

```bash
# List active markets sorted by volume
plugin-store kalshi markets

# Search for a topic
plugin-store kalshi search "fed rate"

# Get event details
plugin-store kalshi event FED-2024

# Check Yes/No prices
plugin-store kalshi price FED-24DEC-T5.25

# View orderbook
plugin-store kalshi book FED-24DEC-T5.25

# Price history
plugin-store kalshi history FED-24DEC-T5.25 --interval 1d
```

### Trade (requires credentials)

```bash
# Buy 10 Yes contracts at 65 cents in demo
plugin-store kalshi buy --ticker FED-24DEC-T5.25 --side yes --count 10 --price 0.65

# Sell 5 No contracts at 30 cents
plugin-store kalshi sell --ticker FED-24DEC-T5.25 --side no --count 5 --price 0.30

# Check open orders
plugin-store kalshi orders

# Cancel an order
plugin-store kalshi cancel <order_id>

# View positions and balance
plugin-store kalshi positions
plugin-store kalshi balance
```

### Switch to production

```bash
plugin-store kalshi --env prod markets
plugin-store kalshi --env prod buy --ticker FED-24DEC-T5.25 --side yes --count 10 --price 0.65
```

## Command Index

| # | Command | Auth | Env | Description |
|---|---------|------|-----|-------------|
| 1 | `plugin-store kalshi search <query>` | No | demo/prod | Search events and markets |
| 2 | `plugin-store kalshi markets` | No | demo/prod | List popular/active markets |
| 3 | `plugin-store kalshi event <event_ticker>` | No | demo/prod | Get event with related markets |
| 4 | `plugin-store kalshi price <ticker>` | No | demo/prod | Get Yes/No price, midpoint |
| 5 | `plugin-store kalshi book <ticker>` | No | demo/prod | View orderbook depth |
| 6 | `plugin-store kalshi history <ticker>` | No | demo/prod | Price history K-line |
| 7 | `plugin-store kalshi buy` | Yes | demo/prod | Buy Yes/No shares (limit order) |
| 8 | `plugin-store kalshi sell` | Yes | demo/prod | Sell Yes/No shares |
| 9 | `plugin-store kalshi cancel <order_id>` | Yes | demo/prod | Cancel an open order |
| 10 | `plugin-store kalshi orders` | Yes | demo/prod | View open orders |
| 11 | `plugin-store kalshi positions` | Yes | demo/prod | View current positions |
| 12 | `plugin-store kalshi balance` | Yes | demo/prod | View USD account balance |

## Cross-Skill Workflows

### Workflow A: Research and Buy (most common)

> User: "What are the hottest prediction markets on Kalshi right now?"

```
1. kalshi markets --sort volume --limit 5      → show top markets
       ↓ user picks one
2. kalshi price <ticker>                        → show Yes/No prices and probability
       ↓ user wants to buy
3. Check KALSHI_KEY_ID is set
       ↓ not set → guide to https://kalshi.com/profile/api-keys
       ↓ set → continue
4. kalshi buy --ticker <ticker> --side yes --count 10 --price 0.65
       ↓
5. "Order placed! 10 Yes contracts @ 65 cents."
```

**Data handoff:**
- `ticker` from markets data → `<ticker>` for price/book/buy/sell commands
- Yes price from `price` command → `--price` for buy (convert to 0–1: 65 cents = 0.65)

### Workflow B: Portfolio Review and Sell

```
1. kalshi positions                             → show all holdings with P&L
2. kalshi price <ticker>                        → check current price of a position
3. kalshi sell --ticker <ticker> --side yes --count 5 --price 0.72
```

### Workflow C: Market Research

```
1. kalshi search "bitcoin"                      → find BTC-related events
2. kalshi event <event_ticker>                  → see all markets in the event
3. kalshi book <ticker>                         → check liquidity depth
4. kalshi history <ticker> --interval 1d        → price trend over time
```

### Workflow D: Demo Before Production

```
# Always test in demo first
1. kalshi --env demo markets                    → verify commands work
2. kalshi --env demo buy --ticker <t> --side yes --count 1 --price 0.5
       ↓ order confirmed in demo
3. kalshi --env prod buy --ticker <t> --side yes --count 10 --price 0.65
       ↓ real money trade
```

## Operation Flow

### Step 1: Identify Intent

- Browse markets → `search` or `markets`
- Check price/odds → `price`
- Analyze depth → `book`
- Check trend → `history`
- Buy/sell shares → `buy` / `sell`
- Manage orders → `orders` / `cancel`
- Check portfolio → `positions` / `balance`

### Step 2: Collect Parameters

- Missing search query → ask what topic the user is interested in
- Missing ticker → use `search` or `markets` first, extract `ticker` field
- Missing side (yes/no) → ask user which outcome they want to trade
- Missing price → show current price via `price`, suggest buying at ask or midpoint
- Missing count → ask how many contracts (each contract = $1 face value)
- Missing credentials (for trading) → prompt to set `KALSHI_KEY_ID` and `KALSHI_PRIVATE_KEY_PEM`
- Environment not set → default to `demo`; prompt for `--env prod` confirmation for real trades

### Step 3: Execute

- **Data phase**: show market info and prices so user can make an informed decision
- **Confirmation phase** (prod only): before any buy/sell on `--env prod`, display ticker, side, count, price, estimated cost, and ask for confirmation
- **Execution phase**: place order, show result with order ID

### Step 4: Suggest Next Steps

| Just completed | Suggest |
|---|---|
| `search` or `markets` | 1. Check price → `price` 2. View orderbook → `book` |
| `price` | 1. Buy shares → `buy` 2. View price history → `history` |
| `buy` or `sell` | 1. Check order status → `orders` 2. View updated positions → `positions` |
| `positions` | 1. Check current price → `price` 2. Sell a position → `sell` |

Present conversationally — never expose skill names or endpoint paths to the user.

## CLI Command Reference

### 1. plugin-store kalshi search

```bash
plugin-store kalshi [--env demo|prod] search <query> [--limit <n>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<query>` | Yes | - | Search keywords |
| `--limit` | No | 20 | Max results |

**Key return fields per market:**

| Field | Description |
|---|---|
| `ticker` | Market unique identifier — **use this for price/book/buy/sell** |
| `event_ticker` | Parent event identifier — use for `event` command |
| `title` | Human-readable market question |
| `yes_bid` | Current best Yes bid price (integer cents, 1–99) |
| `no_bid` | Current best No bid price (integer cents) |
| `volume` | Total trading volume |
| `status` | Market status: open, closed, settled |
| `close_time` | Market closing timestamp |

### 2. plugin-store kalshi markets

```bash
plugin-store kalshi [--env demo|prod] markets [--status <status>] [--sort <sort>] [--limit <n>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--status` | No | open | Filter: open, closed, settled |
| `--sort` | No | volume | Sort: volume, liquidity, newest, ending |
| `--limit` | No | 20 | Max results |

### 3. plugin-store kalshi event

```bash
plugin-store kalshi [--env demo|prod] event <event_ticker>
```

### 4. plugin-store kalshi price

```bash
plugin-store kalshi [--env demo|prod] price <ticker>
```

**Return fields:**

| Field | Description |
|---|---|
| `yes_bid` | Best Yes bid (cents) |
| `yes_ask` | Best Yes ask (cents) |
| `yes_mid` | Yes midpoint (cents) |
| `yes_probability` | Yes mid as 0–1 probability |
| `no_bid` | Best No bid (cents) |
| `no_ask` | Best No ask (cents) |
| `no_probability` | No mid as 0–1 probability |

### 5. plugin-store kalshi book

```bash
plugin-store kalshi [--env demo|prod] book <ticker> [--depth <n>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--depth` | No | 5 | Number of price levels per side |

### 6. plugin-store kalshi history

```bash
plugin-store kalshi [--env demo|prod] history <ticker> [--interval <interval>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--interval` | No | 1d | 1m, 1h, 6h, 1d, 1w, all |

### 7. plugin-store kalshi buy

```bash
plugin-store kalshi [--env demo|prod] buy --ticker <ticker> --side <yes|no> --count <n> --price <0-1> [--order-type limit|market]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--ticker` | Yes | - | Market ticker |
| `--side` | Yes | - | Outcome: `yes` or `no` |
| `--count` | Yes | - | Number of contracts (each = $1 face value) |
| `--price` | Yes | - | Limit price as probability 0–1 (e.g. `0.65` = 65 cents) |
| `--order-type` | No | limit | `limit` or `market` |

### 8. plugin-store kalshi sell

```bash
plugin-store kalshi [--env demo|prod] sell --ticker <ticker> --side <yes|no> --count <n> --price <0-1> [--order-type limit|market]
```

### 9. plugin-store kalshi cancel

```bash
plugin-store kalshi [--env demo|prod] cancel <order_id>
```

### 10. plugin-store kalshi orders

```bash
plugin-store kalshi [--env demo|prod] orders [--ticker <ticker>] [--status <status>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--ticker` | No | - | Filter by market ticker |
| `--status` | No | - | Filter: resting, pending, cancelled, executed |

### 11. plugin-store kalshi positions

```bash
plugin-store kalshi [--env demo|prod] positions [--settlement-status <status>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--settlement-status` | No | unsettled | unsettled, settled, all |

### 12. plugin-store kalshi balance

```bash
plugin-store kalshi [--env demo|prod] balance
```

## Key Concepts

- **Prices are probabilities in cents**: A price of 65 means 65% implied probability. Buying Yes at 65 cents means you pay $0.65 per contract and receive $1.00 if Yes resolves.
- **Yes + No prices sum to ~100**: Due to bid/ask spreads, `yes_ask + no_ask` is slightly above 100 cents.
- **Demo vs Production**: Demo uses `demo-api.kalshi.co` (paper trading, no real money). Production uses `api.elections.kalshi.com` (real USD). Always test in demo first.
- **RSA credentials**: Unlike Polymarket (which uses an EVM private key), Kalshi uses an RSA key pair generated in your Kalshi dashboard. No blockchain wallet needed.
- **Count vs Amount**: Kalshi orders specify `count` (number of contracts), not a USD amount. Each contract has a face value of $1.00.

## Edge Cases

- **Market not open**: Check `status` field — only `open` markets can be traded.
- **Price out of range**: `--price` must be strictly between 0 and 1. Reject 0 or 1.
- **Invalid side**: `--side` must be `yes` or `no` (case-insensitive).
- **Credentials not set**: For trading commands, show clear error: "Set KALSHI_KEY_ID and KALSHI_PRIVATE_KEY_PEM"
- **Production confirmation**: When `--env prod` is used for buy/sell, always confirm with the user before executing.
- **KYC / geo restriction**: If Kalshi returns 403, remind user that Kalshi requires KYC and is US-only (excluding some states).
- **Rate limiting**: Kalshi API has rate limits. Retry with backoff on 429.
- **RSA key format**: Key must be PKCS#8 PEM format. If loading from a file path, ensure the file is readable.
