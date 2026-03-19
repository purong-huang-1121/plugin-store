# plugin-store

On-chain DeFi skill pack for AI agents. Integrates major protocol operations and automated trading strategies. Compatible with Claude Code and other AI coding assistants.

Plugin Store hosts Web3 strategies built on Onchain OS. After installation, browse all available strategies, pick the ones you need, and run them.

- For feedback on a strategy, reach out to the strategy author directly
- Want to share your own strategy? Feel free to get in touch

## Skills

### Automated Strategy Skills

| Skill | Subcommand | Description |
|-------|-----------|-------------|
| `strategy-auto-rebalance` | `auto-rebalance` | Auto-rebalance USDC across Aave / Compound / Morpho for best yield — Base & Ethereum |
| `strategy-grid-trade` | `grid` | ETH/USDC grid trading bot on Base |
| `strategy-ranking-sniper` | `ranking-sniper` | SOL ranking sniper — 3-layer safety filter + 6-layer exit system |
| `strategy-signal-tracker` | `signal-tracker` | Smart money / KOL / whale signal follower — 17-point safety filter |
| `strategy-memepump-scanner` | `scanner` | Pump.fun migrated token auto-scanner — 3-signal momentum detection |

### Supported dApp Protocols

| Protocol | Features |
|----------|----------|
| **Aave V3** | Markets, account info, supply, withdraw, borrow, repay |
| **Morpho Blue** | Markets, MetaMorpho vaults, deposit/withdraw, positions |
| **Uniswap V3** | On-chain token swap, quote, token search |
| **Ethena** | sUSDe stake/unstake, APY query, balance |

## Installation

### Prerequisites

1. Install Agentic Wallet — see [Agentic Wallet setup guide](https://web3.okx.com/zh-hans/onchainos/dev-docs/home/install-your-agentic-wallet)

2. Install Plugin Store:
```bash
npx skills add okx/plugin-store
```

## Configuration

### Wallet Authorization (required for SOL/EVM strategies)

```bash
onchainos wallet login
```

### Telegram Notifications (optional)

```env
TELEGRAM_BOT_TOKEN="your-bot-token"
TELEGRAM_CHAT_ID="your-chat-id"
```

> **Q: How do I set up a Telegram Bot?**
> A: Configure `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` in your environment. Just ask the Agent how to get them.

## Supported Chains

Solana, Ethereum, Base, BSC, Arbitrum, Polygon, XLayer, and 20+ other chains.

## Usage Examples

**Query Aave lending rates**
> "What's the USDC supply rate on Aave?"

**Start the ranking sniper strategy**
> "Start the SOL ranking sniper for me"

**Check strategy wallet balance**
> `plugin-store ranking-sniper balance`

**Grid trading**
> "Set up an ETH/USDC grid strategy on Base"

**Smart money signal tracking**
> "Start the smart money signal tracker"

## Disclaimer

- Built-in public API keys are for evaluation only and may be rate-limited or unavailable at any time. No liability for any losses arising from this.
- Automated trading strategies involve real assets. Understand the risks before use.
- In production, always use your own API keys and wallet credentials, and keep them secure.

## License

Apache-2.0
