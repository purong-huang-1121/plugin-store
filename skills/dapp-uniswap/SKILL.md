---
name: dapp-uniswap
description: "This skill should be used when the user asks about Uniswap, 'swap on Uniswap', 'Uniswap V3 swap', 'Uniswap quote', 'get a swap quote', 'swap WETH for USDC on Uniswap', 'trade tokens on Uniswap', 'Uniswap fee tiers', 'concentrated liquidity swap', or mentions Uniswap V3, on-chain token swaps via Uniswap, or checking swap prices/quotes. Covers swap quotes, swap execution, and token address lookup on Ethereum, Arbitrum, and Polygon. Do NOT use for general DEX aggregator swaps — use okx-dex-swap instead. Do NOT use for lending — use dapp-aave instead."
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Uniswap V3 DEX Swap CLI

3 commands for swap quotes, swap execution, and token address lookup on Uniswap V3.

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

- For DEX aggregator swaps (multi-source routing) → use `okx-dex-swap`
- For token search / analytics → use `okx-dex-token`
- For token prices / charts → use `okx-dex-market`
- For wallet balances → use `okx-wallet-portfolio`
- For transaction broadcasting → use `okx-onchain-gateway`
- For lending / borrowing → use `dapp-aave`
- For prediction markets → use `dapp-polymarket`

## Authentication

**Data commands (quote, tokens):** The `quote` command requires `EVM_PRIVATE_KEY` to be set (it uses the on-chain QuoterV2 contract), but does not spend any gas or sign transactions. The `tokens` command requires no authentication.

**Transaction commands (swap):** Require an EVM wallet private key:

```bash
# Add to .env file
EVM_PRIVATE_KEY=0x...
```

The private key is used to:
1. Approve the Uniswap SwapRouter02 to spend your input token (ERC-20 approval)
2. Sign and broadcast the swap transaction on-chain

## Quickstart

### Get a Swap Quote

```bash
# Quote swapping 0.05 WETH to wstETH on Arbitrum (default chain, 0.01% fee tier)
plugin-store uniswap quote --from WETH --to wstETH --amount 0.05

# Quote swapping 100 USDC to WETH on Ethereum with 0.3% fee tier
plugin-store uniswap quote --from USDC --to WETH --amount 100 --chain ethereum --fee 3000

# Quote using a contract address as the token
plugin-store uniswap quote --from 0xaf88d065e77c8cC2239327C5EDb3A432268e5831 --to WETH --amount 50 --chain arbitrum
```

### Execute a Swap

```bash
# Swap 0.05 WETH to wstETH on Arbitrum with default 0.5% slippage
plugin-store uniswap swap --from WETH --to wstETH --amount 0.05

# Swap 100 USDC to WETH on Ethereum with 0.3% fee and 1% slippage
plugin-store uniswap swap --from USDC --to WETH --amount 100 --chain ethereum --fee 3000 --slippage 100
```

### List Available Tokens

```bash
# List well-known tokens on Arbitrum (default)
plugin-store uniswap tokens

# List well-known tokens on Ethereum
plugin-store uniswap tokens --chain ethereum

# List well-known tokens on Polygon
plugin-store uniswap tokens --chain polygon
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store uniswap quote --from <token> --to <token> --amount <n> [--chain <chain>] [--fee <bps>]` | Yes* | Get estimated swap output without executing |
| 2 | `plugin-store uniswap swap --from <token> --to <token> --amount <n> [--chain <chain>] [--fee <bps>] [--slippage <bps>]` | Yes | Execute an on-chain swap via Uniswap V3 SwapRouter02 |
| 3 | `plugin-store uniswap tokens [--chain <chain>]` | No | List well-known token symbols and addresses for a chain |

*The `quote` command requires `EVM_PRIVATE_KEY` to be set but does not sign or broadcast transactions.

## Cross-Skill Workflows

### Workflow A: Quote Then Swap (most common)

> User: "I want to swap some WETH for wstETH on Arbitrum"

```
1. uniswap tokens --chain arbitrum                  → confirm token availability
       ↓
2. uniswap quote --from WETH --to wstETH --amount 0.05 --chain arbitrum
       ↓ show estimated output and fee tier
3. Check EVM_PRIVATE_KEY is set
       ↓ not set → prompt user to add to .env
       ↓ set → continue
4. uniswap swap --from WETH --to wstETH --amount 0.05 --chain arbitrum
       ↓
5. "Swapped 0.05 WETH for ~0.0425 wstETH on Arbitrum. Tx: 0x..."
```

**Data handoff:**
- Token symbols or addresses from `tokens` output → `--from` / `--to` for quote and swap
- Quote output shows expected amount → user confirms before executing swap

### Workflow B: Compare Fee Tiers

```
1. uniswap quote --from WETH --to USDC --amount 1 --fee 100     → 0.01% pool
2. uniswap quote --from WETH --to USDC --amount 1 --fee 500     → 0.05% pool
3. uniswap quote --from WETH --to USDC --amount 1 --fee 3000    → 0.3% pool
4. uniswap swap --from WETH --to USDC --amount 1 --fee <best>   → execute on best pool
```

### Workflow C: Multi-Chain Comparison

```
1. uniswap quote --from WETH --to USDC --amount 1 --chain arbitrum
2. uniswap quote --from WETH --to USDC --amount 1 --chain ethereum
3. uniswap quote --from WETH --to USDC --amount 1 --chain polygon
4. uniswap swap --from WETH --to USDC --amount 1 --chain <best>
```

### Workflow D: With OKX Skills

```
1. okx-wallet-portfolio balance --chain arbitrum   → check token balances
2. uniswap tokens --chain arbitrum                 → see available pairs
3. uniswap quote --from WETH --to wstETH --amount 0.05 --chain arbitrum
4. uniswap swap --from WETH --to wstETH --amount 0.05 --chain arbitrum
5. okx-wallet-portfolio balance --chain arbitrum   → verify updated balances
```

### Workflow E: Swap Then Supply to Aave

```
1. uniswap swap --from WETH --to USDC --amount 1 --chain ethereum
2. dapp-aave supply --token USDC --amount 1500 --chain ethereum
```

## Operation Flow

### Step 1: Identify Intent

- Check swap price/output → `quote`
- Execute a swap → `swap`
- Find token addresses → `tokens`

### Step 2: Collect Parameters

- Missing `--from` or `--to` → ask user which tokens they want to swap. Use `tokens` to show available well-known symbols.
- Missing `--amount` → ask user how much of the input token they want to swap.
- Missing `--chain` → default is `arbitrum`. If the user mentions a specific chain, use that.
- Missing `--fee` → default is `100` (0.01%). For stablecoin pairs, suggest `100` or `500`. For volatile pairs, suggest `3000`.
- Missing `--slippage` → default is `50` (0.5%). For volatile markets, suggest `100` (1%).
- Missing `EVM_PRIVATE_KEY` → prompt to set it in `.env`.
- Unknown token symbol → suggest using the contract address directly with `0x...` prefix. Use `tokens` to list known symbols.

### Step 3: Execute

- **Data phase**: run `quote` to show expected output, fee tier, and chain
- **Confirmation phase**: before any swap, display input amount, expected output, minimum output (after slippage), fee tier, and chain. Ask for confirmation.
- **Execution phase**: submit swap transaction, show result with tx hash, actual output, and block number

### Step 4: Suggest Next Steps

| Just completed | Suggest |
|---|---|
| `tokens` | 1. Get a swap quote → `quote` 2. Check wallet balance → `okx-wallet-portfolio` |
| `quote` | 1. Execute the swap → `swap` 2. Compare fee tiers → run `quote` with different `--fee` values |
| `swap` | 1. Check wallet balance → `okx-wallet-portfolio` 2. Supply to Aave → `dapp-aave supply` |

Present conversationally — never expose skill names or endpoint paths to the user.

## CLI Command Reference

### 1. plugin-store uniswap quote

Get an estimated swap output from Uniswap V3 QuoterV2 contract without executing a transaction.

```bash
plugin-store uniswap quote --from <token> --to <token> --amount <amount> [--chain <chain>] [--fee <bps>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--from` | Yes | - | Input token symbol (e.g. WETH, USDC) or contract address (0x...) |
| `--to` | Yes | - | Output token symbol (e.g. wstETH, USDC) or contract address (0x...) |
| `--amount` | Yes | - | Amount of input token in human-readable units (e.g. "0.05", "100") |
| `--chain` | No | arbitrum | Chain: arbitrum, ethereum, polygon |
| `--fee` | No | 100 | Pool fee tier in basis points: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%) |

**Return fields:**

| Field | Description |
|---|---|
| `from` | Input token symbol (uppercased) |
| `to` | Output token symbol (uppercased) |
| `amount_in` | Amount of input token (human-readable) |
| `amount_out` | Estimated output amount (human-readable, accounting for token decimals) |
| `fee_tier` | Fee tier description (e.g. "100bps (0.01%)") |
| `chain` | Chain used |

### 2. plugin-store uniswap swap

Execute an exact-input-single swap on Uniswap V3 via SwapRouter02. Handles ERC-20 approval automatically if needed.

```bash
plugin-store uniswap swap --from <token> --to <token> --amount <amount> [--chain <chain>] [--fee <bps>] [--slippage <bps>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--from` | Yes | - | Input token symbol (e.g. WETH, USDC) or contract address (0x...) |
| `--to` | Yes | - | Output token symbol (e.g. wstETH, USDC) or contract address (0x...) |
| `--amount` | Yes | - | Amount of input token in human-readable units (e.g. "0.05", "100") |
| `--chain` | No | arbitrum | Chain: arbitrum, ethereum, polygon |
| `--fee` | No | 100 | Pool fee tier in basis points: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%) |
| `--slippage` | No | 50 | Slippage tolerance in basis points (50 = 0.5%) |

**Return fields:**

| Field | Description |
|---|---|
| `action` | Always "swap" |
| `chain_id` | Numeric chain ID |
| `token_in` | Input token contract address |
| `token_out` | Output token contract address |
| `amount_in` | Amount of input token (human-readable) |
| `expected_out` | Expected output amount from quote |
| `minimum_out` | Minimum output after slippage tolerance |
| `slippage_bps` | Slippage tolerance applied (in basis points) |
| `fee_tier` | Pool fee tier (in basis points) |
| `tx_hash` | On-chain transaction hash |
| `status` | Transaction status: "success" or "failed" |
| `block_number` | Block number the transaction was included in |

### 3. plugin-store uniswap tokens

List well-known token symbols and their contract addresses for a given chain. Useful for discovering available tokens before quoting or swapping.

```bash
plugin-store uniswap tokens [--chain <chain>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--chain` | No | arbitrum | Chain: arbitrum, ethereum, polygon |

**Return fields:**

| Field | Description |
|---|---|
| `chain` | Chain name |
| `chain_id` | Numeric chain ID |
| `tokens` | Array of `{symbol, address}` objects |

**Available tokens by chain:**

| Chain | Tokens |
|---|---|
| Arbitrum (42161) | WETH, USDC, USDC.e, USDT, wstETH, weETH, WBTC, ARB |
| Ethereum (1) | WETH, USDC, USDT, wstETH, weETH, WBTC, DAI, sUSDe, USDe |
| Polygon (137) | WETH, USDC, USDT, WMATIC, wstETH |

## Key Concepts

- **Uniswap V3**: A decentralized exchange protocol using concentrated liquidity. Liquidity providers can set custom price ranges, leading to higher capital efficiency and potentially tighter spreads.
- **Fee Tiers**: Uniswap V3 pools are separated by fee tier. Different pairs have liquidity concentrated in different tiers:
  - **100 bps (0.01%)**: Best for very stable pairs (e.g. WETH/wstETH, USDC/USDT). Most correlated assets use this tier.
  - **500 bps (0.05%)**: Good for moderately stable pairs.
  - **3000 bps (0.3%)**: Standard tier for most volatile pairs (e.g. WETH/USDC).
  - **10000 bps (1%)**: For exotic or very volatile pairs.
- **SwapRouter02**: The Uniswap V3 router contract that handles token approvals, swap execution, and deadline enforcement. Uses `multicall` internally to batch operations.
- **QuoterV2**: A read-only contract that simulates a swap and returns the expected output amount and gas estimate without executing a transaction.
- **Slippage Tolerance**: The maximum acceptable difference between the quoted output and the actual output. Specified in basis points (50 bps = 0.5%). If the actual output would be less than `quoted_amount * (1 - slippage/10000)`, the transaction reverts.
- **Deadline**: Every swap has a 5-minute deadline. If the transaction is not mined within 5 minutes, it reverts automatically.
- **ERC-20 Approval**: Before swapping an ERC-20 token, the router must be approved to spend it. The CLI handles this automatically. The first swap of a given token will require an extra approval transaction.
- **Token Resolution**: You can use either a well-known symbol (e.g. WETH, USDC) or a full contract address (0x...). When using a contract address, the CLI defaults to 18 decimals.

## Edge Cases

- **Pool does not exist**: If no Uniswap V3 pool exists for the given pair and fee tier, the quote will fail with "pool may not exist for this pair/fee tier". Try a different fee tier.
- **Wrong fee tier**: The most common mistake is using the wrong fee tier. Check which fee tier has the most liquidity for the pair. For correlated assets (WETH/wstETH), use 100. For standard pairs (WETH/USDC), try 3000 first.
- **Insufficient balance**: If the user tries to swap more than their wallet balance, the transaction will fail. Check balance first via `okx-wallet-portfolio`.
- **Token approval needed**: First-time swap of any ERC-20 token requires an approval transaction. This costs additional gas but is handled automatically. The approval amount is set to exactly the swap amount (not unlimited).
- **Private key not set**: Both `quote` and `swap` commands require `EVM_PRIVATE_KEY`. Show clear error: "Set EVM_PRIVATE_KEY in your .env file".
- **Unsupported chain**: Only arbitrum, ethereum, and polygon are supported. Other chains will return an error.
- **Unknown token symbol**: If a token symbol is not in the well-known list, use the full contract address (0x...) instead. Run `tokens` to see available symbols.
- **Too many decimal places**: If the amount has more decimal places than the token supports (e.g. 7 decimals for USDC which has 6), the command will fail. Use the correct precision for the token.
- **High slippage**: If the market is volatile, increase slippage tolerance with `--slippage`. The default 50 bps (0.5%) is suitable for most swaps. For large swaps or illiquid pairs, consider 100-200 bps.
- **Transaction deadline**: Swaps have a 5-minute deadline. If the network is congested and the transaction is not mined in time, it will revert. Retry the swap.
- **Contract address with unknown decimals**: When using a raw contract address instead of a known symbol, the CLI defaults to 18 decimals. If the token has different decimals (e.g. USDC = 6), the amount will be misinterpreted. Use the well-known symbol when possible.
