---
name: dapp-aave
description: "This skill should be used when the user asks about Aave, lending, borrowing, 'supply to Aave', 'deposit into Aave', 'withdraw from Aave', 'Aave APY', 'Aave markets', 'lending rates', 'health factor', or mentions Aave V3, DeFi lending, supply/withdraw assets, or checking lending rates. Covers market data, reserve details, account positions, and supply/withdraw operations on Ethereum, Polygon, and Arbitrum. Do NOT use for DEX swaps — use okx-dex-swap instead. Do NOT use for prediction markets — use okx-dapp-polymarket instead."
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Aave V3 Lending Protocol CLI

7 commands for lending market data, reserve details, account positions, supply/withdraw, and borrow/repay operations.

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
- For prediction markets → use `okx-dapp-polymarket`

## Authentication

**Data commands (markets, reserve, account):** No authentication needed. Work immediately.

**Transaction commands (supply, withdraw, borrow, repay):** Require an EVM wallet private key:

```bash
# Add to .env file
EVM_PRIVATE_KEY=0x...
```

The private key is used to sign supply, withdraw, borrow, and repay transactions on-chain.

## Quickstart

### Browse and Research

```bash
# List all Aave V3 markets on Ethereum
plugin-store aave markets --chain ethereum

# Get reserve details for USDC
plugin-store aave reserve USDC --chain ethereum

# Check account positions
plugin-store aave account 0xYourAddress --chain ethereum
```

### Supply and Withdraw

```bash
# Supply 100 USDC to Aave on Ethereum
plugin-store aave supply --token USDC --amount 100 --chain ethereum

# Withdraw 50 USDC from Aave
plugin-store aave withdraw --token USDC --amount 50 --chain ethereum

# Withdraw all USDC (full balance)
plugin-store aave withdraw --token USDC --amount max --chain ethereum
```

### Borrow and Repay

```bash
# Borrow 500 USDC against your collateral (variable rate)
plugin-store aave borrow --token USDC --amount 500 --chain ethereum

# Repay 200 USDC of your debt
plugin-store aave repay --token USDC --amount 200 --chain ethereum

# Repay all outstanding USDC debt
plugin-store aave repay --token USDC --amount max --chain ethereum
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store aave markets --chain <chain>` | No | List all Aave V3 reserve markets on a chain |
| 2 | `plugin-store aave reserve <symbol> --chain <chain>` | No | Get detailed reserve data (APY, liquidity, config) |
| 3 | `plugin-store aave account <address> --chain <chain>` | No | View account positions, health factor, borrowing power |
| 4 | `plugin-store aave supply --token <symbol> --amount <n> --chain <chain>` | Yes | Supply assets to earn yield |
| 5 | `plugin-store aave withdraw --token <symbol> --amount <n> --chain <chain>` | Yes | Withdraw supplied assets (use "max" for full withdrawal) |
| 6 | `plugin-store aave borrow --token <symbol> --amount <n> --chain <chain>` | Yes | Borrow assets at variable rate against collateral |
| 7 | `plugin-store aave repay --token <symbol> --amount <n> --chain <chain>` | Yes | Repay borrowed assets (use "max" for full repayment) |

## Cross-Skill Workflows

### Workflow A: Research and Supply (most common)

> User: "What are the best lending rates on Aave right now?"

```
1. aave markets --chain ethereum                  → show all reserves with APYs
       ↓ user picks an asset
2. aave reserve USDC --chain ethereum              → show detailed reserve info
       ↓ user wants to supply
3. Check EVM_PRIVATE_KEY is set
       ↓ not set → prompt user to add to .env
       ↓ set → continue
4. aave supply --token USDC --amount 100 --chain ethereum
       ↓
5. "Supplied 100 USDC to Aave V3. You will receive aUSDC as a receipt token."
```

**Data handoff:**
- `symbol` from markets data → `<symbol>` for reserve/supply/withdraw commands
- `underlyingAsset` address is resolved automatically from the symbol

### Workflow B: Portfolio Review and Withdrawal

```
1. aave account 0xYourAddress --chain ethereum     → show positions, health factor
2. aave reserve WETH --chain ethereum              → check current rates on a position
3. aave withdraw --token WETH --amount max --chain ethereum  → withdraw full balance
```

### Workflow C: Multi-Chain Comparison

```
1. aave markets --chain ethereum                   → Ethereum rates
2. aave markets --chain polygon                    → Polygon rates
3. aave markets --chain arbitrum                   → Arbitrum rates
4. aave supply --token USDC --amount 100 --chain arbitrum  → supply on best chain
```

### Workflow D: With OKX Skills

```
1. okx-wallet-portfolio balance --chain ethereum   → check token balances
2. aave markets --chain ethereum                   → check lending rates
3. aave supply --token USDC --amount 100 --chain ethereum  → supply to Aave
```

## Operation Flow

### Step 1: Identify Intent

- Browse markets/rates → `markets`
- Check specific asset details → `reserve`
- Check account status → `account`
- Deposit/lend assets → `supply`
- Withdraw assets → `withdraw`
- Borrow against collateral → `borrow`
- Repay debt → `repay`

### Step 2: Collect Parameters

- Missing chain → ask user which chain (ethereum, polygon, or arbitrum)
- Missing symbol for reserve → use `markets` first, then pick from the list
- Missing address for account → ask user for their wallet address
- Missing amount for supply/withdraw/borrow/repay → ask user how much
- Missing private key (for supply/withdraw/borrow/repay) → prompt to set `EVM_PRIVATE_KEY` in `.env`

### Step 3: Execute

- **Data phase**: show market info, rates, positions, let user make informed decision
- **Confirmation phase**: before any supply/withdraw, display token, amount, chain, estimated APY, and ask for confirmation
- **Execution phase**: submit transaction, show result with tx hash

### Step 4: Suggest Next Steps

| Just completed | Suggest |
|---|---|
| `markets` | 1. Check specific reserve details → `reserve` 2. View account positions → `account` |
| `reserve` | 1. Supply assets → `supply` 2. Compare with other chains → `markets` on another chain |
| `account` | 1. Withdraw assets → `withdraw` 2. Supply more → `supply` |
| `supply` | 1. Check updated position → `account` 2. View reserve APY → `reserve` |
| `withdraw` | 1. Check updated position → `account` 2. View wallet balance → `okx-wallet-portfolio` |
| `borrow` | 1. Check health factor → `account` 2. Monitor borrow rate → `reserve` |
| `repay` | 1. Check updated debt → `account` 2. View wallet balance → `okx-wallet-portfolio` |

Present conversationally — never expose skill names or endpoint paths to the user.

## CLI Command Reference

### 1. plugin-store aave markets

```bash
plugin-store aave markets --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--chain` | Yes | - | Chain: ethereum, polygon, arbitrum |

**Key return fields per reserve:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol (e.g. USDC, WETH, DAI) |
| `underlyingAsset` | Token contract address |
| `supplyAPY` | Current annual supply yield (as decimal, multiply by 100 for %) |
| `borrowAPY` | Current annual borrow rate |
| `totalSupply` | Total amount supplied |
| `totalBorrow` | Total amount borrowed |
| `availableLiquidity` | Amount available to borrow |
| `utilizationRate` | Ratio of borrowed to supplied |
| `ltv` | Loan-to-value ratio for collateral |
| `liquidationThreshold` | Threshold at which position can be liquidated |

### 2. plugin-store aave reserve

```bash
plugin-store aave reserve <symbol> --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<symbol>` | Yes | - | Asset symbol (e.g. USDC, WETH, DAI) |
| `--chain` | Yes | - | Chain: ethereum, polygon, arbitrum |

**Return fields:**

| Field | Description |
|---|---|
| `symbol` | Asset symbol |
| `supplyAPY` | Current supply APY |
| `variableBorrowAPY` | Variable borrow rate |
| `stableBorrowAPY` | Stable borrow rate (if available) |
| `totalSupply` | Total supplied amount |
| `totalBorrow` | Total borrowed amount |
| `availableLiquidity` | Available to borrow |
| `utilizationRate` | Current utilization |
| `ltv` | Max loan-to-value for collateral |
| `liquidationThreshold` | Liquidation trigger threshold |
| `liquidationBonus` | Bonus for liquidators |
| `reserveFactor` | Protocol fee on interest |
| `canBeCollateral` | Whether asset can be used as collateral |
| `borrowingEnabled` | Whether borrowing is enabled |

### 3. plugin-store aave account

```bash
plugin-store aave account <address> --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<address>` | Yes | - | Wallet address (0x...) |
| `--chain` | Yes | - | Chain: ethereum, polygon, arbitrum |

**Return fields:**

| Field | Description |
|---|---|
| `totalSuppliedUSD` | Total value of supplied assets in USD |
| `totalBorrowedUSD` | Total value of borrowed assets in USD |
| `netWorthUSD` | Net position value in USD |
| `healthFactor` | Health factor (< 1.0 = liquidatable) |
| `availableBorrowsUSD` | Remaining borrowing capacity in USD |
| `currentLTV` | Current loan-to-value ratio |
| `supplies` | List of supplied positions `[{symbol, amount, amountUSD, apy}]` |
| `borrows` | List of borrow positions `[{symbol, amount, amountUSD, apy}]` |

### 4. plugin-store aave supply

```bash
plugin-store aave supply --token <symbol> --amount <amount> --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--token` | Yes | - | Asset symbol (e.g. USDC, WETH) |
| `--amount` | Yes | - | Amount to supply (in token units) |
| `--chain` | Yes | - | Chain: ethereum, polygon, arbitrum |

**Return fields:**

| Field | Description |
|---|---|
| `txHash` | Transaction hash |
| `status` | Transaction status (pending, confirmed, failed) |
| `supplied` | Amount supplied |
| `token` | Token symbol |
| `chain` | Chain used |

### 5. plugin-store aave withdraw

```bash
plugin-store aave withdraw --token <symbol> --amount <amount> --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--token` | Yes | - | Asset symbol (e.g. USDC, WETH) |
| `--amount` | Yes | - | Amount to withdraw (in token units, or "max" for full withdrawal) |
| `--chain` | Yes | - | Chain: ethereum, polygon, arbitrum |

**Return fields:**

| Field | Description |
|---|---|
| `txHash` | Transaction hash |
| `status` | Transaction status (pending, confirmed, failed) |
| `withdrawn` | Amount withdrawn |
| `token` | Token symbol |
| `chain` | Chain used |

### 6. plugin-store aave borrow

```bash
plugin-store aave borrow --token <symbol> --amount <amount> --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--token` | Yes | - | Asset symbol to borrow (e.g. USDC, WETH) |
| `--amount` | Yes | - | Amount to borrow (in token units) |
| `--chain` | No | ethereum | Chain: ethereum, polygon, arbitrum |

**Return fields:**

| Field | Description |
|---|---|
| `txHash` | Transaction hash |
| `status` | Transaction status (success, reverted) |
| `borrowed` | Amount borrowed |
| `token` | Token symbol |
| `chain` | Chain used |

### 7. plugin-store aave repay

```bash
plugin-store aave repay --token <symbol> --amount <amount> --chain <chain>
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--token` | Yes | - | Asset symbol to repay (e.g. USDC, WETH) |
| `--amount` | Yes | - | Amount to repay (in token units, or "max" for full repayment) |
| `--chain` | No | ethereum | Chain: ethereum, polygon, arbitrum |

**Return fields:**

| Field | Description |
|---|---|
| `txHash` | Transaction hash |
| `status` | Transaction status (success, reverted) |
| `repaid` | Amount repaid |
| `token` | Token symbol |
| `chain` | Chain used |

## Key Concepts

- **Supply APY**: Annual percentage yield earned by supplying assets. Rates are variable and change based on utilization.
- **Health Factor**: Ratio indicating position safety. Above 1.0 is safe; below 1.0 means the position can be liquidated. Keep it well above 1.0 (recommended > 1.5).
- **aTokens**: Receipt tokens received when supplying (e.g. supply USDC, receive aUSDC). aTokens accrue interest automatically — their balance grows over time.
- **RAY**: Aave uses 27-decimal fixed-point numbers (1 RAY = 10^27) for rate calculations internally. The CLI converts these to human-readable percentages.
- **LTV (Loan-to-Value)**: Maximum percentage of collateral value that can be borrowed. E.g. 80% LTV means you can borrow up to 80% of your collateral value.
- **Liquidation Threshold**: The LTV level at which a position becomes eligible for liquidation. Always higher than the max LTV.
- **Utilization Rate**: Ratio of total borrowed to total supplied. Higher utilization = higher rates for both suppliers and borrowers.

## Edge Cases

- **Insufficient balance**: If the user tries to supply more than their wallet balance, the transaction will fail. Check balance first via `okx-wallet-portfolio`.
- **Health factor risk**: When withdrawing collateral or borrowing, warn if the resulting health factor would drop below 1.5. Withdrawing/borrowing too much can trigger liquidation.
- **Borrow capacity**: User must have sufficient collateral before borrowing. Check available borrows via `account` command.
- **Max repay**: Use `--amount max` to repay the full outstanding debt including accrued interest.
- **Max withdrawal**: Use `--amount max` to withdraw the full supplied balance including accrued interest. This uses `type(uint256).max` internally.
- **Token approval**: First-time supply of a token requires an ERC-20 approval transaction. The CLI handles this automatically but it will use additional gas.
- **Private key not set**: For supply/withdraw commands, show clear error: "Set EVM_PRIVATE_KEY in your .env file"
- **Unsupported chain**: Only ethereum, polygon, and arbitrum are supported. Other chains will return an error.
- **Asset not listed**: If a token symbol is not recognized as an Aave V3 reserve, suggest using `markets` to see available assets.
- **Rate limiting**: OKX API has rate limits. Use retry with backoff.
