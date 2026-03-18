#!/bin/sh
# reinstall.sh — 清理并重新安装 plugin-store skill
# Usage: curl -sSL https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/reinstall.sh -o /tmp/reinstall.sh && sh /tmp/reinstall.sh

set -e

echo "=== Step 1: 删除 plugin-store 二进制 ==="

# 在常见目录和 PATH 中查找并删除
for dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /usr/local/bin /usr/bin; do
  if [ -f "$dir/plugin-store" ]; then
    echo "删除 $dir/plugin-store"
    rm -f "$dir/plugin-store"
  fi
done

# 同时清理 which 找到的（可能在其他位置）
found=$(which plugin-store 2>/dev/null || true)
if [ -n "$found" ]; then
  echo "删除 $found"
  rm -f "$found"
fi

# 清理缓存目录
rm -rf "$HOME/.cargo/bin/.plugin-store" 2>/dev/null || true
rm -rf "$HOME/.local/bin/.plugin-store" 2>/dev/null || true

echo "✅ plugin-store 二进制已清理"

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
echo "=== Step 5: 安装 plugin-store skill ==="

npx skills add purong-huang-1121/plugin-store --skill plugin-store --yes
SKILLS_EXIT=$?

if [ $SKILLS_EXIT -ne 0 ]; then
  echo "❌ skills add 命令失败（exit $SKILLS_EXIT），请检查网络后重试" >&2
  exit 1
fi

# 验证 skill 文件确实已写入磁盘
SKILL_FILE_1="$HOME/.agents/skills/plugin-store/SKILL.md"
SKILL_FILE_2="$HOME/.claude/skills/plugin-store/SKILL.md"
if [ ! -f "$SKILL_FILE_1" ] && [ ! -f "$SKILL_FILE_2" ]; then
  echo "⚠️  skill 文件未找到，等待写入..."
  sleep 2
  if [ ! -f "$SKILL_FILE_1" ] && [ ! -f "$SKILL_FILE_2" ]; then
    echo "❌ skill 安装可能未完成，请重新运行脚本" >&2
    exit 1
  fi
fi

echo "✅ plugin-store skill 安装完成"

echo ""
echo "=== Step 6: 安装 plugin-store 二进制 ==="

curl -sSL https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/install.sh | sh
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v plugin-store >/dev/null 2>&1; then
  echo "❌ plugin-store 二进制安装失败，请检查网络后重试" >&2
  exit 1
fi
echo "✅ plugin-store $(plugin-store --version 2>/dev/null | awk '{print $2}') 安装完成"

echo ""
echo "=== Step 7: 检查 onchainos CLI ==="

ONCHAINOS_MIN_VERSION="1.0.5"

if command -v onchainos >/dev/null 2>&1; then
  ONCHAINOS_VERSION=$(onchainos --version 2>/dev/null | awk '{print $2}')

  # Compare versions: check if installed >= minimum
  printf '%s\n%s' "$ONCHAINOS_MIN_VERSION" "$ONCHAINOS_VERSION" | sort -V -C
  if [ $? -eq 0 ]; then
    echo "✅ onchainos $ONCHAINOS_VERSION 已安装 (>= $ONCHAINOS_MIN_VERSION)"
  else
    echo "⚠️  onchainos 版本过低 ($ONCHAINOS_VERSION < $ONCHAINOS_MIN_VERSION)"
    echo "请按照文档升级: https://okg-block.sg.larksuite.com/docx/Cx7PdgNHLogZWIxGlwslfacIgl1"
  fi
else
  echo "⚠️  onchainos CLI 未安装"
  echo ""
  echo "plugin-store 需要 onchainos CLI (>= $ONCHAINOS_MIN_VERSION) 来进行链上签名和交易。"
  echo "请按照以下文档安装:"
  echo ""
  echo "  https://okg-block.sg.larksuite.com/docx/Cx7PdgNHLogZWIxGlwslfacIgl1"
  echo ""
fi

echo ""
echo "=== Step 8: 配置 Telegram 通知（可选） ==="

ENV_FILE="$HOME/.cargo/bin/.env"
mkdir -p "$HOME/.cargo/bin"

if [ ! -f "$ENV_FILE" ]; then
  cat > "$ENV_FILE" <<'EOF'
# plugin-store 通知配置
# 配置 Telegram 机器人后，策略运行时会实时推送交易通知
# 不需要可留空

# Telegram 通知（所有策略支持）
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
EOF
  echo "✅ 已创建 $ENV_FILE"
else
  echo "（$ENV_FILE 已存在，直接打开编辑）"
fi

echo "配置 Telegram 机器人后，策略运行时会及时推送交易通知。"
echo "正在打开编辑器，填写完成后保存关闭，脚本自动继续..."
if command -v code >/dev/null 2>&1; then
  code --wait "$ENV_FILE"
elif [ "$(uname -s)" = "Darwin" ]; then
  open -e "$ENV_FILE"
  echo "请在 TextEdit 中填写完成后保存关闭，然后按回车继续..."
  read -r _
else
  open "$ENV_FILE" 2>/dev/null || xdg-open "$ENV_FILE" 2>/dev/null || true
  echo "请填写完成后保存关闭，然后按回车继续..."
  read -r _
fi

echo ""
echo "✅ 全部完成！重新开始一个新对话即可使用 plugin-store。"
