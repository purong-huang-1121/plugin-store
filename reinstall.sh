#!/bin/sh
# reset.sh — 清理并重新安装 skills-store skill
# Usage: sh reset.sh

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

echo ""
echo "✅ 全部完成！重新开始一个新对话即可使用 skills-store。"
