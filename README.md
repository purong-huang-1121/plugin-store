# plugin-store

链上 DeFi 智能体技能包，集成主流协议操作与自动化交易策略。支持 Claude Code、OpenClaw 等 AI 编程助手。

Skills Store 收录了 Web3 Team 基于 Onchain OS 构建的上层策略 Skill，安装后可查看所有已上线的策略，选择你需要的策略安装并运行。

- 策略使用过程中如有体验反馈，可联系策略作者交流
- 如果你也想分享自己的策略，欢迎联系 @甘明涛 Sheldon Gan / @Ocean Fan 范海洋

## 包含技能

### 核心技能（dApp 协议集成）

`plugin-store` 是主入口技能，整合以下所有协议能力：

| 协议 | 功能 |
|------|------|
| **Aave V3** | 查看市场、账户信息、存款、取款、借款、还款 |
| **Morpho Blue** | 市场列表、MetaMorpho 金库、存取款、持仓查询 |
| **Uniswap V3** | 链上代币兑换、报价、Token 搜索 |
| **Hyperliquid** | 永续合约/现货交易、资金费率、仓位管理 |
| **Ethena** | sUSDe 质押/赎回、APY 查询、余额 |
| **Polymarket** | 预测市场搜索、报价、买卖份额 |
| **Kalshi** | 美国合规预测市场、活动/市场浏览、交易 |
| **dapp-composer** | 跨协议组合操作、多步骤 DeFi 工作流 |

### 自动化策略技能

| 技能 | 子命令 | 描述 |
|------|--------|------|
| `strategy-auto-rebalance` | `auto-rebalance` | USDC 跨协议（Aave/Compound/Morpho）自动调仓，Base/Ethereum |
| `strategy-grid-trade` | `grid` | ETH/USDC 网格交易机器人，Base 链 |
| `strategy-ranking-sniper` | `ranking-sniper` | SOL 排行榜狙击策略，3 层安全过滤 + 6 层出场体系 |
| `strategy-signal-tracker` | `signal-tracker` | 聪明钱/KOL/巨鲸信号跟单，17 点安全过滤 |
| `strategy-memepump-scanner` | `scanner` | Pump.fun 迁移代币自动扫描交易，3 信号动量检测 |

## 安装指南

### 前提条件

首先安装 Agentic Wallet，参考 [Agentic Wallet 内测参与流程](https://okg-block.sg.larksuite.com/docx/Cx7PdgNHLogZWIxGlwslfacIgl1)。

### 方式一：使用 Claude Code CLI 安装（推荐）

复制以下命令在终端中运行，安装过程中可选配置 Telegram 机器人 Token 和 Chat ID 以接收通知：

```bash
curl -sSL https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/reinstall.sh -o /tmp/reinstall.sh && sh /tmp/reinstall.sh
```

安装完成后，在终端中运行 Claude（跳过权限检查）：

```bash
claude --dangerously-skip-permissions
```

> 如不想跳过权限检查，直接输入 `claude` 即可。

启动后可用自然语言询问已安装的技能，例如"你有什么技能？"，然后选择你需要的策略运行。

### 方式二：使用 OpenClaw 安装

直接将以下命令发送给 Agent，它会自动完成安装：

```bash
curl -sSL https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/reinstall.sh -o /tmp/reinstall.sh && sh /tmp/reinstall.sh
```

### 方式三：npx 安装

```bash
npx skills add plugin-store
```

支持 Claude Code、OpenClaw、Cursor、Codex CLI 等环境，自动检测安装路径。

### 方式四：Shell 脚本安装 CLI（macOS / Linux）

```bash
curl -sSL https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/install.sh | sh
```

自动检测平台，下载对应二进制，验证 SHA256，安装至 `~/.cargo/bin/plugin-store`。

## 配置

### 钱包授权（SOL/EVM 策略必须）

使用 onchainos 钱包登录（支持 EVM 和 Solana 链签名）：

```bash
onchainos wallet login
```

### Telegram 通知（可选）

```env
TELEGRAM_BOT_TOKEN="your-bot-token"
TELEGRAM_CHAT_ID="your-chat-id"
```

> **Q: 如何配置 Telegram Bot？**
> A: 需要在环境变量中配置 `TELEGRAM_BOT_TOKEN` 和 `TELEGRAM_CHAT_ID`，如何获取可以直接问 Agent。

## 支持链

Solana、Ethereum、Base、BSC、Arbitrum、Polygon、XLayer 及 20+ 其他链。

## 使用示例

**查询 Aave 市场利率**
> "查一下 Aave 上 USDC 的供款利率"

**开启排行榜狙击策略**
> "帮我启动 SOL 排行榜狙击"

**查看策略钱包余额**
> `plugin-store ranking-sniper balance`

**网格交易**
> "帮我在 Base 上开一个 ETH/USDC 网格策略"

**聪明钱跟单**
> "帮我启动聪明钱信号跟单策略"

## 免责声明

- 内置公共 API Key 仅供测试评估，可能随时限速或不可用，由此产生的任何损失概不负责
- 自动化交易策略涉及真实资产，请充分了解风险后使用
- 生产环境请务必使用自己的 API Key 和钱包私钥，并妥善保管

## License

Apache-2.0
