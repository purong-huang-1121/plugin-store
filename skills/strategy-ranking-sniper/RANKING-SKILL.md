---
name: sol-ranking-sniper
description: "Solana Token Ranking Sniper v0 — 自动化 Meme 币狙击策略。当用户说'启动排名狙击'、'ranking sniper'、'排名策略'、'启动 sol-ranking-sniper'、'Meme 币自动交易'、'Top N 狙击' 时触发。基于 OKX DEX Token Ranking API + Advanced Info 安全检查实时监控 Solana 涨幅榜，自动买入新上榜代币并通过多层退出系统管理仓位。"
license: Apache-2.0
metadata:
  author: okx
  version: "0.0.1"
  homepage: "https://web3.okx.com"
---

# SOL Ranking Sniper v0 SKILL

## 概述

SOL Ranking Sniper 是一个基于 **OKX Onchain OS 公开 MCP** 的 Solana Meme 币自动狙击策略。

核心逻辑：每 10 秒轮询 Solana 涨幅排行榜 Top 20，当有**新代币首次上榜**时，经过三级过滤：
1. **一级 Slot Guard**（基于 ranking 返回的基础指标：涨幅、流动性、市值、买入比、持有者数等）
2. **二级 Advanced Safety Check**（调用 `dex-okx-market-token-advanced-info` 获取蜜罐风险、开发者 rug 历史、狙击手持仓、LP 销毁、Top10 集中度等安全数据）
3. **三级 Holder Risk Scan**（调用 `dex-okx-market-token-holder` 检测可疑地址、疑似钓鱼地址持仓占比）

通过三级过滤后计算 **Momentum Score** 评分，高分优先买入。持仓后通过 **6 层退出系统**（排名退出 → 硬止损 → 快速止损 → 追踪止损 → 时间止损 → 梯度止盈）自动管理仓位。

策略设计理念：捕捉 Meme 币在涨幅榜初次出现时的短期动量，在代币跌出排行榜前快速获利离场。

## 使用的 OKX Onchain OS MCP 接口

本策略**使用以下 OKX Onchain OS 公开 MCP 接口**：

| MCP Tool | 用途 | 调用频率 |
|---|---|---|
| `dex-okx-market-token-ranking` | 获取涨幅排行榜 Top 20 | 每 10 秒 |
| `dex-okx-market-token-advanced-info` | 获取代币安全/风控数据（蜜罐、dev rug 历史、狙击手、LP 销毁等） | 每个新上榜代币调用 1 次 |
| `dex-okx-market-token-holder` | 检测可疑/钓鱼地址持仓（tagFilter=6 Suspicious, tagFilter=8 Suspected Phishing） | 每个新上榜代币调用 2 次 |
| `dex-okx-index-current-price` | 获取 SOL 及持仓代币实时价格 | 每 10 秒（仓位监控） |
| `dex-okx-dex-quote` | 获取 DEX 聚合报价（Paper 模式模拟交易摩擦） | 每次买卖 |
| `dex-okx-dex-swap` | 获取 DEX 聚合交易数据（Live 模式返回可签名的 Solana 交易） | 每次实盘买卖 |

### MCP 调用方式

所有 MCP 调用通过 HTTP POST 到 `https://web3.okx.com/api/v1/plugin-store-mcp`：

```javascript
async function mcpCall(toolName, args) {
  const res = await fetch('https://web3.okx.com/api/v1/plugin-store-mcp', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'OK-ACCESS-KEY': process.env.OKX_API_KEY,
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      method: 'tools/call',
      params: { name: toolName, arguments: args },
      id: ++requestId,
    }),
    signal: AbortSignal.timeout(15000),
  });
  // ⚠️ 必须先读 text 再 parse，避免非 JSON 响应导致 .json() 报错
  const raw = await res.text();
  if (!res.ok) throw new Error(`MCP HTTP ${res.status}: ${raw.slice(0, 200)}`);
  let json;
  try { json = JSON.parse(raw); } catch { throw new Error(`MCP non-JSON: ${raw.slice(0, 200)}`); }
  if (json.error) throw new Error(json.error.message || JSON.stringify(json.error));
  const text = json.result?.content?.[0]?.text;
  if (!text) throw new Error(`MCP empty for ${toolName}`);
  let parsed;
  try { parsed = JSON.parse(text); } catch { throw new Error(`MCP text error: ${text.slice(0, 200)}`); }
  if (parsed.code !== 0 && parsed.code !== '0') throw new Error(`MCP [${parsed.code}]: ${parsed.msg || ''}`);
  const data = parsed.data;
  // 部分 MCP 返回类数组对象 {0:..., 1:...} 而非真数组，需转换
  if (data && typeof data === 'object' && !Array.isArray(data) && '0' in data) {
    return Object.values(data);
  }
  return data;
}
```

**注意事项：**
- **MCP 响应解析必须两步**：先 `res.text()` 读取原始文本，再 `JSON.parse()`。直接用 `res.json()` 在服务端返回非 JSON 错误时会丢失错误信息
- `dex-okx-market-token-ranking` 使用参数 `chains`（复数），其他接口使用 `chainIndex`（单数）
- **`dex-okx-dex-swap` / `dex-okx-dex-quote` 返回数组**：data 是 `[{routerResult, tx}]` 数组格式，取第一个元素 `data[0]`
- **`dex-okx-dex-swap` 滑点参数名为 `slippagePercent`**（字符串 `"2"` 表示 2%），不是 `slippage`
- **Solana swap 交易数据 `tx.data` 为 base58 编码**，不是 base64。反序列化时需用 `bs58.decode()` 而非 `Buffer.from(data, 'base64')`
- `dex-okx-index-current-price` 需要 `items` 数组格式（支持批量查询最多 100 个代币）：
```javascript
mcpCall('dex-okx-index-current-price', {
  items: [
    { chainIndex: '501', tokenContractAddress: 'So111...112' },
    { chainIndex: '501', tokenContractAddress: '<token_addr>' },
  ],
})
// 返回: [{ chainIndex, tokenContractAddress, price, time }]
```

## 前置条件

### 环境要求
- Node.js >= 18（ESM 模块支持）
- npm 或 pnpm 包管理器

### 依赖包
```json
{
  "@solana/web3.js": "^1.98.4",
  "bs58": "^6.0.0",
  "express": "^5.x",
  "cors": "^2.8.x",
  "dotenv": "^17.x"
}
```

Dashboard UI（必须构建）：
```json
{
  "react": "^19.x",
  "recharts": "^3.x",
  "tailwindcss": "^4.x",
  "vite": "^7.x"
}
```

### 前端 UI 设计规范（OKX Branding）

Dashboard 前端需遵循 **OKX 品牌视觉规范**，确保与 OKX 产品风格一致：

**配色体系：**

| 用途 | 色值 | 说明 |
|------|------|------|
| 主色 | `#FFFFFF` | 品牌主色（按钮、高亮、重要操作） |
| 背景（暗色模式） | `#0B0E11` | 主背景色 |
| 卡片背景 | `#1B1F25` | 面板/卡片区域 |
| 次级背景 | `#252930` | 输入框/表格行 |
| 主文字 | `#FFFFFF` | 标题/数值 |
| 次文字 | `#8B919E` | 标签/说明文字 |
| 涨/盈利 | `#2DC98A` | PnL 正值、涨幅 |
| 跌/亏损 | `#F04866` | PnL 负值、止损触发 |
| 警告 | `#F0B90B` | 风控警告、引擎暂停 |
| 分隔线 | `#2B3039` | 区域分隔 |

**字体：**
- 英文/数字：`"Roboto Mono", "SF Mono", monospace`（数据密集型场景用等宽字体）
- 中文：`"PingFang SC", "Microsoft YaHei", sans-serif`
- 标题字重：`600`（Semi Bold），正文：`400`（Regular）

**UI 组件风格：**

| 组件 | 规范 |
|------|------|
| 卡片 | 圆角 `12px`，背景 `#1B1F25`，无阴影，`1px solid #2B3039` 边框 |
| 按钮（主要） | 背景 `#FFFFFF`，文字 `#0B0E11`，圆角 `8px`，hover 透明度 `0.85` |
| 按钮（次要） | 背景 `transparent`，边框 `1px solid #8B919E`，文字 `#FFFFFF` |
| 表格 | 无外边框，行间 `1px solid #2B3039` 分隔，header 加粗 `#8B919E` |
| 数字展示 | 等宽字体，涨跌带色，金额前缀 `$` 或后缀 `SOL` |
| 状态标签 | 圆角 `4px`，字号 `12px`，背景带 `12%` 透明度主色 |
| 图表 | Recharts 默认暗色主题，grid 色 `#2B3039`，tooltip 背景 `#1B1F25` |

**Dashboard 布局参考：**
```
┌─────────────────────────────────────────────────────┐
│  SOL Ranking Sniper v0          [Running ●]  [Stop] │  ← Header
├──────────┬──────────┬──────────┬───────────────────── │
│ 总 PnL   │ 今日 PnL │ 胜率     │ 持仓数/上限         │  ← Stats Bar
│ +0.12SOL │ -0.03SOL │ 42.8%   │ 3 / 5              │
├──────────┴──────────┴──────────┴───────────────────── │
│  当前持仓                                             │
│  ┌────────┬────────┬────────┬──────┬───────────────┐ │
│  │ Token  │ Entry  │ Current│ PnL% │ Exit Trigger  │ │  ← Positions
│  ├────────┼────────┼────────┼──────┼───────────────┤ │
│  │ BONK   │ $0.012 │ $0.014 │ +16% │ Trail ▲8%    │ │
│  └────────┴────────┴────────┴──────┴───────────────┘ │
├─────────────────────────────────────────────────────── │
│  排行榜 Top 20              │  信号日志               │
│  ┌──────┬───────┬────────┐ │  12:01 BUY BONK +23%   │  ← Ranking + Logs
│  │ Rank │ Token │ Change │ │  12:00 SKIP ABC: liq<5k │
│  └──────┴───────┴────────┘ │  11:59 SELL XYZ -8%     │
└─────────────────────────────────────────────────────┘
```

**OKX Logo 使用：**
- Dashboard 左上角展示 "Powered by OKX Onchain OS" 文字标识
- 不嵌入 OKX Logo 图片文件（避免版权分发问题），使用纯文字 + SVG 简化标识
- 字号 `14px`，颜色 `#8B919E`

### API Keys & 环境变量
在项目根目录 `.env` 文件中配置：
```
OKX_API_KEY=<OKX Web3 API Key>          # 必需：OKX Onchain OS MCP API 访问
SOLANA_PRIVATE_KEY=<base58 私钥>         # Live 模式必需：用于签名交易
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com  # 可选：自定义 RPC
PORT=3051                               # 可选：API 服务端口（默认 3051，避免与其他策略冲突）
DASHBOARD_PORT=5051                     # 可选：Dashboard 端口（默认 5051）
```

获取 OKX API Key：前往 [OKX Developer Portal](https://web3.okx.com/onchain-os/dev-portal) 创建。

## 用户初始化输入

项目搭建完成后，AI **必须主动询问用户**以下参数，不可直接使用默认值跳过。用户确认后再写入 `state/config.json` 并启动引擎。

### 必问参数（无默认值，必须用户输入）

| 参数 | 类型 | 范围 | 说明 | 示例 |
|------|------|------|------|------|
| `mode` | `string` | `"paper"` / `"live"` | 模拟模式还是实盘模式 | `"paper"` |
| `totalBudget` | `number` | 1-100 SOL | 策略总预算，控制整体风险敞口 | `5` |
| `buyAmountPerTrade` | `number` | 0.01-1 SOL | 单笔买入金额，必须 ≤ totalBudget | `0.05` |
| `dailyLossLimitRatio` | `number` | 0.05-0.50 | 日亏损停止线（占 totalBudget 比例），触发后当日停止所有买入 | `0.15` |

### 可选参数（有默认值，用户可调整）

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `maxPositions` | `number` | `5` | 最大同时持仓数（1-20） |
| `riskLevel` | `string` | `"moderate"` | `"conservative"` / `"moderate"` / `"aggressive"` |
| `myWallet` | `string` | — | Solana 钱包地址（Live 模式必填，用于展示） |

### 初始化对话示例

```
AI: 策略搭建完成！启动前需要确认几个参数：

1. 交易模式？ paper（模拟）/ live（实盘）
2. 策略总预算多少 SOL？（范围 1-100）
3. 每笔买入多少 SOL？（范围 0.01-1，建议不超过总预算的 5%）
4. 日亏损停止线？（占总预算比例，如 0.15 = 亏 15% 当日停止买入）

User: live, 10 SOL, 每笔 0.1, 亏损 20% 停

AI: 收到，配置如下：
- 模式：Live（实盘）
- 总预算：10 SOL
- 单笔买入：0.1 SOL
- 日亏损停止线：20%（即当日亏损超 2 SOL 停止买入）
- 最大持仓：5（默认）
正在写入配置并启动引擎...
```

## 风险偏好配置

### Moderate（默认 / 推荐）
| 参数 | 值 | 说明 |
|------|------|------|
| `buyAmountPerTrade` | 0.05 SOL | 单笔约 $6.5 |
| `maxPositions` | 5 | 同时最多 5 个持仓 |
| `stopLoss.percent` | -25% | 硬止损 |
| `quickStop` | 5min / -8% | 快速止损 |
| `trailingStop` | 激活 +8% / 回撤 12% | 追踪止损 |
| `takeProfit.tiers` | +5% / +15% / +30% 分三阶段 | 梯度止盈 |
| `timeStop.maxHoldHours` | 6 小时 | 时间止损 |
| `dailyLossLimitRatio` | 15% of totalBudget | 日亏损上限 |

### Conservative（保守）
可调整：`buyAmountPerTrade=0.02`, `stopLoss=-15%`, `quickStop 3min/-5%`, `maxPositions=3`

### Aggressive（激进）
可调整：`buyAmountPerTrade=0.1`, `stopLoss=-35%`, `maxChangePercent=300`, `maxPositions=8`

## 信号源规范

### 唯一信号源：OKX Token Ranking

| 字段 | 值 | 说明 |
|------|------|------|
| API 工具 | `dex-okx-market-token-ranking` | OKX Onchain OS MCP |
| `chains` | `"501"` | Solana |
| `sortBy` | `"2"` | 按价格变化（%）排名 |
| `timeFrame` | `"1"` | 5 分钟窗口 |
| `topN` | `20` | 取前 20 名（API 返回后截取） |
| `pollInterval` | `10000` ms | 每 10 秒轮询一次 |

### 排行榜返回字段（用于一级过滤）
```
tokenContractAddress, tokenSymbol, price, change (%),
liquidity, marketCap, holders, volume,
txs, txsBuy, txsSell, uniqueTraders
```

## 买入决策规则

### 一级过滤：Slot Guard（基于 ranking 数据）

按顺序检查，任一不通过则跳过该代币：

| # | 检查项 | 条件 | 默认阈值 |
|---|--------|------|----------|
| 1 | 涨幅下限 | `change >= minChangePercent` | 5% |
| 2 | 涨幅上限 | `change <= maxChangePercent` | 5000% |
| 3 | 流动性 | `liquidity >= minLiquidity` | $3,000 |
| 4 | 市值下限 | `marketCap >= minMarketCap` | $3,000 |
| 5 | 市值上限 | `marketCap <= maxMarketCap` | $500,000,000 |
| 6 | 持有者数 | `holders >= minHolders` | 10 |
| 7 | 买入比 | `txsBuy / txs >= minBuyRatio` | 40% |
| 8 | 独立交易者 | `uniqueTraders >= minTraders` | 10 |
| 9 | 黑名单 | 不在 `skipTokens` / `blacklist` 中 | SOL, USDC, 系统地址 |
| 10 | 冷却期 | 距上次卖出 >= `cooldownMinutes` | 5 分钟 |
| 11 | 仓位上限 | 当前持仓数 < `maxPositions` | 5 |
| 12 | 去重 | 未持有该代币 | — |
| 13 | 日亏损上限 | 今日亏损 < `totalBudget * dailyLossLimitRatio` | 15% |

### 二级过滤：Advanced Safety Check（安全审查）

通过一级过滤的代币，调用 `dex-okx-market-token-advanced-info` 获取安全数据，执行以下检查：

```
mcpCall('dex-okx-market-token-advanced-info', {
  chainIndex: '501',
  tokenContractAddress: token.tokenContractAddress,
})
```

返回字段与检查规则：

| # | 检查项 | 字段 | 条件 | 默认阈值 | 说明 |
|---|--------|------|------|----------|------|
| S1 | 风控等级 | `riskControlLevel` | `level <= maxRiskLevel` | 3 | 1=低风险, 2=中风险, 3=高风险 |
| S2 | 蜜罐/貔貅检测 | `tokenTags` 含 `"honeypot"` | 不含 | — | 命中即拒绝 |
| S3 | Top10 集中度 | `top10HoldPercent` | `<= maxTop10HoldPercent` | 80% | 持仓过度集中 = 高风险 |
| S4 | 开发者持仓 | `devHoldingPercent` | `<= maxDevHoldPercent` | 50% | 开发者还持有大量 = 可能砸盘 |
| S5 | Bundler 持仓 | `bundleHoldingPercent` | `<= maxBundleHoldPercent` | 30% | 捆绑交易持仓过高 = 操纵 |
| S6 | LP 销毁（已毕业代币） | `lpBurnedPercent` | `>= minLpBurnPercent` | 0% | 仅对已毕业代币检查（`isInternal=false`），设为 0 以兼容未烧 LP 代币 |
| S7 | 开发者 Rug 历史 | `devRugPullTokenCount` | `<= maxDevRugCount` | 20 | 开发者历史 rug pull 代币数过多 = 高危 |
| S8 | 狙击手持仓 | `sniperHoldingPercent` | `<= maxSniperHoldPercent` | 30% | 狙击手占比过高 = 即将抛压 |
| S9 | 内盘检测 | `isInternal` | `=== false` 或允许内盘 | 默认允许内盘 | PumpFun 未毕业代币，`blockInternal: false` |

安全检查通过后，进入三级 Holder Risk Scan。

### 三级过滤：Holder Risk Scan（持有者风险扫描）

通过二级过滤的代币，调用 `dex-okx-market-token-holder` 检测可疑和钓鱼地址：

```
// 查询可疑地址持仓
const suspicious = await mcpCall('dex-okx-market-token-holder', {
  chainIndex: '501',
  tokenContractAddress: token.tokenContractAddress,
  tagFilter: '6',  // Suspicious
});

// 查询疑似钓鱼地址持仓
const phishing = await mcpCall('dex-okx-market-token-holder', {
  chainIndex: '501',
  tokenContractAddress: token.tokenContractAddress,
  tagFilter: '8',  // Suspected Phishing
});
```

| # | 检查项 | 条件 | 默认阈值 | 说明 |
|---|--------|------|----------|------|
| H1 | 可疑地址持仓 | 可疑地址总 holdPercent 之和 | ≤ 50% | 可疑地址占比过高 = 操纵风险 |
| H2 | 钓鱼地址存在 | phishing 结果中仍在持仓的地址数 | 不检查（`blockPhishingHolder: false`） | 可通过配置启用 |
| H3 | 可疑地址数量 | suspicious 结果中仍在持仓（holdPercent > 0）的地址数 | ≤ 20 | 过多可疑地址 = 协调操纵 |

持有者风险检查计算方式：
```javascript
// 计算仍在持仓的可疑地址占比
const suspiciousHoldPercent = suspicious
  .filter(h => parseFloat(h.holdPercent) > 0)
  .reduce((sum, h) => sum + parseFloat(h.holdPercent) * 100, 0);
const suspiciousActiveCount = suspicious
  .filter(h => parseFloat(h.holdPercent) > 0).length;

// 计算仍在持仓的钓鱼地址数
const phishingActiveCount = phishing
  .filter(h => parseFloat(h.holdPercent) > 0).length;

if (suspiciousHoldPercent > 10) reject('SuspiciousHold:' + suspiciousHoldPercent.toFixed(1) + '%');
if (phishingActiveCount > 0) reject('PhishingHolder:' + phishingActiveCount);
if (suspiciousActiveCount > 5) reject('SuspiciousCount:' + suspiciousActiveCount);
```

三级过滤全部通过后，从 `tokenTags`、`advanced-info` 和 `holder` 数据中提取加分项用于 Momentum Score 计算。

### Momentum Score 计算（0-125 分）

```
每个通过两级过滤的代币计算综合动量评分：

Base Score（0-100 分，来自 ranking 数据）:
  buyScore      = min(buyRatio, 1) * 40           // 买入比权重最高
  changePenalty = change > 100 ?
                  max(0, 20 - (change-100)/10) :
                  min(change/5, 20)                // 适中涨幅优于极端
  traderScore   = min(traders/50, 1) * 20          // 独立交易者
  liqScore      = min(liquidity/50000, 1) * 20     // 流动性

Bonus Score（0-25 分，来自 advanced-info + holder 数据）:
  smartMoneyBonus     = tokenTags 含 'smartMoneyBuy' ? +8 : 0
  concentrationBonus  = top10HoldPercent < 30 ? +5 : (< 50 ? +2 : 0)
  dsPaidBonus         = tokenTags 含 'dsPaid' ? +3 : 0
  communityBonus      = tokenTags 含 'dexScreenerTokenCommunityTakeOver' ? +2 : 0
  lowSniperBonus      = sniperHoldingPercent < 5 ? +4 : (< 10 ? +2 : 0)
  devCleanBonus       = devHoldingPercent == 0 && devRugPullTokenCount < 3 ? +3 : 0
  zeroSuspiciousBonus = suspiciousActiveCount == 0 ? +2 : 0  // 无可疑地址 = 更干净

  bonusTotal = min(smartMoneyBonus + concentrationBonus + dsPaidBonus
                   + communityBonus + lowSniperBonus + devCleanBonus
                   + zeroSuspiciousBonus, 25)

totalScore = base + bonusTotal
```

### 买入触发逻辑

```
每 10 秒执行一次 pollRanking():

1. 调用 dex-okx-market-token-ranking(chains='501', sortBy='2', timeFrame='1')
   → 获取当前 Top 20 代币列表
2. 计算差集：newEntries = currentSnapshot - prevSnapshot
   （首次启动时仅记录快照，不触发买入）
3. 更新快照：prevSnapshot = currentSnapshot
4. 对每个 newEntry:
   a. 执行一级 Slot Guard 13 项检查 → 不通过则 SKIP 并记录原因
   b. 通过后调用 dex-okx-market-token-advanced-info → 执行二级安全检查 9 项
      → 不通过则 SKIP 并记录安全拒绝原因（如 "Honeypot", "DevRug:63", "Sniper:14%"）
   c. 通过后调用 dex-okx-market-token-holder(tagFilter=6,8) → 执行三级持有者风险检查 3 项
      → 不通过则 SKIP 并记录原因（如 "PhishingHolder:2", "SuspiciousHold:15.3%"）
   d. 全部通过后计算 Momentum Score（0-125）
5. 按 Momentum Score 降序排序
6. 依次执行买入（高分优先），每笔间隔 2s（Live）/ 300ms（Paper）
```

### 买入执行参数

| 参数 | 值 | 说明 |
|------|------|------|
| 金额 | `buyAmountPerTrade`（0.05 SOL） | 每笔买入固定金额 |
| 滑点 | `slippagePercent`（2%） | OKX DEX 聚合器滑点 |
| 模式 | `exactIn` | 精确输入 SOL 数量 |
| 并发控制 | `buyingNow` Set | 同一代币不重复买入 |
| 买入间隔 | Live: 2000ms / Paper: 300ms | 避免 Solana RPC 429 限流 |

**Paper 模式买入**：调用 `dex-okx-dex-quote` 获取报价模拟真实摩擦（price impact），用报价结果计算有效买入价。

**Live 模式买入（WSOL 处理流程关键）**：

Solana DEX 交易需要 WSOL（Wrapped SOL）。OKX 返回的 swap 交易假设 WSOL 账户已预充值，因此每次买入前必须执行：
```
1. 检查 WSOL ATA 账户是否存在
2. 如果存在 → CloseAccount（回收旧余额）
3. CreateIdempotent ATA（创建新账户）
4. SystemProgram.transfer（充入 amountLamports）
5. SyncNative（同步 token 余额）
   ↑ 以上 2-5 合并为一个 Solana Transaction
6. 发送并确认 prepareWsol 交易
7. 调用 dex-okx-dex-swap → 取 data[0]（返回数组） → 签名 → 发送 → 确认
8. CleanupWsol（关闭 WSOL 账户回收 rent）
```

**⚠️ Swap API 关键细节：**
- **参数名**：滑点参数为 `slippagePercent`（字符串 `"2"` 表示 2%），不是 `slippage`
- **返回格式**：`dex-okx-dex-swap` 和 `dex-okx-dex-quote` 返回数组 `[{routerResult, tx}]`，必须取 `data[0]`
- **交易编码**：`tx.data` 为 **base58** 编码的 Solana VersionedTransaction，反序列化时需要：
```javascript
// 自动检测编码格式（hex / base64 / base58）
let buf;
if (callData.startsWith('0x')) {
  buf = Buffer.from(callData.slice(2), 'hex');
} else if (/^[A-Za-z0-9+/=]+$/.test(callData) && callData.length % 4 === 0) {
  buf = Buffer.from(callData, 'base64');
} else {
  buf = Buffer.from(bs58.decode(callData));  // Solana 默认 base58
}
const tx = VersionedTransaction.deserialize(buf);
tx.sign([keypair]);
```

**买入价格三级回退**
```
price = OKX routerResult.toTokenUsdPrice
if (!price || price <= 0): price = ranking API token.price
if (!price || price <= 0): price = (buyAmountSol * solPrice) / tokenAmount
if (!price || price <= 0): SKIP — 不建仓（避免 PnL Infinity）
```

## 卖出决策规则

### 卖出触发机制

每 10 秒检查一次所有持仓（`monitorPositions`），按以下优先级依次判断，命中即卖出：

| 优先级 | 退出机制 | 触发条件 | 卖出比例 | 设计理念 |
|--------|---------|---------|---------|---------|
| **EXIT 0** | 排名退出 | 代币**不再出现在** Top N 排行榜 且持仓 >= 1 分钟 | 100% | 动量消失 = 立刻离场 |
| **EXIT 1** | 硬止损 | `pnlPercent <= -25%` | 100% | 最大亏损保护 |
| **EXIT 2** | 快速止损 | 持仓 >= 5 分钟 且 `pnlPercent <= -8%` | 100% | 动量不对，快速出局 |
| **EXIT 3** | 追踪止损 | 峰值 PnL >= +8% 后，从峰值回撤 >= 12% | 100% | 保护已到手利润 |
| **EXIT 4** | 时间止损 | 持仓时间 >= 6 小时 | 100% | Meme 币不适合长持 |
| **EXIT 5** | 梯度止盈 TP1 | `pnlPercent >= +5%` | 25% | 锁定小利润 |
| **EXIT 5** | 梯度止盈 TP2 | `pnlPercent >= +15%` | 35% | 大部分利润落袋 |
| **EXIT 5** | 梯度止盈 TP3 | `pnlPercent >= +30%` | 40% | 剩余全出 |
| **StopExit** | 停止清仓 | 用户点击**停止引擎** | 100% | 停止时不遗留未平仓位 |

### 卖出执行参数

| 参数 | 值 | 说明 |
|------|------|------|
| 滑点 | `slippagePercent`（2%） | OKX DEX 聚合器 |
| 模式 | `exactIn` | 精确卖出代币数量 |
| 冷却记录 | 全额卖出或亏损卖出后记录 | `cooldownMinutes`（30 分钟） |

## 定时任务清单

| 任务名 | 频率 | 职责 | 需要 LLM |
|--------|------|------|----------|
| `pollRanking` | 每 10 秒 | 轮询排行榜 → 检测新上榜 → 一级过滤 → 二级安全检查 → 买入 | 否 |
| `monitorPositions` | 每 10 秒 | 获取所有持仓最新价格 → 依次触发 6 层退出系统 | 否 |

## 项目结构

策略以 Node.js ESM 项目构建，结构如下：

```
sol-ranking-sniper/
├── .env                     # 环境变量（API Key, 私钥）
├── package.json             # 依赖声明
├── server/
│   ├── index.mjs            # Express 服务入口，监听端口 PORT（默认 3051）
│   ├── routes.mjs           # API 路由定义
│   ├── engine.mjs           # 策略引擎（pollRanking + monitorPositions）
│   └── lib/
│       ├── okx-api.mjs      # OKX MCP 调用封装（仅公开接口）
│       ├── state.mjs        # 状态文件读写（JSON 持久化）
│       └── swap-executor.mjs # Solana 链上交易执行（WSOL 管理 + 签名发送）
├── state/                   # 运行时状态（JSON 文件）
│   ├── config.json          # 策略完整配置（共享，含 mode 字段）
│   ├── paper/               # 模拟盘数据（自动创建）
│   │   ├── positions.json
│   │   ├── trades.json
│   │   ├── daily-stats.json
│   │   ├── signals-log.json
│   │   └── signals-seen.json
│   └── live/                # 实盘数据（自动创建）
│       ├── positions.json
│       ├── trades.json
│       ├── daily-stats.json
│       ├── signals-log.json
│       └── signals-seen.json
└── dashboard/               # React + Vite Dashboard（必须构建）
    ├── index.html
    ├── package.json
    ├── vite.config.js         # 含 proxy 到 API 服务
    ├── tailwind.config.js
    └── src/
        ├── main.jsx
        ├── index.css          # Tailwind + OKX Branding 全局样式
        └── App.jsx            # 主 Dashboard 组件
```

### API 路由

| 路由 | 方法 | 说明 |
|------|------|------|
| `/api/status` | GET | 引擎状态（running, mode, positionsCount）；实盘模式额外返回 `wallet`、`solBalance` |
| `/api/start` | POST | 启动策略引擎 |
| `/api/stop` | POST | 停止策略引擎（**自动清仓所有持仓**，实盘发链上卖出交易） |
| `/api/positions` | GET | 当前模式的持仓列表 |
| `/api/trades` | GET | 当前模式的历史交易记录 |
| `/api/logs` | GET | 最近 50 条日志 |
| `/api/roster` | GET | 当前排行榜 Top N 代币 |
| `/api/mode` | POST | 切换模式 — Body: `{ "mode": "paper" \| "live" }`，需先停止引擎 |
| `/api/reset` | POST | 清空当前模式的所有数据（持仓、交易、日志），需先停止引擎 |

## 项目初始化流程

AI 收到用户指令后，**必须按以下顺序**完整搭建项目：

### Step 1：创建项目骨架
```
mkdir -p sol-ranking-sniper/{server/lib,state,dashboard/src}
```

### Step 2：初始化后端
1. 创建 `package.json`（type: "module"，声明所有依赖）
2. 创建 `.env`（提示用户填入 `OKX_API_KEY`、`SOLANA_PRIVATE_KEY`）
3. 创建 `.gitignore`（必须包含 `.env`、`state/`、`node_modules/`）
4. 创建 `state/config.json`（完整配置，参考"配置文件示例"章节）
5. `npm install`

### Step 3：实现核心模块（按依赖顺序）
1. `server/lib/okx-api.mjs` — MCP 调用封装（注意 `chains` vs `chainIndex` 参数差异、`current-price` 的 `items` 数组格式、类数组对象转换）
2. `server/lib/state.mjs` — JSON 状态文件读写（原子写入：先写 .tmp 再 rename）
3. `server/lib/swap-executor.mjs` — Solana 链上交易（WSOL 管理 + 签名；私钥仅在此模块内使用）
4. `server/engine.mjs` — 策略引擎（pollRanking + monitorPositions + 三级过滤 + 六层退出）
5. `server/routes.mjs` — API 路由
6. `server/index.mjs` — Express 入口（端口从 `process.env.PORT || 3051` 读取，绑定 `127.0.0.1`）

### Step 4：构建 Dashboard 前端（必须）
1. `cd dashboard && npm create vite@latest . -- --template react`
2. `npm install && npm install -D tailwindcss@3 postcss autoprefixer && npx tailwindcss init -p`
3. 配置 `tailwind.config.js`（OKX Branding 色值）
4. 配置 `vite.config.js`（proxy `/api` → 后端服务端口）
5. 实现 `src/index.css`（Tailwind + OKX 暗色全局样式）
6. 实现 `src/App.jsx`（完整 Dashboard：Header + StatsBar + Positions + Trades + Ranking + Logs）
7. 遵循"前端 UI 设计规范（OKX Branding）"章节的配色/字体/组件/布局

### Step 5：启动验证
1. 启动后端：`node server/index.mjs`
2. 启动前端：`cd dashboard && npx vite --port ${DASHBOARD_PORT:-5051}`
3. 验证 `/health` 和 `/api/status` 返回正常
4. 验证排行榜数据在 Dashboard 中显示
5. 验证信号日志中出现 SKIP/SAFETY_REJECT/PASS 等事件

### Step 6：提示用户
告知用户：
- API 服务地址：`http://127.0.0.1:${PORT}`
- Dashboard 地址：`http://localhost:${DASHBOARD_PORT}`
- 当前模式：Paper/Live
- 引擎已自动启动

## 通知推送规范

通过服务器 console 日志 + Dashboard UI 实时展示：

| 事件类型 | 触发条件 | 日志格式 |
|---------|---------|---------|
| `BUY` | 买入执行成功 | `{symbol} \| +{change}% \| BR:{buyRatio}% \| S:{score} \| {amount}SOL \| ${price}` |
| `SELL` | 卖出执行成功 | `{symbol} \| {reason} \| PnL:{pnlPercent}%({pnlSol}SOL)` |
| `SKIP` | 代币未通过一级 Slot Guard | `{symbol}: {reasons}` |
| `SAFETY_REJECT` | 代币未通过二级安全检查 | `{symbol}: {safetyReasons}` （如 `DevRug:63`, `Sniper:14%`, `Honeypot`） |
| `HOLDER_REJECT` | 代币未通过三级持有者风险检查 | `{symbol}: {holderReasons}` （如 `PhishingHolder:2`, `SuspiciousHold:15%`） |
| `RANK_EXIT` | 排名退出触发 | `{symbol} dropped from ranking, PnL: {pnl}%` |
| `LIVE_BUY` | 实盘买入确认 | `{symbol} \| tx: {txHash} \| ${price}` |
| `StopExit` | 停止引擎清仓 | `{symbol} \| StopExit \| PnL:{pnl}%` |
| `ERROR` | 交易/API 异常 | 具体错误信息 |
| `ENGINE` | 引擎启停 | 策略版本及参数概要 |

## 数据存储规范

所有状态以 JSON 文件存储于 `state/` 目录：

**Position 结构**
```json
{
  "tokenAddress": "436wV8pV...",
  "tokenSymbol": "Loopy",
  "decimal": 9,
  "buyPrice": "0.000040280562643865",
  "buyAmountSol": "0.05",
  "holdAmount": "274.82319297",
  "buyCount": 1,
  "buyTimestamp": 1741180000000,
  "lastCheckPrice": "0.000042",
  "lastCheckTime": 1741180060000,
  "peakPrice": "0.000045",
  "takeProfitTier": 0,
  "triggerReason": "Rank +45% S:62",
  "safetyData": {
    "riskControlLevel": "1",
    "top10HoldPercent": "24.38",
    "devHoldingPercent": "0",
    "bundleHoldingPercent": "0.48",
    "sniperHoldingPercent": "3.2",
    "devRugPullTokenCount": "0",
    "hasSmartMoney": true
  }
}
```

**Trade 结构**
```json
{
  "tradeId": "buy-1741180000000-a3f2",
  "timestamp": 1741180000000,
  "direction": "buy | sell",
  "tokenAddress": "436w...",
  "tokenSymbol": "Loopy",
  "amountSol": "0.05",
  "amountToken": "274.82",
  "priceUsd": "0.00004028",
  "txHash": "5xK9...",
  "reason": "rank_score_62 | SL(-25%) | TP1(+5%) | RankExit | StopExit | ...",
  "pnlPercent": "4.27",
  "pnlSol": "0.00213",
  "mode": "live | paper"
}
```

## 配置文件示例

```jsonc
{
  "strategyId": "sol-ranking-sniper",
  "version": "0.0.1",
  "mode": "<USER_INPUT: paper | live>",
  "chainIndex": "501",
  "myWallet": "",
  "totalBudget": "<USER_INPUT: 1-100 SOL>",
  "riskLevel": "moderate",
  "dailyLossLimitRatio": "<USER_INPUT: 0.05-0.50>",
  "trading": {
    "buyAmountPerTrade": "<USER_INPUT: 0.01-1 SOL>",
    "maxPositions": 5,
    "maxSingleTokenBuys": 1,
    "slippagePercent": 2,
    "gasReserve": 0.01,
    "minWalletBalance": 0.1
  },
  "ranking": {
    "pollInterval": 10000,
    "timeFrame": "1",
    "sortBy": "2",
    "topN": 20,
    "minChangePercent": 5,
    "maxChangePercent": 5000,
    "minLiquidity": 3000,
    "minMarketCap": 3000,
    "maxMarketCap": 500000000,
    "minHolders": 10,
    "minBuyRatio": 0.4,
    "minTraders": 10,
    "cooldownMinutes": 5,
    "enableRankingExit": true,
    "skipTokens": [
      "11111111111111111111111111111111",
      "So11111111111111111111111111111111111111112",
      "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    ],
    "blacklist": []
  },
  "safety": {
    "maxRiskLevel": 3,
    "blockHoneypot": true,
    "maxTop10HoldPercent": 80,
    "maxDevHoldPercent": 50,
    "maxBundleHoldPercent": 30,
    "minLpBurnPercent": 0,
    "maxDevRugCount": 20,
    "maxSniperHoldPercent": 30,
    "blockInternal": false,
    "maxSuspiciousHoldPercent": 50,
    "maxSuspiciousActiveCount": 20,
    "blockPhishingHolder": false
  },
  "takeProfit": {
    "tiers": [
      { "percent": 5, "sellRatio": 0.25 },
      { "percent": 15, "sellRatio": 0.35 },
      { "percent": 30, "sellRatio": 0.40 }
    ]
  },
  "stopLoss": {
    "percent": -25
  },
  "trailingStop": {
    "activatePercent": 8,
    "trailPercent": 12
  },
  "quickStop": {
    "minutes": 5,
    "maxLossPercent": -8
  },
  "timeStop": {
    "maxHoldHours": 6
  },
  "monitoring": {
    "positionCheckInterval": 10000,
    "healthCheckInterval": 300000
  }
}
```

## LLM 使用策略

### 需要 LLM 的环节

| 环节 | 原因 |
|------|------|
| 策略初始化 / 参数调优 | 分析历史数据，建议阈值调整 |
| 异常诊断 | 分析连续亏损原因 |
| 项目搭建 | 根据本 SKILL.md 生成完整代码 |

### 不需要 LLM 的环节

本策略运行时 **100% 纯规则驱动**，以下环节均不需要 LLM：
- 排行榜数据获取 → MCP 调用
- Slot Guard 过滤 → 数值比较
- Advanced Safety Check → MCP 调用 + 数值比较
- Momentum Score 计算 → 数学公式
- 买卖决策 → 规则引擎
- 交易执行 → Solana 链上签名提交
- 仓位监控 → 定时轮询 + 价格比较

## 错误处理规范

| 错误场景 | 处理方式 | 重试策略 |
|---------|---------|---------|
| MCP 请求失败（网络/超时） | 记录 ERROR 日志，跳过本轮 | 下一个 `pollInterval` 自动重试 |
| MCP 返回 `Insufficient liquidity` | 跳过该代币 | 不重试 |
| `advanced-info` 请求失败 | 该代币跳过安全检查，**不买入**（安全第一） | 下次新上榜时重试 |
| `token-holder` 请求失败 | 该代币跳过持有者风险检查，**不买入**（安全第一） | 下次新上榜时重试 |
| Solana RPC 429（Rate Limit） | 增加请求间隔 | Live 模式买入间隔 2s |
| WSOL prepareSwap 失败 | 记录日志，本次买入取消 | 下一个信号到来时重试 |
| 交易确认超时 | 发出 WARN 日志（交易可能仍成功） | 不主动重试 |
| 交易链上失败 | 返回 `success: false`，不建仓 | 不重试 |
| 买入价格为 0 | 跳过建仓，避免 PnL 计算为 Infinity | 不重试 |
| 持仓价格查询失败 | 跳过本轮仓位检查 | 下一轮自动重试 |

## 故障排除指南

当用户遇到问题时，AI 应按以下流程诊断。用户可能完全不了解策略内部机制，AI 需要**主动检查并给出明确结论**，不要让用户自己看日志。

### 诊断步骤

```
1. 检查进程是否存活
   → curl http://127.0.0.1:${PORT}/health
   → 如果无响应：检查 node 进程是否在运行，查看启动日志

2. 检查引擎状态
   → curl http://127.0.0.1:${PORT}/api/status
   → running=false：引擎未启动，提示用户点 Start 或调用 /api/start
   → mode 是否正确

3. 检查最近日志
   → curl http://127.0.0.1:${PORT}/api/logs?n=20
   → 关注 ERROR 和 WARN 类型日志

4. 根据日志类型定位问题（见下方常见问题表）
```

### 常见问题速查

| 现象 | 可能原因 | 解决方法 |
|------|---------|---------|
| 引擎启动但**一直没有买入** | 风控参数太严，所有代币都被过滤 | 查看日志中 SKIP/SAFETY_REJECT 的原因，适当放宽对应阈值（如 `minLiquidity`、`maxRiskLevel`、`minBuyRatio`） |
| 日志中大量 `SAFETY_REJECT` | 二级安全检查拦截（常见：`RiskLevel:2`、`Bundle:XX%`、`Internal`） | 根据拒绝原因调整 `safety` 配置。如果是 `Internal`（PumpFun 内盘），可设 `blockInternal: false` |
| 日志中出现 `MCP [100]: xxx` | OKX API Key 无效或过期 | 检查 `.env` 中 `OKX_API_KEY` 是否正确，前往 OKX Developer Portal 确认 Key 状态 |
| `Empty ranking` 不断出现 | MCP 排行榜接口返回空数据 | 检查网络连接；确认 API Key 有效；可能是 MCP 服务暂时不可用 |
| Live 模式**买入失败** | WSOL 准备或交易签名失败 | 检查 `.env` 中 `SOLANA_PRIVATE_KEY` 是否正确；检查钱包 SOL 余额是否充足（至少 `buyAmountPerTrade + 0.01`） |
| `SOLANA_PRIVATE_KEY not set` | Live 模式缺少私钥 | 在 `.env` 中配置 base58 格式的 Solana 私钥 |
| 买了但**PnL 显示异常**（Infinity/NaN） | 买入价格获取失败 | 应该不会出现（代码有三级回退 + price=0 跳过），如果出现请检查 `state/positions.json` 中 `buyPrice` 字段 |
| Dashboard 打开**空白/无数据** | 前端未启动或代理配置错误 | 确认前端 dev server 在运行；检查 `vite.config.js` 中 proxy 指向的后端端口是否正确 |
| 引擎**频繁买入同一代币** | 冷却期太短或 `maxSingleTokenBuys` 配置不当 | 增大 `cooldownMinutes`（默认 30min），确认 `maxSingleTokenBuys=1` |
| 日亏损达到上限后**不再买入** | `dailyLossLimitRatio` 触发 | 正常行为。日志中会有 `DailyLoss` 提示。次日自动恢复，或调整 `dailyLossLimitRatio` |

### AI 诊断提示词

当用户说"出错了"、"不工作了"、"为什么没有买入"等模糊描述时，AI 应该：

1. **先读日志**：`curl http://127.0.0.1:${PORT}/api/logs?n=30`
2. **再看状态**：`curl http://127.0.0.1:${PORT}/api/status`
3. **然后看持仓**：`curl http://127.0.0.1:${PORT}/api/positions`
4. **根据上表给出结论**，用用户能理解的语言解释
5. **如果需要改参数**，直接帮用户改 `state/config.json` 并重启引擎

## Dashboard 功能

### 模拟盘 / 实盘切换

Dashboard 顶部提供**模拟盘**和**实盘**切换按钮：

- **模拟盘（Paper）**：默认模式，不需要私钥，使用 DEX Quote 模拟交易
- **实盘（Live）**：真实链上交易，需要在 `.env` 中配置 `SOLANA_PRIVATE_KEY`

切换规则：
- 必须先**停止引擎**才能切换模式
- 切换到实盘时，后端会验证 `SOLANA_PRIVATE_KEY` 是否已配置
- 切换成功后配置自动保存到 `state/config.json`

API：`POST /api/mode` — Body: `{ "mode": "paper" | "live" }`

### 模拟盘 / 实盘数据隔离

模拟盘和实盘的运行数据**完全隔离**，互不影响：

```
state/
├── config.json          # 共享配置（含 mode 字段）
├── paper/               # 模拟盘数据
│   ├── positions.json
│   ├── trades.json
│   ├── daily-stats.json
│   ├── signals-log.json
│   └── signals-seen.json
└── live/                # 实盘数据
    ├── positions.json
    ├── trades.json
    ├── daily-stats.json
    ├── signals-log.json
    └── signals-seen.json
```

- `config.json` 在根目录共享，切换模式只改 `mode` 字段
- 所有 API（`/api/positions`、`/api/trades`、`/api/logs`）自动返回当前模式的数据
- 切换模式后 Dashboard 自动显示对应模式的持仓、交易和日志

### 数据重置

Dashboard 顶部提供**重置**按钮，用于清空**当前模式**的策略运行数据：

- 清空持仓（`<mode>/positions.json`）
- 清空交易记录（`<mode>/trades.json`）
- 清空每日统计（`<mode>/daily-stats.json`）
- 清空信号日志（`<mode>/signals-log.json`）
- 清空已见代币缓存（`<mode>/signals-seen.json`）

使用规则：
- 必须先**停止引擎**才能重置
- 重置前会弹出确认对话框
- 重置只影响当前模式的数据，不影响另一个模式
- 重置不影响 `config.json` 配置

API：`POST /api/reset`

### 实盘钱包信息

实盘模式下，Dashboard 顶部右侧会显示：

- **钱包地址缩写**：`xxxx...xxxx`（前 4 位 + 后 4 位）
- **SOL 实时余额**：每 3 秒自动刷新

显示条件：
- `mode` 为 `live` 且 `.env` 中已配置 `SOLANA_PRIVATE_KEY`
- 模拟盘模式下不显示

数据来源：`GET /api/status` 在 live 模式下额外返回 `wallet`（地址）和 `solBalance`（余额）字段

### 停止引擎自动清仓（StopExit）

停止引擎时，**自动卖出所有当前持仓**：

- 获取所有持仓的当前价格（`dex-okx-index-current-price`）
- 逐个执行全额卖出（ratio=1），退出原因为 `StopExit`
- **实盘模式**：调用 `dex-okx-dex-swap` 获取卖出交易 → 签名 → 发送链上
- **模拟盘模式**：调用 `dex-okx-dex-quote` 记录虚拟卖出
- 所有卖出完成后清空持仓列表
- PnL 和交易记录正常写入

这确保无论模拟盘还是实盘，停止时不会遗留未平仓的位置。

### PnL 显示精度

Dashboard 统计卡片中 PnL 金额显示规则：
- **百分比**（`%`）：固定 2 位小数
- **SOL 金额**：绝对值 < 0.01 SOL 时显示 4 位小数（如 `-0.0014 SOL`），否则 2 位小数

避免小额 PnL 被四舍五入显示为 `+0.00 SOL` / `-0.00 SOL`。

## 交易纪律规则

- **RULE-1**: 日亏损 > `totalBudget * dailyLossLimitRatio`（用户初始化时设定）时，**停止所有新买入**
- **RULE-2**: 同一代币卖出后 `cooldownMinutes`（30 分钟）内**不可重复买入**
- **RULE-3**: 同时持仓不超过 `maxPositions`（默认 5 个）
- **RULE-4**: 每笔交易金额固定 `buyAmountPerTrade`，**不追加仓位**（`maxSingleTokenBuys=1`）
- **RULE-5**: 排名退出（EXIT 0）是最高优先级——代币跌出 Top N 即全部卖出，不等其他止盈止损
- **RULE-6**: Live 模式必须配置 `SOLANA_PRIVATE_KEY`，否则引擎拒绝启动
- **RULE-7**: 永远保留 `gasReserve`（0.01 SOL）用于交易手续费
- **RULE-8**: SOL 余额低于 `minWalletBalance`（0.1 SOL）时停止买入
- **RULE-9**: `advanced-info` 或 `token-holder` 安全检查失败的代币**绝不买入**，即使其他指标再好
- **RULE-10**: 任何安全检查接口请求失败时，该代币**视为不安全**，不买入（Fail-Closed 原则）

## 代码安全规范

### 私钥安全

| 规则 | 说明 |
|------|------|
| **私钥仅从 `.env` 读取** | 通过 `dotenv` 加载，**绝不硬编码**在代码中 |
| **`.env` 必须加入 `.gitignore`** | 项目初始化时自动创建 `.gitignore`，包含 `.env`、`state/`、`node_modules/` |
| **私钥不出现在日志中** | `console.log` / `log()` 中**禁止打印**私钥、API Key 等敏感信息 |
| **私钥不出现在状态文件中** | `state/` 目录下的 JSON 文件**不存储**任何密钥 |
| **私钥不出现在 API 响应中** | `/api/*` 路由的返回值**不包含**私钥或 API Key |
| **私钥不出现在错误信息中** | `catch` 块中的错误日志**不附带**密钥上下文 |
| **Keypair 对象仅在 swap-executor 内部使用** | 签名操作封装在 `swap-executor.mjs` 中，不暴露到其他模块 |

### 私钥处理代码规范

```javascript
// ✅ 正确：从环境变量加载，仅在需要签名时使用
import { Keypair } from '@solana/web3.js';
import bs58 from 'bs58';

let _keypair = null;
function getKeypair() {
  if (!_keypair) {
    const pk = process.env.SOLANA_PRIVATE_KEY;
    if (!pk) throw new Error('SOLANA_PRIVATE_KEY not set');
    _keypair = Keypair.fromSecretKey(bs58.decode(pk));
  }
  return _keypair;
}

// ✅ 正确：只暴露公钥地址
export function getWalletAddress() {
  return getKeypair().publicKey.toBase58();
}

// ✅ 正确：签名操作封装在内部
export async function signAndSend(transaction) {
  transaction.sign(getKeypair());
  // ...send
}

// ❌ 错误：绝不做以下操作
// console.log('Private key:', process.env.SOLANA_PRIVATE_KEY);
// return { keypair: getKeypair(), ... };
// state.save({ privateKey: pk });
```

### API Key 安全

| 规则 | 说明 |
|------|------|
| `OKX_API_KEY` 仅通过 `.env` 配置 | 不硬编码在代码中 |
| API Key 仅在 HTTP Header 中传输 | 通过 `OK-ACCESS-KEY` header，不放在 URL query string 中 |
| API Key 不记录在日志中 | MCP 调用失败时只记录错误码和消息，不记录 header |

### 输入校验

| 检查点 | 规则 |
|--------|------|
| 用户输入的 `totalBudget` | 必须为正数，范围 1-100 SOL |
| 用户输入的 `buyAmountPerTrade` | 必须为正数，范围 0.01-1 SOL，且 ≤ totalBudget |
| 用户输入的 `mode` | 只允许 `"paper"` 或 `"live"` |
| 用户输入的 `myWallet` | Live 模式必须是有效的 Solana base58 地址（32-44 字符） |
| 配置文件 JSON | 启动时校验 schema，缺少必填字段则拒绝启动并报错 |
| MCP 返回的代币地址 | 必须是非空字符串，32-44 字符的 base58 格式 |
| MCP 返回的数值字段 | `parseFloat` 前检查非 null/undefined，NaN 视为 0 |

### 运行时安全

| 规则 | 说明 |
|------|------|
| **并发控制** | `buyingNow` Set 防止同一代币并发买入；`setInterval` 保证单线程执行 |
| **重入保护** | `pollRanking` 和 `monitorPositions` 内部设 `isRunning` 标志，前一轮未完成不启动下一轮 |
| **超时控制** | 所有 MCP 调用设 `AbortSignal.timeout(15000)`，防止网络挂起 |
| **内存限制** | 日志数组最多保留 200 条；信号日志最多 100 条；冷却 Map 定期清理过期条目 |
| **状态文件原子写入** | 写入 JSON 时先写 `.tmp` 文件再 `rename`，防止写入中途崩溃导致文件损坏 |
| **交易金额上限** | 单笔买入不超过 `buyAmountPerTrade`（配置值），即使代码 bug 也不会超额交易 |
| **SOL 余额保护** | 每次买入前检查余额 ≥ `minWalletBalance`（0.1 SOL），保留 `gasReserve`（0.01 SOL） |
| **防止价格为 0 建仓** | 买入价格三级回退全部失败时（price ≤ 0），跳过建仓，避免 PnL 计算 Infinity/NaN |

### .gitignore 模板

项目初始化时必须创建：

```gitignore
# 环境变量（含私钥和 API Key）
.env
.env.*

# 运行时状态（含交易记录和持仓数据）
state/

# 依赖
node_modules/

# 系统文件
.DS_Store
*.log
```

### 网络安全

| 规则 | 说明 |
|------|------|
| **仅 HTTPS** | MCP 调用使用 `https://web3.okx.com/api/v1/plugin-store-mcp`，不允许 HTTP |
| **不信任外部输入** | MCP 返回的代币数据全部做类型检查和范围校验 |
| **Dashboard 仅监听 localhost** | Express 服务默认绑定 `127.0.0.1:${PORT}`，不暴露到公网 |
| **无 CORS 通配符** | 如需跨域访问，只允许 `http://localhost:*`，不用 `*` |

### 风控检查完整清单总览

共 **25 项风控检查**，分三级执行：

**一级 Slot Guard（13 项，基于 ranking 数据，0 额外 API 调用）：**
1. 涨幅下限 ≥ 5%
2. 涨幅上限 ≤ 5000%
3. 流动性 ≥ $3,000
4. 市值下限 ≥ $3,000
5. 市值上限 ≤ $500,000,000
6. 持有者数 ≥ 10
7. 买入比 ≥ 40%
8. 独立交易者 ≥ 10
9. 不在黑名单
10. 冷却期 ≥ 5 分钟
11. 仓位数 < maxPositions
12. 未持有该代币
13. 日亏损 < 15%

**二级 Advanced Safety Check（9 项，调用 `advanced-info` 1 次）：**
14. 风控等级 ≤ 3
15. 无蜜罐标签
16. Top10 集中度 ≤ 80%
17. 开发者持仓 ≤ 50%
18. Bundler 持仓 ≤ 30%
19. LP 销毁 ≥ 0%（不强制要求）
20. 开发者 Rug 历史 ≤ 20
21. 狙击手持仓 ≤ 30%
22. 允许内盘（blockInternal = false）

**三级 Holder Risk Scan（3 项，调用 `token-holder` 2 次）：**
23. 可疑地址持仓占比 ≤ 50%
24. 钓鱼地址检查关闭（blockPhishingHolder = false）
25. 可疑地址数 ≤ 20

## 术语表

| 术语 | 定义 |
|------|------|
| **Ranking Snapshot** | 每次轮询 Token Ranking API 返回的 Top N 代币列表快照 |
| **New Entry** | 当前快照中出现、上一次快照中不存在的代币（差集） |
| **Slot Guard** | 一级过滤：基于 ranking 基础指标的 13 项前置条件检查 |
| **Advanced Safety Check** | 二级过滤：基于 `advanced-info` 的 9 项安全审查（蜜罐/dev rug/sniper 等） |
| **Holder Risk Scan** | 三级过滤：基于 `token-holder` 检测可疑/钓鱼地址持仓 |
| **Momentum Score** | 0-125 的综合动量评分（基础 0-100 + 安全加分 0-25） |
| **Ranking Exit** | 代币不再出现在 Top N 排行榜时触发的最高优先级退出机制 |
| **WSOL** | Wrapped SOL，Solana DEX 交易中 SOL 的 SPL Token 包装形式 |
| **ATA** | Associated Token Account，Solana 钱包对某 Token 的标准关联账户地址 |
| **Quick Stop** | 短时间内未涨反跌时的快速止损（默认 5min / -8%） |
| **Trailing Stop** | 利润达到激活阈值后，从历史峰值回撤超阈值时触发卖出 |
| **Paper Mode** | 模拟交易模式，不发送链上交易，用 DEX Quote 模拟真实摩擦 |
| **Live Mode** | 实盘模式，通过 Solana 链上交易实际执行买卖 |
| **riskControlLevel** | OKX 代币综合风控等级：1=低风险 2=中风险 3=高风险 |
| **devRugPullTokenCount** | 代币开发者历史创建的 rug pull 代币数量 |
| **sniperHoldingPercent** | 狙击手（机器人在代币上线瞬间抢购）的持仓占比 |
| **isInternal** | PumpFun 代币是否仍在内盘（bonding curve 未毕业），流动性受限 |
