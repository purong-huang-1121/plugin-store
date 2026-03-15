---
name: dapp-polymarket
description: "This skill should be used when the user asks about prediction markets, event betting, 'what are the odds of...', 'bet on...', 'buy Yes/No shares', 'Polymarket positions', 'prediction market prices', 'will X happen', or mentions Polymarket, prediction markets, event contracts, or outcome trading. Covers market search, price queries, orderbook data, price history, and trading (buy/sell outcome shares). Do NOT use for general DEX swaps or token trading — use okx-dex-swap instead."
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Polymarket Prediction Markets CLI

12 commands for prediction market search, pricing, and trading.

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

## Authentication

**Data commands (search, markets, event, price, book, history):** No authentication needed. Work immediately.

**Trading commands (buy, sell, cancel, orders, positions, balance):** Require a Polygon wallet private key:

```bash
# Add to .env file
EVM_PRIVATE_KEY=0x...
```

API credentials are automatically derived from the private key on first use and cached at `~/.plugin-store/polymarket_creds.json`.

## Quickstart

### Browse and Research

```bash
# Search for markets about bitcoin
plugin-store polymarket search "bitcoin"

# List hottest markets by volume
plugin-store polymarket markets --sort volume --limit 10

# Get event details
plugin-store polymarket event <event_id>

# Check Yes/No price
plugin-store polymarket price <token_id>

# View orderbook
plugin-store polymarket book <token_id>

# Price history (daily)
plugin-store polymarket history <token_id> --interval 1d
```

### Trade

```bash
# Buy 100 USDC of Yes at 0.65
plugin-store polymarket buy --token <token_id> --amount 100 --price 0.65

# Sell 50 shares at 0.70
plugin-store polymarket sell --token <token_id> --amount 50 --price 0.70

# Check open orders
plugin-store polymarket orders

# Cancel an order
plugin-store polymarket cancel <order_id>

# View positions and balance
plugin-store polymarket positions
plugin-store polymarket balance
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store polymarket search <query>` | No | Search prediction markets |
| 2 | `plugin-store polymarket markets` | No | List popular/active markets |
| 3 | `plugin-store polymarket event <id>` | No | Get event details with related markets |
| 4 | `plugin-store polymarket price <token_id>` | No | Get Yes/No price, midpoint, spread |
| 5 | `plugin-store polymarket book <token_id>` | No | View orderbook depth |
| 6 | `plugin-store polymarket history <token_id>` | No | Price history K-line |
| 7 | `plugin-store polymarket buy` | Yes | Buy outcome shares (limit order) |
| 8 | `plugin-store polymarket sell` | Yes | Sell outcome shares |
| 9 | `plugin-store polymarket cancel <order_id>` | Yes | Cancel an order |
| 10 | `plugin-store polymarket orders` | Yes | View open orders |
| 11 | `plugin-store polymarket positions` | Yes | View current positions |
| 12 | `plugin-store polymarket balance` | Yes | View USDC balance |

## Cross-Skill Workflows

### Workflow A: Search and Buy (most common)

> User: "What are the hottest prediction markets right now?"

```
1. polymarket markets --sort volume --limit 5     → show top markets
       ↓ user picks one
2. polymarket price <token_id>                     → show Yes/No prices
       ↓ user wants to buy
3. Check EVM_PRIVATE_KEY is set
       ↓ not set → prompt user to add to .env
       ↓ set → continue
4. polymarket buy --token <token_id> --amount 100 --price 0.65
       ↓
5. "Order placed! 100 USDC @ 0.65 for ~153 Yes shares."
```

**Data handoff:**
- `clobTokenIds` from market data → `<token_id>` for price/buy/sell commands
- Markets typically have 2 token IDs: index 0 = Yes, index 1 = No

### Workflow B: Portfolio Review

```
1. polymarket positions                            → show all holdings with PnL
2. polymarket price <token_id>                     → check current price of a position
3. polymarket sell --token <token_id> --amount 50 --price 0.70  → take profit
```

### Workflow C: Market Research

```
1. polymarket search "AI"                          → find AI-related markets
2. polymarket event <event_id>                     → see all markets in the event
3. polymarket book <token_id>                      → check liquidity depth
4. polymarket history <token_id> --interval 1d     → price trend
```

### Workflow D: With OKX Skills

```
1. okx-wallet-portfolio balance --chain polygon    → check USDC on Polygon
2. polymarket search "ethereum"                    → find ETH prediction markets
3. polymarket buy ...                              → place bet
```

## Operation Flow

### Step 1: Identify Intent

- Browse markets → `search` or `markets`
- Check price/odds → `price`
- Analyze depth → `book`
- Check trend → `history`
- Buy/sell → `buy` / `sell`
- Manage orders → `orders` / `cancel`
- Check portfolio → `positions` / `balance`

### Step 2: Collect Parameters

- Missing search query → ask user what topic they're interested in
- Missing token_id → use `search` or `markets` first, then extract `clobTokenIds` from the result
- Missing price for buy/sell → show current price via `price` command, suggest using midpoint
- Missing amount → ask user how much USDC they want to spend
- Missing private key (for trading) → prompt to set `EVM_PRIVATE_KEY` in `.env`

### Step 3: Execute

- **Data phase**: show market info, prices, let user make informed decision
- **Confirmation phase**: before any buy/sell, display price, amount, expected shares, and ask for confirmation
- **Execution phase**: place order, show result

### Step 4: Suggest Next Steps

| Just completed | Suggest |
|---|---|
| `search` or `markets` | 1. Check price of a specific market → `price` 2. View orderbook → `book` |
| `price` | 1. Buy/sell shares 2. View price history → `history` |
| `buy` or `sell` | 1. Check order status → `orders` 2. View updated positions → `positions` |
| `positions` | 1. Check current price of a holding → `price` 2. Sell a position → `sell` |

Present conversationally — never expose skill names or endpoint paths to the user.

## CLI Command Reference

### 1. plugin-store polymarket search

```bash
plugin-store polymarket search <query> [--limit <n>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<query>` | Yes | - | Search keywords |
| `--limit` | No | 20 | Max results |

**Key return fields per market:**

| Field | Description |
|---|---|
| `condition_id` | Market unique identifier |
| `question` | Market question |
| `outcomes` | Result options (usually `["Yes","No"]`) |
| `outcomePrices` | Current prices (e.g. `["0.65","0.35"]`) |
| `clobTokenIds` | Token IDs for CLOB operations — **use these for price/buy/sell** |
| `volume` | Total volume (USDC) |
| `liquidity` | Current liquidity |
| `endDate` | Market end date |

### 2. plugin-store polymarket markets

```bash
plugin-store polymarket markets [--tag <tag>] [--sort <sort>] [--limit <n>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--tag` | No | - | Filter: politics, crypto, sports, etc. |
| `--sort` | No | volume | Sort: volume, liquidity, newest, ending |
| `--limit` | No | 20 | Max results |

### 3. plugin-store polymarket event

```bash
plugin-store polymarket event <event_id>
```

### 4. plugin-store polymarket price

```bash
plugin-store polymarket price <token_id>
```

**Return fields:**

| Field | Description |
|---|---|
| `buy` | Buy price (probability of Yes) |
| `sell` | Sell price |
| `midpoint` | Mid price |
| `spread` | Bid-ask spread |

### 5. plugin-store polymarket book

```bash
plugin-store polymarket book <token_id>
```

**Return fields:**

| Field | Description |
|---|---|
| `bids` | Buy orders `[{price, size}]` |
| `asks` | Sell orders `[{price, size}]` |

### 6. plugin-store polymarket history

```bash
plugin-store polymarket history <token_id> [--interval <interval>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--interval` | No | 1d | 1m, 1h, 6h, 1d, 1w, max |

### 7. plugin-store polymarket buy

```bash
plugin-store polymarket buy --token <token_id> --amount <usdc> --price <0-1> [--order-type GTC]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--token` | Yes | - | Outcome token ID |
| `--amount` | Yes | - | USDC to spend |
| `--price` | Yes | - | Limit price (0-1) |
| `--order-type` | No | GTC | GTC, FOK, or GTD |

### 8. plugin-store polymarket sell

```bash
plugin-store polymarket sell --token <token_id> --amount <shares> --price <0-1> [--order-type GTC]
```

### 9. plugin-store polymarket cancel

```bash
plugin-store polymarket cancel <order_id>
```

### 10. plugin-store polymarket orders

```bash
plugin-store polymarket orders [--market <condition_id>]
```

### 11. plugin-store polymarket positions

```bash
plugin-store polymarket positions
```

### 12. plugin-store polymarket balance

```bash
plugin-store polymarket balance
```

## Key Concepts

- **Prices are probabilities**: Price 0.65 means 65% implied probability. Buying Yes at 0.65 means you pay $0.65 per share and receive $1.00 if Yes wins.
- **Two token IDs per market**: `clobTokenIds[0]` = Yes outcome, `clobTokenIds[1]` = No outcome.
- **USDC on Polygon**: All trading uses USDC on Polygon network.
- **Order book model**: Polymarket uses a CLOB (central limit order book), not AMM. Orders may not fill immediately.

## Edge Cases

- **Market closed/resolved**: Check `active` and `closed` fields. Don't allow trading on closed markets.
- **Low liquidity**: If `book` shows thin orders, warn user about slippage.
- **Price out of range**: Price must be strictly between 0 and 1. Reject 0 or 1.
- **Private key not set**: For trading commands, show clear error: "Set EVM_PRIVATE_KEY in your .env file"
- **Rate limiting**: Polymarket API has rate limits. Use retry with backoff.
