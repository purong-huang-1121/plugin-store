# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

This is a **Claude Code plugin** — a collection of plugin-store skills for on-chain DeFi operations. The project provides skills for DeFi lending/borrowing, perpetual trading, prediction markets, on-chain swaps, and automated trading strategies.

## Architecture

- **skills/** — 14 plugin-store CLI skill definitions (each is a `SKILL.md` with YAML frontmatter + CLI command reference)
- **cli/** — Rust CLI binary (`plugin-store`), built with `clap`; source in `cli/src/`, config in `cli/Cargo.toml`
- **Formula/** — Homebrew formula template (`plugin-store.rb`), auto-updated on release by `scripts/update-formula.sh`
- **scripts/** — Release automation scripts (e.g. `update-formula.sh` rewrites Formula SHA256 from GitHub Release checksums)
- **.github/workflows/** — CI/CD pipeline (`release.yml`: tag-triggered build for 9 platforms → GitHub Release → Homebrew update)
- **install.sh** — One-line installer for macOS / Linux (`curl | sh`)

## Available Skills

### dApp Skills (Protocol Integrations)

| Skill | CLI Subcommand | Purpose |
|-------|---------------|---------|
| dapp-aave | `aave` | Aave V3 lending: markets, account, supply, withdraw, borrow, repay |
| dapp-hyperliquid | `hyperliquid` | Perpetual/spot exchange: markets, prices, funding, trading |
| dapp-polymarket | `polymarket` | Prediction markets: search, price, orderbook, buy/sell shares |
| dapp-kalshi | `kalshi` | Regulated prediction markets (US): events, markets, trading |
| dapp-ethena | `ethena` | sUSDe staking: stake, unstake, yield info |
| dapp-morpho | `morpho` | Morpho Blue lending: markets, vaults, positions |
| dapp-uniswap | `uniswap` | Uniswap V3 on-chain swaps: quote, swap, tokens |
| dapp-composer | — | Multi-protocol orchestration and cross-dApp workflows |

### Strategy Skills (Automated Trading)

| Skill | CLI Subcommand | Purpose |
|-------|---------------|---------|
| strategy-auto-rebalance | `auto-rebalance` | USDC yield rebalancing across Aave/Compound/Morpho on Base |
| strategy-grid-trade | `grid` | ETH/USDC grid trading bot on Base |
| strategy-ranking-sniper | `ranking-sniper` | SOL trending token sniper with safety checks |
| strategy-signal-tracker | `signal-tracker` | Smart money signal follower with safety filter |
| strategy-memepump-scanner | `scanner` | Pump.fun token auto-scanner and trader |

