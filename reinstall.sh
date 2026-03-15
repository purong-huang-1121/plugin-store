#!/bin/sh
# reinstall.sh — 清理并重新安装 skills-store skill
# Usage: sh reinstall.sh

set -e

echo "=== Step 1: 删除 skills-store 二进制 ==="

# 在常见目录和 PATH 中查找并删除
for dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /usr/local/bin /usr/bin; do
  if [ -f "$dir/skills-store" ]; then
    echo "删除 $dir/skills-store"
    rm -f "$dir/skills-store"
  fi
done

# 同时清理 which 找到的（可能在其他位置）
found=$(which skills-store 2>/dev/null || true)
if [ -n "$found" ]; then
  echo "删除 $found"
  rm -f "$found"
fi

# 清理缓存目录
rm -rf "$HOME/.cargo/bin/.skills-store" 2>/dev/null || true
rm -rf "$HOME/.local/bin/.skills-store" 2>/dev/null || true

echo "✅ skills-store 二进制已清理"

echo ""
echo "=== Step 2: 删除已安装的 skills ==="

SKILL_DIRS="$HOME/.claude/skills $HOME/.agents/skills"

for skill_base in $SKILL_DIRS; do
  if [ -d "$skill_base" ]; then
    echo "清理 $skill_base ..."
    rm -rf "$skill_base"
    echo "✅ $skill_base 已删除"
  else
    echo "（跳过，不存在：$skill_base）"
  fi
done

echo ""
echo "=== Step 3: 检查 curl ==="

if command -v curl >/dev/null 2>&1; then
  echo "✅ curl 已安装：$(curl --version | head -1)"
else
  echo "❌ curl 未安装，尝试安装..."
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get install -y curl
  elif command -v brew >/dev/null 2>&1; then
    brew install curl
  else
    echo "无法自动安装 curl，请手动安装后重新运行此脚本" >&2
    exit 1
  fi
  echo "✅ curl 安装完成"
fi

echo ""
echo "=== Step 4: 检查 npx ==="

if command -v npx >/dev/null 2>&1; then
  echo "✅ npx 已安装：$(npx --version)"
else
  echo "❌ npx 未安装（需要 Node.js），尝试安装..."
  if command -v brew >/dev/null 2>&1; then
    brew install node
    # 将 Homebrew bin 加入当前会话 PATH（Apple Silicon: /opt/homebrew/bin，Intel: /usr/local/bin）
    BREW_PREFIX=$(brew --prefix 2>/dev/null || echo "/usr/local")
    export PATH="$BREW_PREFIX/bin:$PATH"
  elif command -v apt-get >/dev/null 2>&1; then
    sudo apt-get install -y nodejs npm
  else
    echo "无法自动安装 npx，请先安装 Node.js：https://nodejs.org" >&2
    exit 1
  fi
  # 验证 npx 现在可用
  if ! command -v npx >/dev/null 2>&1; then
    echo "安装完成但 npx 仍未找到，请手动运行：" >&2
    echo "  export PATH=\"\$(brew --prefix)/bin:\$PATH\"" >&2
    echo "然后重新运行此脚本" >&2
    exit 1
  fi
  echo "✅ npx 安装完成：$(npx --version)"
fi

echo ""
echo "=== Step 5: 安装 skills-store skill ==="

npx skills add purong-huang-1121/skills-store --skill skills-store --yes
SKILLS_EXIT=$?

if [ $SKILLS_EXIT -ne 0 ]; then
  echo "❌ skills add 命令失败（exit $SKILLS_EXIT），请检查网络后重试" >&2
  exit 1
fi

# 验证 skill 文件确实已写入磁盘
SKILL_FILE_1="$HOME/.agents/skills/skills-store/SKILL.md"
SKILL_FILE_2="$HOME/.claude/skills/skills-store/SKILL.md"
if [ ! -f "$SKILL_FILE_1" ] && [ ! -f "$SKILL_FILE_2" ]; then
  echo "⚠️  skill 文件未找到，等待写入..."
  sleep 2
  if [ ! -f "$SKILL_FILE_1" ] && [ ! -f "$SKILL_FILE_2" ]; then
    echo "❌ skill 安装可能未完成，请重新运行脚本" >&2
    exit 1
  fi
fi

echo "✅ skills-store skill 安装完成"

echo ""
echo "=== Step 6: 安装 skills-store 二进制 ==="

curl -sSL https://raw.githubusercontent.com/purong-huang-1121/skills-store/main/install.sh | sh
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v skills-store >/dev/null 2>&1; then
  echo "❌ skills-store 二进制安装失败，请检查网络后重试" >&2
  exit 1
fi
echo "✅ skills-store $(skills-store --version 2>/dev/null | awk '{print $2}') 安装完成"

echo ""
echo "=== Step 7: 配置 .env 环境变量 ==="

ENV_FILE="$HOME/.cargo/bin/.env"
mkdir -p "$HOME/.cargo/bin"

# 如果 .env 不存在，创建模板
if [ ! -f "$ENV_FILE" ]; then
  cat > "$ENV_FILE" <<'EOF'
# skills-store 环境变量配置
# 填写后保存退出即可

# ── EVM 钱包私钥（Aave / Morpho / Grid Trading / Auto-Rebalance 必填）──
EVM_PRIVATE_KEY=

# ── OKX API（Grid Trading / Ranking Sniper / Signal Tracker / Memepump 必填）──
OKX_API_KEY=
OKX_SECRET_KEY=
OKX_PASSPHRASE=

# ── Solana 钱包私钥（Ranking Sniper / Signal Tracker / Memepump 必填）──
SOLANA_PRIVATE_KEY=

# ── Telegram 通知（可选，所有策略支持）──
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
EOF
  echo "✅ 已创建 $ENV_FILE"
else
  echo "（$ENV_FILE 已存在，跳过创建）"
fi

# 打开编辑器（优先图形界面，对小白更友好）
echo "正在打开编辑器，请填写需要的环境变量后保存退出..."
if command -v code >/dev/null 2>&1; then
  # VS Code — 等待用户关闭文件
  code --wait "$ENV_FILE"
elif [ "$(uname -s)" = "Darwin" ]; then
  # macOS：用 TextEdit 打开，等待用户关闭
  open -e "$ENV_FILE"
  echo ""
  echo "已用 TextEdit 打开 $ENV_FILE"
  echo "请填写环境变量后保存，然后按回车继续..."
  read -r _
elif command -v xdg-open >/dev/null 2>&1; then
  # Linux 桌面环境
  xdg-open "$ENV_FILE"
  echo ""
  echo "已打开 $ENV_FILE，请填写环境变量后保存，然后按回车继续..."
  read -r _
else
  echo ""
  echo "请手动编辑以下文件，填写环境变量后再继续："
  echo "  $ENV_FILE"
  echo ""
  echo "填写完成后按回车继续..."
  read -r _
fi

echo ""
echo "✅ 全部完成！重新开始一个新对话即可使用 skills-store。"
