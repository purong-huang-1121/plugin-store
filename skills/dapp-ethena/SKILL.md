---
name: dapp-ethena
description: >-
  This skill should be used when the user asks about Ethena, sUSDe, USDe staking,
  'sUSDe yield', 'sUSDe APY', 'stake USDe', 'unstake sUSDe', 'Ethena balance',
  'sUSDe exchange rate', 'cooldown period', or mentions Ethena protocol, sUSDe
  yield-bearing stablecoin, staking USDe for sUSDe, or checking sUSDe yield info.
  Covers sUSDe APY/exchange rate queries, USDe/sUSDe balance checks, staking,
  cooldown initiation, and unstaking. Do NOT use for DEX swaps — use okx-dex-swap
  instead. Do NOT use for Aave lending — use okx-dapp-aave instead.
license: Apache-2.0
metadata:
  author: okx
  version: "1.0.0"
  homepage: "https://web3.okx.com"
---

# Ethena sUSDe — Yield-Bearing Stablecoin

Interact with Ethena's sUSDe (staked USDe) protocol on Ethereum mainnet. sUSDe is an ERC-4626 vault token that earns yield from Ethena's delta-neutral strategy (~10% APY).

## Pre-flight Checks

Run once per session:

1. **Confirm installed**: Run `which plugin-store`. If not found, install:
   ```bash
   curl -sSL https://raw.githubusercontent.com/okx/plugin-store/main/install.sh | sh
   ```
2. **Check for updates**: Read `~/.plugin-store/last_check`, skip if updated within 12 hours.

## Authentication

- **Read-only commands** (`apy`, `balance`): No auth required.
- **Write commands** (`stake`, `cooldown`, `unstake`): Requires `EVM_PRIVATE_KEY` in `.env` (Ethereum mainnet wallet with ETH for gas).

## Commands

### View sUSDe Yield Info

```bash
plugin-store ethena apy
```

Returns: exchange rate (sUSDe → USDe), total assets, total supply, cooldown duration.

**Example output:**
```json
{
  "ok": true,
  "data": {
    "exchange_rate": "1.223045",
    "total_assets_usde": "5234567890.123456",
    "total_supply_susde": "4280000000.000000",
    "cooldown_duration_seconds": 604800
  }
}
```

- `exchange_rate`: 1 sUSDe = X USDe (grows over time as yield accrues)
- `cooldown_duration_seconds`: 604800 = 7 days

### Check Balances

```bash
plugin-store ethena balance <ADDRESS>
```

Returns USDe and sUSDe balances, plus sUSDe value in USDe terms.

**Example output:**
```json
{
  "ok": true,
  "data": {
    "usde_balance": "1000.000000",
    "susde_balance": "500.000000",
    "susde_value_in_usde": "611.522500"
  }
}
```

### Stake USDe → sUSDe

```bash
plugin-store ethena stake --amount <AMOUNT>
```

Deposits USDe into the sUSDe vault, receiving sUSDe shares. Requires ERC-20 approval (handled automatically).

- `amount`: Decimal string (e.g. "100.5")
- Gas: ~150k gas on Ethereum mainnet (~$2-5 at typical gas prices)

### Initiate Unstake Cooldown

```bash
plugin-store ethena cooldown --amount <AMOUNT>
```

Starts the 7-day cooldown period. The `amount` is in USDe terms (how much USDe you want to receive). During cooldown, the sUSDe shares are locked but still earn yield until withdrawal.

### Withdraw After Cooldown

```bash
plugin-store ethena unstake
```

Completes the withdrawal after the 7-day cooldown period has elapsed. Returns USDe to the wallet.

## Contracts

| Contract | Address | Network |
|----------|---------|---------|
| sUSDe | `0x9D39A5DE30e57443BfF2A8307A4256c8797A3497` | Ethereum |
| USDe | `0x4c9EDD5852cd905f086C759E8383e09bff1E68B3` | Ethereum |

## Key Concepts

- **ERC-4626 Vault**: sUSDe is a standard tokenized vault. Deposit USDe, receive sUSDe shares. The exchange rate increases over time as yield accrues.
- **Cooldown Period**: 7 days. Must call `cooldown` first, wait 7 days, then call `unstake`. No partial withdrawals during cooldown.
- **Yield Source**: Ethena runs a delta-neutral strategy (spot ETH + short perp) and distributes funding rate income to sUSDe holders. APY is variable (~8-15% historically).
- **sUSDe on L2**: sUSDe is bridged to Arbitrum, Base, and other L2s. You can buy sUSDe directly on L2 DEXes (cheaper gas), but staking/unstaking only works on Ethereum mainnet.

## Workflow Examples

### Check if sUSDe yield is attractive
```
User: "sUSDe 现在收益多少"
→ plugin-store ethena apy
→ "sUSDe 当前兑换率 1.2230，年化约 10%。7 天冷却期。"
```

### Stake USDe
```
User: "帮我质押 1000 USDe"
→ plugin-store ethena stake --amount 1000
→ "✅ 已质押 1000 USDe，获得 ~817.7 sUSDe"
```

### Full unstake flow
```
User: "我要取出 sUSDe"
→ plugin-store ethena cooldown --amount 500
→ "已发起冷却，500 USDe 将在 7 天后可取出"
... 7 days later ...
User: "冷却到了吗"
→ plugin-store ethena unstake
→ "✅ 已取出 500 USDe 到钱包"
```
