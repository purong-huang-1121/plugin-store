---
name: dapp-composer
description: >-
  This skill should be used when the user asks about on-chain earning opportunities,
  'how to make money on-chain', 'any profitable strategies', 'arbitrage opportunities',
  '链上有什么赚钱机会', '有什么盈利机会', '有什么套利机会', '怎么赚钱', '有什么好的策略',
  '帮我理财', '有什么收益机会', 'yield opportunities', 'how to earn', 'investment strategy',
  'DeFi 策略推荐', '有什么自动化策略', 'automated strategies', 'passive income on-chain',
  or mentions looking for on-chain profit, yield, or automated trading opportunities.
  This is a strategy discovery and routing skill — it presents available built-in strategies
  and routes users to the correct skill. Do NOT use for single-protocol operations
  (use plugin-store aave, plugin-store hyperliquid, etc. directly).
license: Apache-2.0
metadata:
  author: okx
  version: "3.0.0"
  homepage: "https://web3.okx.com"
---

# On-Chain Strategy Composer

Helps users discover and launch built-in automated strategies. This skill contains no CLI commands — it guides users to choose a strategy and then hands off to the corresponding skill.

## Pre-flight Checks

Same as sub-skills. Run once per session:

1. **Confirm installed**: Run `which plugin-store`. If not found, install:
   ```bash
   curl -sSL https://raw.githubusercontent.com/okx/plugin-store/main/install.sh | sh
   ```
2. **Check for updates**: Read `~/.plugin-store/last_check`, skip if updated within 12 hours.

## Skill Routing

### dApp 平台（单一协议操作）
- User asks about **Aave** (存款、借贷、利率) → use `plugin-store aave`
- User asks about **Morpho** (vault、借贷) → use `plugin-store morpho`
- User asks about **Uniswap** (换币、swap、报价) → use `plugin-store uniswap`
- User asks about **Hyperliquid** (永续合约、funding rate、现货) → use `plugin-store hyperliquid`
- User asks about **Ethena** (sUSDe、质押、收益) → use `plugin-store ethena`
- User asks about **Polymarket** (预测市场、下注) → use `plugin-store polymarket`
- User asks about **Kalshi** (合规预测市场) → use `plugin-store kalshi`

### 自动化策略
- User asks about **grid trading specifically** → use `plugin-store grid`
- User asks about **USDC yield / auto-rebalance specifically** → use `plugin-store auto-rebalance`

### 策略发现（本 skill）
- User asks **"有什么赚钱/盈利/套利机会"** or general strategy discovery → **use this skill**

---

## Entry Point: Strategy Discovery

### Trigger

User says: "链上有什么赚钱机会", "有什么盈利机会", "有什么套利机会", "有什么好的策略", "how to earn on-chain", "any profitable strategies", "帮我理财", "yield opportunities", "automated strategies"

### Step 1: Present Built-in Strategies and Supported Platforms

Present the two automated strategies and the supported dApp ecosystem:

```
目前内置了 6 个策略（3 个 EVM + 3 个 Solana）：

┌─────────────────────────────────────────────────────────────────────┐
│  A. USDC 智能调仓 (Auto-Rebalance)                                 │
│                                                                     │
│  自动在 Aave V3、Compound V3、Morpho 三个协议之间寻找最优 USDC      │
│  收益率，检测到利差超过阈值时自动调仓。                              │
│                                                                     │
│  ● 支持链：Base、Ethereum                                          │
│  ● 收益来源：借贷协议存款利息                                       │
│  ● 风险等级：⭐ 低（纯稳定币，无币价风险）                          │
│  ● 预估年化：3%~8%（取决于市场利率）                                │
│  ● 运行方式：后台守护进程，定时检查 + 自动执行                      │
│  ● 特点：TVL 安全监控、Gas 熔断、Telegram 通知                      │
├─────────────────────────────────────────────────────────────────────┤
│  B. ETH/USDC 网格交易 (Grid Trading)                                │
│                                                                     │
│  基于 EMA 动态网格，在价格波动中自动低买高卖，赚取网格利润。         │
│  通过 OKX DEX 聚合器执行链上 swap。                                  │
│                                                                     │
│  ● 支持链：Base                                                     │
│  ● 交易对：ETH/USDC                                                │
│  ● 风险等级：⭐⭐ 中低（持有 ETH 有币价风险，网格对冲部分波动）      │
│  ● 预估年化：10%~30%（取决于市场波动率，震荡行情最佳）              │
│  ● 运行方式：后台守护进程，默认每 60 秒执行一次（可通过               │
│    plugin-store grid set --key tick_interval_secs --value N 调整）      │
│  ● 特点：自适应波动率、风控熔断、仓位限制、失败重试                  │
├─────────────────────────────────────────────────────────────────────┤
│  C. 稳定币杠杆循环 (Aave Leverage Loop)                              │
│                                                                     │
│  在 Aave V3 上循环执行 USDC 存款→借款→再存款，赚取存借利差。         │
│  全程 USDC，无币价风险，利差通过杠杆放大约 2.4 倍。                  │
│                                                                     │
│  ● 支持链：Ethereum、Polygon、Arbitrum、Base                        │
│  ● 收益来源：Aave 存款利率 - 借款利率 × 杠杆倍数                    │
│  ● 风险等级：⭐ 低（纯稳定币，需关注利差反转和健康因子）             │
│  ● 预估年化：5%~15%（取决于存借利差和循环轮数）                      │
│  ● 运行方式：AI 引导逐步执行（非自动守护进程）                       │
│  ● 特点：健康因子监控、利差反转告警、一键去杠杆退出                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ═══════════════ Solana Meme 策略（依赖线上 plugin-store）══════════════ │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  D. SOL 涨幅榜狙击 (Ranking Sniper)                                  │
│                                                                     │
│  实时监控 Solana 涨幅榜 Top 20，新币上榜自动买入、跌出自动卖出。     │
│  不预判哪个币能涨，而是吃上榜后的那一段动量。                        │
│                                                                     │
│  ● 支持链：Solana                                                   │
│  ● 收益来源：涨幅榜动量跟踪                                         │
│  ● 风险等级：⭐⭐⭐ 高（Meme 币高波动）                              │
│  ● 运行方式：后台守护进程，每 10 秒轮询                              │
│  ● 风控：25 项链上安全检查 + Momentum Score 评分 + 6 层退出机制       │
│  ● 特点：排名退出 > 硬止损 > 快速止损 > 追踪止损 > 时间止损 > 梯度止盈│
│  ● 依赖：plugin-store (token-ranking, token-advanced-info, holder,      │
│          current-price, quote, swap)                                 │
├─────────────────────────────────────────────────────────────────────┤
│  E. SOL 聪明钱跟单 (Signal Tracker)                                  │
│                                                                     │
│  实时监控链上聪明钱动向，多个高质量钱包同时买入同一代币时自动跟单。   │
│  SmartMoney / KOL / Whale 三类信号，跟着最聪明的钱走。               │
│                                                                     │
│  ● 支持链：Solana                                                   │
│  ● 收益来源：聪明钱信号跟单                                         │
│  ● 风险等级：⭐⭐⭐ 高（Meme 币高波动）                              │
│  ● 运行方式：后台守护进程，每 20 秒轮询                              │
│  ● 风控：MC/流动性过滤 + Dev 零容忍检查 + Bundler 操控检测            │
│         + K线追高检测 + Session 风控（连亏暂停）                     │
│  ● 特点：同车钱包数分级仓位 + 成本感知止盈 + 时间衰减止损            │
│  ● 依赖：plugin-store (signal-list, price-info, token-search, candles,  │
│          tokenDevInfo, tokenBundleInfo, balances, quote, swap)       │
├─────────────────────────────────────────────────────────────────────┤
│  F. SOL Memepump 扫描 (Memepump Scanner)                             │
│                                                                     │
│  实时扫描 Pump.fun 迁移代币，TX加速 + 成交量突增 + 买压主导          │
│  三信号共振时自动买入——捕捉安全验证后的动量爆发瞬间。                │
│                                                                     │
│  ● 支持链：Solana                                                   │
│  ● 收益来源：Pump.fun 迁移后动量爆发                                │
│  ● 风险等级：⭐⭐⭐ 高（Meme 币高波动）                              │
│  ● 运行方式：后台守护进程，每 10 秒轮询                              │
│  ● 风控：服务端安全过滤 + Dev/Bundler 深度验证 + 三重信号检测        │
│  ● 特点：SCALP/MINIMUM 分档仓位 + Hot Mode 自适应 + 30min 最大持仓  │
│  ● 依赖：plugin-store (memepump-tokenList, tokenDevInfo,               │
│          tokenBundleInfo, candles, trades, price-info, quote, swap)  │
└─────────────────────────────────────────────────────────────────────┘

请选择：输入 A ~ F

此外，我们还支持以下 dApp 平台，可以直接交互：

┌─────────────────────────────────────────────────────────────────────┐
│  支持的 dApp 平台                                                   │
├──────────────┬──────────────────────┬───────────────────────────────┤
│  平台         │  类型                │  支持链                       │
├──────────────┼──────────────────────┼───────────────────────────────┤
│  Aave V3     │  借贷协议            │  Ethereum, Polygon,           │
│              │                      │  Arbitrum, Base               │
│  Compound V3 │  借贷协议            │  Base, Ethereum               │
│  Morpho      │  借贷协议 (Vault)    │  Base, Ethereum               │
│  Uniswap V3  │  DEX 链上交易         │  Arbitrum, Ethereum, Polygon  │
│  Hyperliquid │  永续合约 + 现货交易  │  Hyperliquid L1               │
│  Ethena      │  sUSDe 质押收益       │  Ethereum                     │
│  Polymarket  │  预测市场             │  Polygon                      │
│  Kalshi      │  合规预测市场 (美国)  │  -（中心化）                  │
└──────────────┴──────────────────────┴───────────────────────────────┘

如果你想直接使用某个平台（如 "帮我在 Aave 存 USDC"、"Uniswap 换币"），
我会直接跳转到对应的 skill。
```

### Step 2: User Selects Strategy or Platform

| User says | Action |
|-----------|--------|
| "A", "调仓", "auto-rebalance", "USDC 收益" | → Go to **Flow A** |
| "B", "网格", "grid", "grid trading" | → Go to **Flow B** |
| "C", "杠杆循环", "leverage loop", "套利" | → Go to **Flow C** |
| "D", "涨幅榜", "ranking", "榜单狙击" | → Go to **Flow D** |
| "E", "聪明钱", "signal", "跟单", "smart money" | → Go to **Flow E** |
| "F", "memepump", "pump.fun", "meme 扫描" | → Go to **Flow F** |
| "都要", "both", "两个都跑" | → Explain that multiple strategies can run concurrently, guide one by one |
| "Aave", "存款", "借贷" | → Route to `plugin-store aave` commands |
| "Uniswap", "换币", "swap" | → Route to `plugin-store uniswap` commands |
| "Hyperliquid", "永续", "合约" | → Route to `plugin-store hyperliquid` commands |
| "Ethena", "sUSDe", "质押" | → Route to `plugin-store ethena` commands |
| "Polymarket", "预测市场" | → Route to `plugin-store polymarket` commands |
| Mentions a specific dApp platform | → Route to the corresponding `plugin-store <dapp>` commands |

---

## Flow A: USDC 智能调仓

### Step A1: Ask for chain

```
USDC 智能调仓支持以下链：

| 链 | Gas 成本 | 说明 |
|----|----------|------|
| Base | 极低 (~$0.01) | 推荐小资金用户 |
| Ethereum | 较高 (~$2-10) | 大资金用户，协议 TVL 更高 |

你想在哪条链上运行？(base / ethereum)
```

### Step A2: Ask for balance / check wallet

After user selects chain:

```
好的，在 {chain} 上运行 USDC 智能调仓。

请问你在 {chain} 上有多少 USDC 可以投入？
（或者我可以帮你查一下钱包余额）
```

If user provides wallet address or says "帮我查" → use `plugin-store portfolio` to check balance.

### Step A3: Confirm and launch

```
确认启动参数：

| 参数 | 值 |
|------|---|
| 策略 | USDC 智能调仓 |
| 链 | {chain} |
| 可用资金 | {amount} USDC |
| 检查频率 | 每 60 分钟 |
| 最小利差 | 0.5% |
| 协议覆盖 | Aave V3 + Compound V3 + Morpho |

确认启动？(Y/n)
```

After confirmation, execute:

```bash
plugin-store auto-rebalance start --chain {chain} --interval 60 --min-spread 0.5
```

### Step A4: Post-launch guidance

```
智能调仓守护进程已启动！

后续操作：
• 查看状态：plugin-store auto-rebalance status
• 停止运行：plugin-store auto-rebalance stop
• 设置 Telegram 通知（推荐）：
  export TELEGRAM_BOT_TOKEN=<TOKEN>
  export TELEGRAM_CHAT_ID=<CHAT_ID>
  plugin-store auto-rebalance start --chain {chain}
```

---

## Flow B: ETH/USDC 网格交易

### Step B1: Confirm chain

```
ETH/USDC 网格交易目前仅支持 Base 链。

需要准备：
• Base 链上的 ETH（用于交易 + Gas）
• Base 链上的 USDC（用于交易）
• 建议 ETH:USDC 比例约 50:50

你在 Base 上有多少资金可以投入？
（或者我可以帮你查一下钱包余额）
```

If user provides wallet address or says "帮我查" → use `plugin-store portfolio` to check balance.

### Step B2: Market analysis

Before launching, run market analysis:

```bash
plugin-store grid analyze
```

Present results to user:

```
当前市场状况：

| 指标 | 值 |
|------|---|
| ETH 价格 | ${price} |
| EMA-20 | ${ema} |
| 波动率 | {vol}% |
| 趋势 | {trend} |

{market_comment}
```

Market comment logic:
- Volatility > 3%: "波动率较高，网格策略表现良好的环境"
- Volatility < 1%: "波动率偏低，网格收益可能有限"
- Strong trend: "单边趋势中，注意仓位限制会自动保护"

### Step B3: Confirm and launch

```
确认启动参数：

| 参数 | 值 |
|------|---|
| 策略 | ETH/USDC 网格交易 |
| 链 | Base |
| 可用资金 | ~${total_usd} (ETH + USDC) |
| 网格级数 | 6 |
| 执行频率 | 每 60 秒（可通过 plugin-store grid set --key tick_interval_secs 调整） |
| 单笔上限 | 12% 总仓位 |
| 仓位保护 | ETH 占比 35%~65% |

确认启动？(Y/n)
```

After confirmation, execute:

```bash
plugin-store grid start
```

### Step B4: Post-launch guidance

```
网格交易 Bot 已启动！

后续操作：
• 查看状态：plugin-store grid status
• 查看收益：plugin-store grid report
• 交易记录：plugin-store grid history
• 停止运行：plugin-store grid stop
• 市场分析：plugin-store grid analyze
• 调整参数：plugin-store grid set --key <name> --value <value>
• 查看配置：plugin-store grid config
```

---

## Flow C: 稳定币杠杆循环 (Aave Leverage Loop)

### 原理

在 Aave V3 上执行 USDC 存款 → 借 USDC → 再存款 → 再借 → 再存款的循环，赚取存款利率和借款利率之间的利差。全程 USDC，无币价风险。

```
本金 1000 USDC，LTV 80%，循环 3 轮：

第 1 轮：存入 1000 → 借出 800
第 2 轮：存入 800  → 借出 640
第 3 轮：存入 640  → 借出 512（保留不再循环）

总存款 = 1000 + 800 + 640 = 2440 USDC
总借款 = 800 + 640 + 512 = 1952 USDC (如最后一轮也循环)
       = 800 + 640       = 1440 USDC (如最后一轮不借)
有效杠杆 ≈ 2.44x

净年化 = (总存款 × 存款利率 - 总借款 × 借款利率) / 本金
举例：存款 4%, 借款 3% → (2440×4% - 1440×3%) / 1000 = 5.36%
```

### Step C1: Ask for chain

```
稳定币杠杆循环支持以下链：

| 链 | Gas 成本 | 说明 |
|----|----------|------|
| Ethereum | ~$2-10/tx | TVL 最高，利率最稳定 |
| Arbitrum | ~$0.1-0.5/tx | 推荐，Gas 低且流动性好 |
| Polygon | ~$0.01/tx | Gas 极低 |
| Base | ~$0.01/tx | Gas 极低 |

推荐 Arbitrum（Gas 低 + 流动性好）。你想在哪条链上执行？
```

### Step C2: Check profitability

After user selects chain, check real-time利差:

```bash
plugin-store aave reserve USDC --chain {chain}
```

Extract `supplyAPY` and `borrowAPY`, then validate:

```
当前 Aave {chain} USDC 利率：

| 指标 | 值 |
|------|---|
| 存款利率 (Supply APY) | {supply_apy}% |
| 借款利率 (Borrow APY) | {borrow_apy}% |
| 利差 | {spread}% |
| 循环 3 轮后预估净年化 | {net_apy}% |
```

**Profitability check:**
- If `supply_apy <= borrow_apy`: ABORT — "利差为负（存款 {supply}% < 借款 {borrow}%），当前不适合执行此策略。建议改用策略 A（智能调仓）或等待利率回归。"
- If spread < 0.5%: WARN — "利差仅 {spread}%，杠杆后年化约 {net}%，收益偏低。是否继续？"
- If spread >= 0.5%: PROCEED — "利差 {spread}%，杠杆放大后预估年化 {net}%，可以执行。"

### Step C3: Ask for amount and confirm

```
请问投入多少 USDC？（需要你在 {chain} 上已有 USDC）
```

After user provides amount:

```
确认执行参数：

| 参数 | 值 |
|------|---|
| 策略 | 稳定币杠杆循环 |
| 链 | {chain} |
| 本金 | {amount} USDC |
| LTV | 80% |
| 循环轮数 | 3（健康因子 > 1.20 时继续） |
| 预估总存款 | ~{amount × 2.44} USDC |
| 预估总借款 | ~{amount × 1.44} USDC |
| 预估净年化 | {net_apy}% |
| 预估月收益 | ~${monthly} |

需要 EVM_PRIVATE_KEY 签署链上交易
确认执行？(Y/n)
```

### Step C4: Execute leverage loops

After user confirms, execute `plugin-store aave` commands in sequence:

```
Step 1: 验证利差
──────────────
  plugin-store aave reserve USDC --chain {chain}
  → 确认 supply > borrow，否则中止

Step 2: 首次存入
──────────────
  plugin-store aave supply --asset USDC --amount {principal} --chain {chain}
  → 确认 tx 成功
  → total_supplied = principal

Step 3: 循环（最多 3 轮）
──────────────────────────
  每一轮：

    a) 检查健康因子：
       plugin-store aave account {address} --chain {chain}
       → 如果 health_factor < 1.30，停止循环，报告当前状态

    b) 借出 USDC：
       borrow_amount = 上一轮存入金额 × 0.80
       plugin-store aave borrow --asset USDC --amount {borrow_amount} --chain {chain}
       → total_borrowed += borrow_amount

    c) 再存入：
       plugin-store aave supply --asset USDC --amount {borrow_amount} --chain {chain}
       → total_supplied += borrow_amount

Step 4: 报告最终状态
────────────────────
  plugin-store aave account {address} --chain {chain}
```

Present final result:

```
稳定币杠杆循环完成！

| 指标 | 值 |
|------|---|
| 本金 | {principal} USDC |
| 总存款 | {total_supplied} USDC |
| 总借款 | {total_borrowed} USDC |
| 有效杠杆 | {total_supplied / principal}x |
| 健康因子 | {health_factor} |
| 存款利率 | {supply_apy}% |
| 借款利率 | {borrow_apy}% |
| 预估净年化 | {net_apy}% |
| 预估月收益 | ~${monthly} |

后续操作：
• 查看仓位：plugin-store aave account {address} --chain {chain}
• 查看利率变化：plugin-store aave reserve USDC --chain {chain}
• 退出策略（去杠杆）：告诉我 "退出策略C" 或 "去杠杆"
```

### Exit Flow (去杠杆)

When user says "退出策略C", "去杠杆", "close leverage loop":

```
Step 1: 查看当前仓位
  plugin-store aave account {address} --chain {chain}

Step 2: 反向循环（逐轮退出）
  每一轮：
    a) plugin-store aave withdraw --asset USDC --amount {该轮借出金额} --chain {chain}
    b) plugin-store aave repay --asset USDC --amount {该轮借出金额} --chain {chain}

Step 3: 最终提取全部
  plugin-store aave withdraw --asset USDC --amount max --chain {chain}

Step 4: 报告
  "已完全退出杠杆循环，取回 {final_amount} USDC"
```

### Monitoring (策略监控)

When user asks "策略C状态", "杠杆循环状态", "check my loop":

```bash
plugin-store aave account {address} --chain {chain}
plugin-store aave reserve USDC --chain {chain}
```

Present:
```
| 指标 | 当前值 |
|------|--------|
| 总存款 (USD) | ${total_collateral} |
| 总借款 (USD) | ${total_debt} |
| 健康因子 | {health_factor} |
| 存款利率 | {supply_apy}% |
| 借款利率 | {borrow_apy}% |
| 利差 | {spread}% |
| 预估月净收益 | ~${monthly} |
```

Alerts:
- `health_factor < 1.30` → "健康因子偏低 ({hf})，建议去杠杆一轮"
- `health_factor < 1.10` → "清算风险！立即去杠杆"
- `borrow_apy > supply_apy` → "利差转负，建议退出策略C"

---

## Flow D: SOL 涨幅榜狙击 (Ranking Sniper)

### 原理

实时监控 OKX Solana 涨幅排行榜 Top 20，当新代币首次上榜时，经过三级风控过滤 + Momentum Score 评分后自动买入，通过 6 层退出系统管理仓位。不预判哪个币能涨，而是吃上榜后的那一段动量。

### 策略细节

1. **监控**: 每 10 秒轮询 OKX Solana 涨幅榜 Top 20
2. **风控过滤** (25 项):
   - Slot Guard: 蜜罐检测、Top10 集中度 ≤80%、Dev 持仓 ≤50%
   - Advanced Safety: Bundler ≤30%、狙击手 ≤30%、Dev Rug 历史 ≤20
   - Holder Risk: 13 项基础过滤 + 3 项可疑地址扫描
3. **评分**: Smart Money 标签 +8 分、持仓分散度、低狙击手等信号加分，0-125 分达标后买入
4. **退出机制** (6 层优先级):
   - 排名退出（最高优先级）> 硬止损（-25%）> 快速止损（5min/-8%）
   - 追踪止损（+8%激活/12%回撤）> 时间止损（6h）> 梯度止盈（+5%/+15%/+30% 分三批）
5. **安全网**: 停止引擎自动清仓所有持仓，日亏损上限 15% 自动停机

### 依赖的 plugin-store 命令

| CLI 命令 | 用途 |
|----------|------|
| `plugin-store ranking-sniper tick` | 执行单次轮询 |
| `plugin-store ranking-sniper start` | 启动守护进程 |
| `plugin-store ranking-sniper stop` | 停止运行 |
| `plugin-store ranking-sniper status` | 查看状态 |
| `plugin-store ranking-sniper report` | 详细 PnL 报告 |

### Step D1: Confirm and configure

```
SOL 涨幅榜狙击 运行在 Solana 链上。

需要准备：
• SOL 钱包私钥（用于签署链上交易）
• 钱包中有足够 SOL（用于交易 + Gas）
• plugin-store 已安装

请问你准备投入多少 SOL？（建议 0.5~2 SOL 起步测试）
```

### Step D2: Launch

确认后，引导用户启动 Ranking Sniper：

```bash
# 查看当前配置
plugin-store ranking-sniper config

# 启动
plugin-store ranking-sniper start
```

---

## Flow E: SOL 聪明钱跟单 (Signal Tracker)

### 原理

实时监控 SmartMoney / KOL / Whale 三类链上信号，当多个聪明钱钱包同时买入同一代币时，经过多层安全验证后自动跟单买入。不猜哪个币能涨，而是跟着最聪明的钱走。

### 策略细节

1. **监控**: 每 20 秒轮询 OKX Signal API，拉取三类钱包买入信号
2. **服务端预过滤**: MC ≥ $200K / 流动性 ≥ $80K
3. **客户端二次验证**:
   - Holders ≥ 300 / Liq/MC ≥ 5% / Top10 ≤ 50%
   - LP Burn ≥ 80% / Holder 密度 300/百万MC
4. **Dev 零容忍检查**: rug 记录 = 0 / 发币数 ≤ 20 / 持仓 ≤ 15%
5. **Bundler 操控检测**: ATH ≤ 25% / 数量 ≤ 5
6. **分级仓位** (按同车钱包数):
   - ≥8 个钱包 = 0.020 SOL
   - ≥5 个钱包 = 0.015 SOL
   - ≥3 个钱包 = 0.010 SOL
7. **退出机制**:
   - 成本感知 3 级止盈（+5%/+15%/+30% net）+ Trailing Stop（+12%激活/10%回撤）
   - 硬止损 -10% + 时间衰减止损（15min→-10% / 30min→-8% / 60min→-5%）
   - 趋势时间止损（15min K线反转）+ 4h 硬性退出
8. **Session 风控**: 连续亏损 3 次暂停 10min / 累计亏损 0.05 SOL 暂停 30min / 累计 0.10 SOL 终止

### 依赖的 plugin-store 命令

| CLI 命令 | 用途 |
|----------|------|
| `plugin-store signal-tracker tick` | 执行单次轮询 |
| `plugin-store signal-tracker start` | 启动守护进程 |
| `plugin-store signal-tracker stop` | 停止运行 |
| `plugin-store signal-tracker status` | 查看状态 |
| `plugin-store signal-tracker report` | 详细 PnL 报告 |

### Step E1: Confirm and configure

```
SOL 聪明钱跟单 运行在 Solana 链上。

需要准备：
• SOL 钱包私钥
• 钱包中有足够 SOL
• plugin-store 已安装

请问你准备投入多少 SOL？（建议 0.3~1 SOL 起步测试）
```

### Step E2: Launch

确认后，引导用户启动 Signal Tracker：

```bash
# 查看当前配置
plugin-store signal-tracker config

# 启动（推荐先用 dry-run 测试）
plugin-store signal-tracker start --dry-run

# 确认无误后正式启动
plugin-store signal-tracker start
```

---

## Flow F: SOL Memepump 扫描 (Memepump Scanner)

### 原理

实时扫描 Pump.fun 迁移代币，通过服务端安全过滤 + 客户端预分类 + Dev/Bundler 深度验证后，基于三重信号检测（TX 加速 × 成交量突增 × 买压主导）确认动量后自动买入。不预判哪个 meme 能爆，而是捕捉安全验证后的动量爆发瞬间。

### 策略细节

1. **监控**: 每 10 秒调用 Trenches tokenList API 拉取已迁移代币
2. **服务端过滤**:
   - MC $80K-$800K / Holders ≥ 50 / Dev ≤ 10% / Bundler ≤ 15%
   - Sniper ≤ 20% / Top10 ≤ 50% / 新钱包 ≤ 40% / 年龄 4-180min
3. **客户端预分类**: B/S ratio ≥ 1.3 / Vol/MC ≥ 5% / Top10 ≤ 55%
4. **Dev 零容忍检查**: rug = 0 / 发币 ≤ 20
5. **Bundler 检测**: ATH ≤ 25% / 数量 ≤ 5
6. **三重信号检测**:
   - Signal A: TX 加速（当前/前分钟 ≥ 1.35× 或投影 ≥ 60/min）
   - Signal B: 成交量突增（当前/5min 均值 ≥ 1.5-2.0×）
   - Signal C: 买压主导（1h B/S ≥ 1.5）
7. **分级仓位**:
   - SCALP（A+C）= 0.0375 SOL
   - MINIMUM（A+B+C 三信号共振）= 0.075 SOL
   - Hot Mode 自适应（高活跃市场降低 A 门槛 1.35→1.2）
8. **退出机制**:
   - 成本感知 2 级止盈（+15%/+25% net）
   - 分级止损（SCALP -15% / HOT -20% / QUIET -25%）
   - 时间止损（SCALP 5min / HOT 8min / QUIET 15min）
   - TP1 后 breakeven stop + Trailing -5%，最大持仓 30min

### 依赖的 plugin-store 命令

| CLI 命令 | 用途 |
|----------|------|
| `plugin-store scanner tick` | 执行单次扫描 |
| `plugin-store scanner start` | 启动守护进程 |
| `plugin-store scanner stop` | 停止运行 |
| `plugin-store scanner status` | 查看状态 |
| `plugin-store scanner report` | 详细 PnL 报告 |
| `plugin-store scanner analyze` | Dry-run 分析 |

### Step F1: Confirm and configure

```
SOL Memepump 扫描 运行在 Solana 链上。

需要准备：
• SOL 钱包私钥
• 钱包中有足够 SOL
• plugin-store 已安装

请问你准备投入多少 SOL？（建议 0.2~0.5 SOL 起步测试）
```

### Step F2: Launch

确认后，引导用户启动 Memepump Scanner：

```bash
# 查看当前配置
plugin-store scanner config

# 先用 analyze 观察
plugin-store scanner analyze

# 启动
plugin-store scanner start
```

---

## Strategy Comparison (Internal Reference)

| 维度 | A: USDC 智能调仓 | B: ETH/USDC 网格 | C: 稳定币杠杆循环 | D: 涨幅榜狙击 | E: 聪明钱跟单 | F: Memepump 扫描 |
|------|-------------------|-------------------|--------------------|---------------|---------------|-------------------|
| 支持链 | Base, Ethereum | Base | Ethereum, Arbitrum, Polygon, Base | Solana | Solana | Solana |
| 交易对 | USDC (单币) | ETH/USDC | USDC (单币) | SOL/Meme | SOL/Meme | SOL/Meme |
| 收益来源 | 跨协议利差 | 网格价差 | Aave 存借利差 × 杠杆 | 涨幅榜动量 | 聪明钱信号 | Pump.fun 迁移动量 |
| 风险 | Low | Medium-Low | Low | High | High | High |
| 最佳市况 | 任何市况 | 震荡行情 | 存借利差为正 | Meme 行情活跃 | 聪明钱活跃期 | Pump.fun 热潮期 |
| 最小资金 | ~$500 (ETH) | ~$50 | ~$100 (Arb) | ~0.5 SOL | ~0.3 SOL | ~0.2 SOL |
| 需要的密钥 | EVM_PRIVATE_KEY | EVM_PRIVATE_KEY + OKX API | EVM_PRIVATE_KEY | SOL 私钥 + OKX API | SOL 私钥 + OKX API | SOL 私钥 + OKX API |
| 运行方式 | 后台守护进程 | 后台守护进程 | AI 引导执行 | 后台守护进程 | 后台守护进程 | 后台守护进程 |
| CLI 命令 | `plugin-store auto-rebalance` | `plugin-store grid` | `plugin-store aave` | `plugin-store ranking-sniper` | `plugin-store signal-tracker` | `plugin-store scanner` |

## Authentication Requirements

| 策略 | 环境变量 | 说明 |
|------|---------|------|
| A | `EVM_PRIVATE_KEY` | 用于签署链上交易 |
| A (可选) | `TELEGRAM_BOT_TOKEN` + `TELEGRAM_CHAT_ID` | Telegram 通知 |
| B | `EVM_PRIVATE_KEY` | 用于签署链上交易 |
| B | `OKX_API_KEY` + `OKX_SECRET_KEY` + `OKX_PASSPHRASE` | OKX DEX 聚合器 API |
| B (可选) | `TELEGRAM_BOT_TOKEN` + `TELEGRAM_CHAT_ID` | Telegram 通知 |
| C | `EVM_PRIVATE_KEY` | 用于签署 Aave supply/borrow 交易 |
| D/E/F | `SOL_PRIVATE_KEY` | Solana 钱包私钥，用于链上交易 |
| D/E/F | `OKX_API_KEY` + `OKX_SECRET_KEY` + `OKX_PASSPHRASE` | OKX DEX API（报价 + swap） |

If user hasn't set up keys, guide them:

```
需要先配置环境变量。在 .env 文件中添加：

# EVM 策略 (A/B/C) — 钱包私钥
EVM_PRIVATE_KEY=0x...

# Solana 策略 (D/E/F) — 钱包私钥
SOL_PRIVATE_KEY=...

# 策略 B/D/E/F 需要 — OKX API
OKX_API_KEY=...
OKX_SECRET_KEY=...
OKX_PASSPHRASE=...

# 可选 — Telegram 通知
TELEGRAM_BOT_TOKEN=...
TELEGRAM_CHAT_ID=...
```

## Edge Cases

| Scenario | Behavior |
|---|---|
| User asks for both strategies | Guide to run both in separate terminals |
| User has no USDC | Suggest using `plugin-store uniswap swap` to swap first |
| User has no ETH on Base | Suggest bridging or swapping |
| EVM_PRIVATE_KEY not set | Show setup instructions before launching |
| User asks about other strategies (funding rate, sUSDe loop) | These are not yet built-in — guide user through the steps using individual `plugin-store` commands (`plugin-store aave`, `plugin-store hyperliquid`, `plugin-store ethena`) |
| Aave 利差为负 (borrow > supply) | Strategy C 不可执行，建议策略 A 或等待利率回归 |
| 健康因子过低 | 策略 C 循环时自动停止（HF < 1.30），提醒用户去杠杆 |
| User asks about a specific dApp platform | Route to the corresponding `plugin-store <dapp>` command directly |
| User asks "支持哪些平台/协议" | Show the dApp platform table |
| User says "哪个更好" | Use the comparison table; recommend A for conservative, D/E/F for aggressive Meme 玩家 |
| User has very small capital (<$50) | Recommend B on Base (low gas) or D/E/F on Solana (小额测试) |
| User has large capital (>$10k) | Recommend A on Ethereum (higher TVL, deeper liquidity) |
| User asks about Solana Meme 策略 | Show D/E/F options, explain each strategy's signal source differs |
| plugin-store 未安装 | 引导安装: `curl -sSL .../install.sh \| sh` |
| SOL_PRIVATE_KEY not set | Show setup instructions, warn about Meme 币高风险 |
| User asks "哪个 Solana 策略更好" | D 最稳（榜单动量）、E 最聪明（跟单）、F 最激进（Pump.fun），建议小额分散测试 |
