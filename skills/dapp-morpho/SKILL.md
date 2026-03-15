---
name: dapp-morpho
description: "This skill should be used when the user asks about Morpho, Morpho Blue, MetaMorpho vaults, 'Morpho lending rates', 'Morpho markets', 'Morpho vaults', 'deposit into Morpho', 'withdraw from Morpho', 'Morpho positions', or mentions Morpho protocol, Morpho Blue markets, MetaMorpho, ERC-4626 vaults, or checking Morpho supply/borrow APYs. Covers market listing, market details, vault listing, vault details, and user positions across Ethereum, Base, Arbitrum, Optimism, and Polygon. Do NOT use for Aave lending — use dapp-aave instead. Do NOT use for DEX swaps — use okx-dex-swap instead."
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Morpho Protocol CLI

5 commands for browsing Morpho Blue lending markets, MetaMorpho vaults, and querying user positions across multiple chains.

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

- For Aave V3 lending → use `dapp-aave`
- For token search / analytics → use `okx-dex-token`
- For DEX swap → use `okx-dex-swap`
- For token prices / charts → use `okx-dex-market`
- For wallet balances → use `okx-wallet-portfolio`
- For transaction broadcasting → use `okx-onchain-gateway`
- For prediction markets → use `dapp-polymarket`

## Authentication

**All query commands (markets, market, vaults, vault, positions):** No authentication needed. The Morpho GraphQL API is fully public. These commands work immediately.

**On-chain vault operations (deposit, withdraw via MorphoVaultClient):** Require an EVM wallet private key. The vault client (`vault.rs`) supports ERC-4626 deposit/withdraw with automatic ERC-20 approval:

```bash
# Add to .env file
EVM_PRIVATE_KEY=0x...
```

The private key is used to sign deposit and withdraw transactions on-chain via the MetaMorpho vault's ERC-4626 interface.

## Quickstart

### Browse Markets

```bash
# List top Morpho Blue markets by supply (all chains)
plugin-store morpho markets

# List markets on Base, sorted by supply APY
plugin-store morpho markets --chain base --order-by SupplyApy --direction Desc

# Get details for a specific market
plugin-store morpho market 0xb323495f7e4148be5643a4ea4a8221eef163e4bccfdedc2a6f4696baacbc86cc --chain-id 1
```

### Browse Vaults

```bash
# List MetaMorpho vaults by TVL
plugin-store morpho vaults

# List vaults on Ethereum sorted by APY
plugin-store morpho vaults --chain ethereum --order-by Apy --direction Desc

# Get details for a specific vault
plugin-store morpho vault 0xBEEF01735c132Ada46AA9aA9B6290e399855f378 --chain-id 1
```

### Check Positions

```bash
# View all Morpho positions for a wallet
plugin-store morpho positions 0xYourAddress

# View positions on a specific chain
plugin-store morpho positions 0xYourAddress --chain base
```

## Command Index

| # | Command | Auth | Description |
|---|---------|------|-------------|
| 1 | `plugin-store morpho markets` | No | List Morpho Blue lending markets with supply/borrow APY and TVL |
| 2 | `plugin-store morpho market <unique_key>` | No | Get detailed data for a specific market by unique key |
| 3 | `plugin-store morpho vaults` | No | List MetaMorpho vaults with TVL, APY, and fees |
| 4 | `plugin-store morpho vault <address>` | No | Get detailed data for a specific vault by address |
| 5 | `plugin-store morpho positions <address>` | No | View supply/borrow and vault positions for a wallet |

## Cross-Skill Workflows

### Workflow A: Research Morpho Markets (most common)

> User: "What are the best lending rates on Morpho?"

```
1. morpho markets --order-by SupplyApy --direction Desc   → show top markets by supply APY
       ↓ user picks a market
2. morpho market <unique_key> --chain-id 1                 → show full details including rewards
       ↓ user wants to compare with Aave
3. aave markets --chain ethereum                           → compare Aave rates
       ↓ user decides to supply on Morpho
4. (deposit via MorphoVaultClient or direct on-chain interaction)
```

**Data handoff:**
- `uniqueKey` from markets list → `<unique_key>` for the market detail command
- `address` from vaults list → `<address>` for the vault detail command

### Workflow B: Vault Discovery and Deposit

```
1. morpho vaults --chain base --order-by NetApy --direction Desc  → find best vaults
       ↓ user picks a vault
2. morpho vault <address> --chain-id 8453                  → see vault details, description, fee
       ↓ user wants to deposit
3. Check EVM_PRIVATE_KEY is set
       ↓ not set → prompt user to add to .env
       ↓ set → deposit via vault ERC-4626 interface
```

### Workflow C: Portfolio Review

```
1. morpho positions 0xYourAddress                          → see all market and vault positions
2. morpho market <unique_key> --chain-id 1                 → check current rates on a position
3. morpho vault <vault_address> --chain-id 1               → check vault APY
```

### Workflow D: Multi-Chain Comparison

```
1. morpho markets --chain ethereum                         → Ethereum markets
2. morpho markets --chain base                             → Base markets
3. morpho markets --chain arbitrum                         → Arbitrum markets
4. morpho vaults --chain base --order-by NetApy            → find best vault on best chain
```

### Workflow E: With OKX Skills

```
1. okx-wallet-portfolio balance --chain ethereum           → check token balances
2. morpho markets --chain ethereum --order-by SupplyApy    → find best lending opportunity
3. morpho market <unique_key> --chain-id 1                 → confirm details before depositing
```

## Operation Flow

### Step 1: Identify Intent

- Browse lending markets → `markets`
- Check specific market details → `market`
- Browse yield vaults → `vaults`
- Check specific vault details → `vault`
- Check portfolio/positions → `positions`

### Step 2: Collect Parameters

- Missing chain filter → show all chains by default, or ask user which chain they prefer
- Missing unique_key for market → use `markets` first, then pick from the list
- Missing vault address → use `vaults` first, then pick from the list
- Missing wallet address for positions → ask user for their wallet address
- Missing sort preference → default to largest TVL (SupplyAssetsUsd for markets, TotalAssetsUsd for vaults)

### Step 3: Execute

- **Data phase**: show market info, rates, vault APYs, let user make informed decision
- **Comparison phase**: optionally compare with Aave or other protocols via their respective skills
- **Action phase**: for deposits/withdrawals, use the vault client with EVM_PRIVATE_KEY

### Step 4: Suggest Next Steps

| Just completed | Suggest |
|---|---|
| `markets` | 1. Check specific market details → `market` 2. Compare with vaults → `vaults` |
| `market` | 1. Check your positions → `positions` 2. Compare with Aave → `aave markets` |
| `vaults` | 1. Check specific vault details → `vault` 2. Check your positions → `positions` |
| `vault` | 1. Check your positions → `positions` 2. View wallet balance → `okx-wallet-portfolio` |
| `positions` | 1. Check current rates for a market → `market` 2. Check vault APY → `vault` |

Present conversationally — never expose skill names or endpoint paths to the user.

## CLI Command Reference

### 1. plugin-store morpho markets

List Morpho Blue lending markets with supply/borrow data.

```bash
plugin-store morpho markets [--chain <chain>] [--limit <n>] [--order-by <field>] [--direction <dir>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--chain` | No | all chains | Filter by chain: ethereum, base, arbitrum, optimism, polygon, or numeric chain ID |
| `--limit` | No | 20 | Max results (1-1000) |
| `--order-by` | No | SupplyAssetsUsd | Sort field: SupplyAssetsUsd, BorrowAssetsUsd, Utilization, SupplyApy, BorrowApy |
| `--direction` | No | Desc | Sort direction: Desc or Asc |

**Key return fields per market:**

| Field | Description |
|---|---|
| `uniqueKey` | Market unique identifier (32-byte hex) — use for `market` command |
| `lltv` | Liquidation loan-to-value ratio |
| `oracleAddress` | Oracle contract address |
| `irmAddress` | Interest rate model contract address |
| `loanAsset` | Loan token info: `{address, symbol, name, decimals}` |
| `collateralAsset` | Collateral token info: `{address, symbol, name, decimals}` |
| `state.supplyAssets` | Total supply in token units |
| `state.supplyAssetsUsd` | Total supply in USD |
| `state.borrowAssets` | Total borrow in token units |
| `state.borrowAssetsUsd` | Total borrow in USD |
| `state.utilization` | Utilization ratio (0-1) |
| `state.supplyApy` | Current supply APY |
| `state.borrowApy` | Current borrow APY |
| `state.avgSupplyApy` | Average supply APY (time-weighted) |
| `state.avgBorrowApy` | Average borrow APY (time-weighted) |
| `morphoBlue.chain` | Chain info: `{id, network}` |

### 2. plugin-store morpho market

Get detailed data for a specific Morpho Blue market.

```bash
plugin-store morpho market <unique_key> [--chain-id <id>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<unique_key>` | Yes | - | Market unique key (32-byte hex string, e.g. 0xb323...) |
| `--chain-id` | No | 1 | Chain ID (1 = Ethereum, 8453 = Base, 42161 = Arbitrum, 10 = Optimism, 137 = Polygon) |

**Return fields (superset of markets):**

| Field | Description |
|---|---|
| `uniqueKey` | Market unique identifier |
| `lltv` | Liquidation LTV |
| `oracleAddress` | Oracle contract address |
| `irmAddress` | Interest rate model contract address |
| `loanAsset` | Loan token: `{address, symbol, name, decimals}` |
| `collateralAsset` | Collateral token: `{address, symbol, name, decimals}` |
| `state.supplyAssets` | Total supply in token units |
| `state.supplyAssetsUsd` | Total supply in USD |
| `state.borrowAssets` | Total borrow in token units |
| `state.borrowAssetsUsd` | Total borrow in USD |
| `state.collateralAssets` | Total collateral in token units |
| `state.collateralAssetsUsd` | Total collateral in USD |
| `state.utilization` | Utilization ratio |
| `state.supplyApy` | Current supply APY |
| `state.borrowApy` | Current borrow APY |
| `state.avgSupplyApy` | Average supply APY |
| `state.avgBorrowApy` | Average borrow APY |
| `state.rewards` | Reward incentives: `[{asset: {symbol}, supplyApr, borrowApr}]` |
| `morphoBlue.chain` | Chain info: `{id, network}` |

### 3. plugin-store morpho vaults

List MetaMorpho vaults with TVL and yield data.

```bash
plugin-store morpho vaults [--chain <chain>] [--limit <n>] [--order-by <field>] [--direction <dir>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `--chain` | No | all chains | Filter by chain: ethereum, base, arbitrum, optimism, polygon, or numeric chain ID |
| `--limit` | No | 20 | Max results (1-1000) |
| `--order-by` | No | TotalAssetsUsd | Sort field: TotalAssetsUsd, TotalAssets, Apy, NetApy, Name |
| `--direction` | No | Desc | Sort direction: Desc or Asc |

**Key return fields per vault:**

| Field | Description |
|---|---|
| `address` | Vault contract address — use for `vault` command |
| `name` | Vault display name |
| `symbol` | Vault token symbol |
| `asset` | Underlying asset: `{address, symbol, name, decimals}` |
| `state.totalAssetsUsd` | Total value locked in USD |
| `state.totalAssets` | Total assets in token units |
| `state.apy` | Gross APY (before fees) |
| `state.netApy` | Net APY (after performance fee) |
| `state.fee` | Performance fee ratio |
| `chain` | Chain info: `{id, network}` |

### 4. plugin-store morpho vault

Get detailed data for a specific MetaMorpho vault.

```bash
plugin-store morpho vault <address> [--chain-id <id>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<address>` | Yes | - | Vault contract address (0x...) |
| `--chain-id` | No | 1 | Chain ID (1 = Ethereum, 8453 = Base, 42161 = Arbitrum, 10 = Optimism, 137 = Polygon) |

**Return fields (superset of vaults):**

| Field | Description |
|---|---|
| `address` | Vault contract address |
| `name` | Vault display name |
| `symbol` | Vault token symbol |
| `asset` | Underlying asset: `{address, symbol, name, decimals}` |
| `state.totalAssetsUsd` | Total value locked in USD |
| `state.totalAssets` | Total assets in token units |
| `state.apy` | Gross APY |
| `state.netApy` | Net APY (after fee) |
| `state.fee` | Performance fee ratio |
| `chain` | Chain info: `{id, network}` |
| `metadata.description` | Vault description text |
| `metadata.forumLink` | Link to governance/forum discussion |

### 5. plugin-store morpho positions

Get supply/borrow and vault positions for a wallet address.

```bash
plugin-store morpho positions <address> [--chain <chain>]
```

| Param | Required | Default | Description |
|---|---|---|---|
| `<address>` | Yes | - | Wallet address (0x...) |
| `--chain` | No | all chains | Filter by chain: ethereum, base, arbitrum, optimism, polygon, or numeric chain ID |

**Return fields:**

| Field | Description |
|---|---|
| `address` | Queried wallet address |
| `marketPositions` | Array of Morpho Blue market positions |
| `marketPositions[].market.uniqueKey` | Market identifier |
| `marketPositions[].market.loanAsset.symbol` | Loan token symbol |
| `marketPositions[].market.collateralAsset.symbol` | Collateral token symbol |
| `marketPositions[].market.morphoBlue.chain.network` | Chain network name |
| `marketPositions[].state.collateral` | Collateral amount in token units |
| `marketPositions[].state.collateralUsd` | Collateral value in USD |
| `marketPositions[].state.borrowAssets` | Borrow amount in token units |
| `marketPositions[].state.borrowAssetsUsd` | Borrow value in USD |
| `marketPositions[].state.supplyAssets` | Supply amount in token units |
| `marketPositions[].state.supplyAssetsUsd` | Supply value in USD |
| `vaultPositions` | Array of MetaMorpho vault positions |
| `vaultPositions[].vault.address` | Vault contract address |
| `vaultPositions[].vault.name` | Vault name |
| `vaultPositions[].vault.symbol` | Vault token symbol |
| `vaultPositions[].vault.chain.network` | Chain network name |
| `vaultPositions[].state.assets` | Deposit amount in underlying token units |
| `vaultPositions[].state.assetsUsd` | Deposit value in USD |

## Key Concepts

- **Morpho Blue**: A permissionless, minimal lending protocol. Unlike Aave, Morpho Blue allows anyone to create isolated lending markets with custom parameters (collateral asset, loan asset, oracle, IRM, LLTV). Each market is independent with no shared risk between them.
- **MetaMorpho Vaults**: ERC-4626 compliant vaults that aggregate deposits across multiple Morpho Blue markets. Vault curators manage the allocation strategy, optimizing yield while managing risk. Users deposit a single asset and the vault distributes it across markets.
- **LLTV (Liquidation Loan-to-Value)**: The maximum ratio of borrow value to collateral value. When a position's LTV exceeds the LLTV, it becomes liquidatable. Each market has a fixed, immutable LLTV set at creation.
- **IRM (Interest Rate Model)**: Each market uses an interest rate model contract that determines supply and borrow rates based on utilization. Morpho Blue supports pluggable IRM contracts.
- **Oracle**: Each market specifies an oracle for pricing the collateral relative to the loan asset. The oracle is immutable once the market is created.
- **Utilization**: The ratio of total borrow to total supply in a market. Higher utilization means higher rates for both suppliers and borrowers.
- **Net APY vs Gross APY**: For vaults, gross APY is the raw yield from underlying markets. Net APY subtracts the vault's performance fee. Always compare using Net APY.
- **Rewards**: Some markets offer additional token rewards (e.g. MORPHO tokens) on top of the base supply/borrow APY. Check the `rewards` field in market details.
- **ERC-4626**: The standard interface for tokenized vaults. MetaMorpho vaults implement this standard, allowing deposit, withdraw, and redeem operations. Shares represent proportional ownership of vault assets.
- **Unique Key**: A 32-byte hex identifier that uniquely identifies a Morpho Blue market. It is derived from the market's parameters (loan asset, collateral asset, oracle, IRM, LLTV).

## Edge Cases

- **Unknown chain name**: If the user specifies an unrecognized chain, the command returns an error listing supported chains: ethereum, base, arbitrum, optimism, polygon. Numeric chain IDs are also accepted.
- **Market not found**: If a `unique_key` does not match any market on the given chain, the API returns null. Suggest using `markets` to browse available markets.
- **Vault not found**: If a vault `address` does not exist on the given chain, the API returns null. Suggest using `vaults` to browse available vaults.
- **No positions**: If a wallet has no Morpho positions, the positions command returns empty arrays for both `marketPositions` and `vaultPositions`. This is normal for wallets that have not interacted with Morpho.
- **Chain ID mismatch**: The `--chain-id` parameter for `market` and `vault` commands expects a numeric chain ID (not a chain name). Use 1 for Ethereum, 8453 for Base, 42161 for Arbitrum, 10 for Optimism, 137 for Polygon.
- **Rate limiting**: The Morpho GraphQL API allows 5,000 requests per 5 minutes. If rate limited, retry after a moment.
- **Large result sets**: Use `--limit` to control result size. Maximum is 1000. Default is 20 which is suitable for most browsing use cases.
- **Sorting fields are case-sensitive**: Use exact field names: SupplyAssetsUsd, BorrowAssetsUsd, Utilization, SupplyApy, BorrowApy for markets; TotalAssetsUsd, TotalAssets, Apy, NetApy, Name for vaults. Direction must be Desc or Asc.
- **EVM_PRIVATE_KEY for vault operations**: On-chain deposit/withdraw through the MorphoVaultClient requires `EVM_PRIVATE_KEY`. The GraphQL query commands do not need any key.
- **Token approval**: First-time deposit into a vault requires an ERC-20 approval transaction. The vault client handles this automatically but it uses additional gas.
