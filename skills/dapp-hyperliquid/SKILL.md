---
name: dapp-hyperliquid
description: >-
  This skill should be used when the user asks about Hyperliquid, perpetual futures,
  'open a long position', 'short BTC', 'check my perp positions', 'funding rate',
  'Hyperliquid orderbook', 'spot trade on Hyperliquid', 'set leverage', or mentions
  Hyperliquid DEX, perpetual trading, funding rates, or leverage trading. Covers
  perpetual and spot markets, prices, orderbook, funding rates, and trading
  (buy/sell/cancel). Do NOT use for Aave lending — use okx-dapp-aave instead.
  Do NOT use for Polymarket prediction markets — use okx-dapp-polymarket instead.
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Hyperliquid Perpetual & Spot Trading CLI

11 commands for perpetual futures and spot trading on Hyperliquid: market data, orderbook, funding rates, and order management.

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

- For token search / analytics -> use `okx-dex-token`
- For DEX swap -> use `okx-dex-swap`
- For token prices / charts -> use `okx-dex-market`
- For wallet balances -> use `okx-wallet-portfolio`
- For transaction broadcasting -> use `okx-onchain-gateway`
- For prediction markets -> use `okx-dapp-polymarket`
- For DeFi lending -> use `okx-dapp-aave`

## Authentication

**Data commands (markets, spot-markets, price, orderbook, funding):** No authentication needed. Work immediately.

**Trading commands (buy, sell, cancel, positions, balances, orders):** Require an EVM wallet private key:

```bash
# Add to .env file
EVM_PRIVATE_KEY=0x...
```

The private key is used to sign Hyperliquid L1 actions via EIP-712 typed data signatures.

## Quickstart

### Browse and Research

```bash
# List all perpetual markets
plugin-store hyperliquid markets

# List all spot markets
plugin-store hyperliquid spot-markets

# Get BTC mid price
plugin-store hyperliquid price BTC

# View BTC orderbook
plugin-store hyperliquid orderbook BTC

# Check BTC funding rate
plugin-store hyperliquid funding BTC
```

### Trade

```bash
# Open a long position: buy 0.01 BTC at $65000 with 10x leverage
plugin-store hyperliquid buy --symbol BTC --size 0.01 --price 65000 --leverage 10

# Open a short position: sell 0.5 ETH at $3500
plugin-store hyperliquid sell --symbol ETH --size 0.5 --price 3500

# Cancel an order
plugin-store hyperliquid cancel --symbol BTC --order-id 123456

# Check positions and balances
plugin-store hyperliquid positions
plugin-store hyperliquid balances
plugin-store hyperliquid orders
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store hyperliquid markets` | No | List perpetual markets (price, leverage, volume) |
| 2 | `plugin-store hyperliquid spot-markets` | No | List spot markets |
| 3 | `plugin-store hyperliquid price <symbol>` | No | Real-time mid price for a symbol |
| 4 | `plugin-store hyperliquid orderbook <symbol>` | No | L2 order book snapshot |
| 5 | `plugin-store hyperliquid funding <symbol>` | No | Current and historical funding rates |
| 6 | `plugin-store hyperliquid buy --symbol <s> --size <n> --price <p> [--leverage <l>]` | Yes | Buy (long perp or spot buy) |
| 7 | `plugin-store hyperliquid sell --symbol <s> --size <n> --price <p>` | Yes | Sell (short perp or spot sell) |
| 8 | `plugin-store hyperliquid cancel --symbol <s> --order-id <oid>` | Yes | Cancel an open order |
| 9 | `plugin-store hyperliquid positions` | Yes | View perpetual positions |
| 10 | `plugin-store hyperliquid balances` | Yes | View USDC margin and spot balances |
| 11 | `plugin-store hyperliquid orders [--symbol <s>]` | Yes | List open orders |

## Cross-Skill Workflows

### Workflow A: Research and Trade (most common)

> User: "What's the funding rate on BTC? I want to open a long."

```
1. hyperliquid funding BTC                          -> show current and historical funding rates
       | user decides to go long
2. hyperliquid price BTC                            -> check current mid price
3. hyperliquid orderbook BTC                        -> check spread and liquidity
       | user confirms trade
4. Check EVM_PRIVATE_KEY is set
       | not set -> prompt user to add to .env
       | set -> continue
5. hyperliquid buy --symbol BTC --size 0.01 --price 65000 --leverage 10
       |
6. "Opened long 0.01 BTC at $65,000 with 10x leverage. Liquidation price: $58,500."
```

**Data handoff:**
- `symbol` from markets data -> `--symbol` for trading commands
- Mid price from `price` -> reference for `--price` in buy/sell
- Funding rate from `funding` -> helps user decide long vs short

### Workflow B: Position Management

```
1. hyperliquid positions                             -> show all open perp positions
2. hyperliquid orders --symbol BTC                   -> check pending orders on BTC
3. hyperliquid cancel --symbol BTC --order-id 123456 -> cancel a stale order
4. hyperliquid sell --symbol BTC --size 0.01 --price 66000  -> close long by selling
```

### Workflow C: Spot Trading

```
1. hyperliquid spot-markets                          -> browse available spot pairs
2. hyperliquid price PURR                            -> check price of a spot token
3. hyperliquid orderbook PURR                        -> check liquidity
4. hyperliquid buy --symbol PURR --size 100 --price 0.50  -> buy spot tokens
```

### Workflow D: With OKX Skills

```
1. okx-wallet-portfolio balance --chain arbitrum     -> check USDC balance on Arbitrum
2. hyperliquid markets                               -> browse perp markets
3. hyperliquid buy --symbol ETH --size 1 --price 3500 --leverage 5  -> open position
4. hyperliquid positions                             -> verify position opened
```

## Operation Flow

### Step 1: Identify Intent

- Browse perpetual markets -> `markets`
- Browse spot markets -> `spot-markets`
- Check current price -> `price`
- Check orderbook depth -> `orderbook`
- Check funding rates -> `funding`
- Open a long / buy spot -> `buy`
- Open a short / sell spot -> `sell`
- Cancel an order -> `cancel`
- View open positions -> `positions`
- Check balances -> `balances`
- View open orders -> `orders`

### Step 2: Collect Parameters

- Missing symbol for price/orderbook/funding -> use `markets` or `spot-markets` first, then pick from the list
- Missing size for buy/sell -> ask user how much they want to trade
- Missing price for buy/sell -> use `price` to get current mid price, then ask user for limit price
- Missing leverage for buy -> default is current account leverage; ask if user wants to change it
- Missing order-id for cancel -> use `orders` to list open orders, then pick the one to cancel
- Missing private key (for trading) -> prompt to set `EVM_PRIVATE_KEY` in `.env`

### Step 3: Execute

- **Data phase**: show market info, prices, funding rates, orderbook depth to help user make an informed decision
- **Confirmation phase**: before any buy/sell, display symbol, size, price, leverage, estimated liquidation price, and ask for confirmation
- **Execution phase**: submit order, show result with order status

### Step 4: Suggest Next Steps

| Just completed | Suggest |
|---|---|
| `markets` | 1. Check price -> `price` 2. Check funding -> `funding` 3. View orderbook -> `orderbook` |
| `spot-markets` | 1. Check spot price -> `price` 2. Buy spot tokens -> `buy` |
| `price` | 1. View orderbook -> `orderbook` 2. Place order -> `buy` or `sell` |
| `orderbook` | 1. Place order -> `buy` or `sell` |
| `funding` | 1. Open position -> `buy` or `sell` 2. Check positions -> `positions` |
| `buy` | 1. Check positions -> `positions` 2. View open orders -> `orders` |
| `sell` | 1. Check positions -> `positions` 2. View open orders -> `orders` |
| `cancel` | 1. View remaining orders -> `orders` 2. Place new order -> `buy` or `sell` |
| `positions` | 1. Close position -> `sell` or `buy` 2. Check balances -> `balances` |
| `balances` | 1. Open position -> `buy` 2. Check positions -> `positions` |
| `orders` | 1. Cancel order -> `cancel` 2. Check positions -> `positions` |

Present conversationally -- never expose skill names or endpoint paths to the user.

## CLI Command Reference

### 1. plugin-store hyperliquid markets

```bash
plugin-store hyperliquid markets
```

No parameters required.

**Key return fields per market:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol (e.g. BTC, ETH, SOL) |
| `markPrice` | Current mark price |
| `midPrice` | Current mid price (best bid + best ask / 2) |
| `fundingRate` | Current hourly funding rate |
| `openInterest` | Total open interest in contracts |
| `volume24h` | 24-hour trading volume in USD |
| `maxLeverage` | Maximum allowed leverage |
| `szDecimals` | Size decimal precision for orders |

### 2. plugin-store hyperliquid spot-markets

```bash
plugin-store hyperliquid spot-markets
```

No parameters required.

**Key return fields per market:**

| Field | Description |
|---|---|
| `symbol` | Token symbol (e.g. PURR, HYPE) |
| `index` | Spot asset index (10000 + universe_index) |
| `markPrice` | Current mark price |
| `midPrice` | Current mid price |
| `volume24h` | 24-hour trading volume |
| `szDecimals` | Size decimal precision for orders |

### 3. plugin-store hyperliquid price

```bash
plugin-store hyperliquid price <symbol>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<symbol>` | Yes | - | Asset symbol (e.g. BTC, ETH, SOL, PURR) |

**Return fields:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol |
| `midPrice` | Current mid price |
| `markPrice` | Current mark price (for perps) |
| `bestBid` | Best bid price |
| `bestAsk` | Best ask price |
| `spread` | Bid-ask spread |

### 4. plugin-store hyperliquid orderbook

```bash
plugin-store hyperliquid orderbook <symbol>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<symbol>` | Yes | - | Asset symbol (e.g. BTC, ETH) |

**Return fields:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol |
| `bids` | List of bid levels `[{price, size}]` |
| `asks` | List of ask levels `[{price, size}]` |
| `midPrice` | Mid price derived from best bid/ask |
| `spread` | Spread between best bid and ask |

### 5. plugin-store hyperliquid funding

```bash
plugin-store hyperliquid funding <symbol>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<symbol>` | Yes | - | Asset symbol (e.g. BTC, ETH) |

**Return fields:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol |
| `currentRate` | Current hourly funding rate (positive = longs pay shorts) |
| `predictedRate` | Predicted next funding rate |
| `annualizedRate` | Annualized funding rate (currentRate * 8760) |
| `history` | Recent funding rate history `[{time, rate}]` |
| `premium` | Current premium of mark price vs index price |

### 6. plugin-store hyperliquid buy

```bash
plugin-store hyperliquid buy --symbol <symbol> --size <size> --price <price> [--leverage <leverage>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--symbol` | Yes | - | Asset symbol (e.g. BTC, ETH, PURR) |
| `--size` | Yes | - | Order size in base asset units (must respect szDecimals) |
| `--price` | Yes | - | Limit price in USD |
| `--leverage` | No | Current setting | Leverage multiplier (1-50, varies by asset) |

**Return fields:**

| Field | Description |
|---|---|
| `status` | Order status (placed, filled, partially_filled, rejected) |
| `orderId` | Order identifier |
| `symbol` | Asset traded |
| `side` | "buy" |
| `size` | Order size |
| `price` | Limit price |
| `leverage` | Leverage used |
| `liquidationPrice` | Estimated liquidation price (for perps) |

### 7. plugin-store hyperliquid sell

```bash
plugin-store hyperliquid sell --symbol <symbol> --size <size> --price <price>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--symbol` | Yes | - | Asset symbol (e.g. BTC, ETH, PURR) |
| `--size` | Yes | - | Order size in base asset units (must respect szDecimals) |
| `--price` | Yes | - | Limit price in USD |

**Return fields:**

| Field | Description |
|---|---|
| `status` | Order status (placed, filled, partially_filled, rejected) |
| `orderId` | Order identifier |
| `symbol` | Asset traded |
| `side` | "sell" |
| `size` | Order size |
| `price` | Limit price |
| `liquidationPrice` | Estimated liquidation price (for perp shorts) |

### 8. plugin-store hyperliquid cancel

```bash
plugin-store hyperliquid cancel --symbol <symbol> --order-id <order-id>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--symbol` | Yes | - | Asset symbol the order was placed on |
| `--order-id` | Yes | - | Order ID to cancel (from `orders` or `buy`/`sell` response) |

**Return fields:**

| Field | Description |
|---|---|
| `status` | Cancellation status (cancelled, not_found, error) |
| `orderId` | Order ID that was cancelled |
| `symbol` | Asset symbol |

### 9. plugin-store hyperliquid positions

```bash
plugin-store hyperliquid positions
```

No parameters required. Uses wallet derived from `EVM_PRIVATE_KEY`.

**Key return fields per position:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol |
| `side` | Position direction (long, short) |
| `size` | Position size in base asset |
| `entryPrice` | Average entry price |
| `markPrice` | Current mark price |
| `unrealizedPnl` | Unrealized profit/loss in USD |
| `leverage` | Current leverage |
| `liquidationPrice` | Price at which position is liquidated |
| `marginUsed` | Margin allocated to this position |
| `returnOnEquity` | ROE percentage |

### 10. plugin-store hyperliquid balances

```bash
plugin-store hyperliquid balances
```

No parameters required. Uses wallet derived from `EVM_PRIVATE_KEY`.

**Return fields:**

| Field | Description |
|---|---|
| `accountValue` | Total account value in USD |
| `marginUsed` | Total margin allocated to positions |
| `availableBalance` | Free balance available for new orders |
| `crossMarginSummary` | Cross-margin account details |
| `spotBalances` | List of spot token balances `[{token, amount, valueUSD}]` |

### 11. plugin-store hyperliquid orders

```bash
plugin-store hyperliquid orders [--symbol <symbol>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--symbol` | No | All symbols | Filter orders by asset symbol |

**Key return fields per order:**

| Field | Description |
|---|---|
| `orderId` | Order identifier |
| `symbol` | Asset symbol |
| `side` | Order side (buy, sell) |
| `size` | Order size |
| `filledSize` | Amount already filled |
| `price` | Limit price |
| `status` | Order status (open, partially_filled) |
| `createdAt` | Order creation timestamp |
| `orderType` | Type of order (limit) |

## Key Concepts

- **Perpetual Futures**: Contracts with no expiry date that track an underlying index price. Positions can be held indefinitely, with funding payments keeping the contract price aligned with the index.
- **Funding Rate**: Hourly payment exchanged between longs and shorts to keep the perpetual price close to the index. Positive rate means longs pay shorts; negative means shorts pay longs. Capped at 4% per hour.
- **Leverage**: Multiplier on position exposure relative to margin posted. BTC supports up to 50x, other assets vary. Higher leverage means smaller margin requirement but closer liquidation price.
- **Cross Margin vs Isolated Margin**: Hyperliquid uses cross margin by default -- all positions share the same margin pool. This means PnL from one position can offset losses in another, but also means a large loss can affect all positions.
- **Liquidation Price**: The mark price at which a position is force-closed because margin is insufficient. Calculate by considering entry price, leverage, and maintenance margin. Always monitor this value.
- **szDecimals**: Each asset has a specific size precision. For example, BTC has szDecimals=5, meaning the smallest order increment is 0.00001 BTC. Orders with incorrect precision are rejected.
- **Spot Asset Index**: Spot assets on Hyperliquid are identified by index = 10000 + universe_index. For example, if PURR is at universe index 0, its spot index is 10000.
- **Mark Price vs Mid Price**: Mark price is the fair price used for PnL and liquidation calculations (based on index price and funding). Mid price is simply (best bid + best ask) / 2 from the orderbook.
- **USDC Margin**: All perpetual positions are margined in USDC. You need USDC on Hyperliquid L1 to trade perps.

## Edge Cases

- **Insufficient margin**: If the user tries to open a position larger than their available balance supports at the given leverage, the order will be rejected. Check balances first via `balances`.
- **Invalid size precision**: Orders must respect the `szDecimals` for each asset. Use `markets` to check the required precision before placing orders. Orders with too many decimal places are rejected.
- **Leverage limits**: Maximum leverage varies by asset (e.g. 50x for BTC, 20x for smaller assets). Attempting to exceed the limit will fail. Check `maxLeverage` from `markets`.
- **Self-trade prevention**: Hyperliquid prevents orders that would trade against yourself. If you have a resting sell and place a crossing buy, the system may cancel one.
- **Private key not set**: For trading commands, show clear error: "Set EVM_PRIVATE_KEY in your .env file"
- **Symbol not found**: If a symbol is not recognized, suggest using `markets` or `spot-markets` to see available assets.
- **Rate limiting**: Hyperliquid has rate limits on API calls. Use retry with backoff.
- **Minimum order size**: Each asset has a minimum order size. Very small orders below this threshold will be rejected.
- **Funding accrual**: Funding is settled continuously. When holding positions across funding intervals, PnL will include accumulated funding payments.
- **Spot vs perp symbols**: Some symbols exist in both perp and spot markets. The CLI resolves this based on context -- `buy`/`sell` default to perp unless the symbol is spot-only.
