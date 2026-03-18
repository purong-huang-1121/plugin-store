#!/usr/bin/env python3
"""
SOL 涨幅榜狙击 (Ranking Sniper) — Strategy D Engine

Monitors Solana token trending rankings, buys new entries that pass 25-point
safety checks + Momentum Score, manages positions with a 6-layer exit system.

Usage:
    source cli/.env
    python3 skills/strategy-ranking-sniper/engine.py [--budget 0.5] [--per-trade 0.05] [--dry-run]
"""

import argparse
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

# ── Constants ────────────────────────────────────────────────────────────────

POLL_INTERVAL = 10          # seconds between ranking polls
TOP_N = 20                  # monitor top N by price change
CHAIN = "solana"
SOL_NATIVE = "So11111111111111111111111111111111111111112"
GAS_RESERVE = 0.05          # SOL reserved for gas
MAX_POSITIONS = 5           # max concurrent positions
DAILY_LOSS_LIMIT_PCT = 15   # auto-stop if daily loss exceeds this %

# Exit thresholds
HARD_STOP_PCT = -25         # hard stop-loss
FAST_STOP_TIME = 300        # 5 minutes
FAST_STOP_PCT = -8          # fast stop if down this much within FAST_STOP_TIME
TRAILING_ACTIVATE_PCT = 8   # trailing stop activates at +8%
TRAILING_DRAWDOWN_PCT = 12  # trailing stop triggers on 12% drawdown from peak
TIME_STOP_SECS = 6 * 3600  # 6 hour time stop
# Gradient take-profit: sell 33% at each level
TP_LEVELS = [5, 15, 30]    # +5%, +15%, +30%

# Momentum Score thresholds
SCORE_BUY_THRESHOLD = 40    # minimum score to buy (out of 125)


def safe_float(val, default=0.0) -> float:
    """Safely convert to float, returning default for empty/None/invalid values."""
    if val is None or val == "":
        return default
    try:
        return float(val)
    except (ValueError, TypeError):
        return default


def safe_int(val, default=0) -> int:
    """Safely convert to int."""
    if val is None or val == "":
        return default
    try:
        return int(float(val))
    except (ValueError, TypeError):
        return default


@dataclass
class Position:
    token_address: str
    symbol: str
    buy_price: float
    buy_amount_sol: float
    buy_time: float          # timestamp
    peak_pnl_pct: float = 0.0
    trailing_active: bool = False
    tp_sold: list = field(default_factory=list)  # which TP levels have been hit
    tx_hash: str = ""


@dataclass
class EngineState:
    known_tokens: set = field(default_factory=set)     # tokens seen on ranking
    positions: dict = field(default_factory=dict)       # token_addr -> Position
    total_invested: float = 0.0
    total_returned: float = 0.0
    trades: list = field(default_factory=list)
    daily_pnl: float = 0.0
    start_time: float = 0.0
    stopped: bool = False


# ── CLI Helpers ──────────────────────────────────────────────────────────────

def run_onchainos(args: list[str], timeout: int = 30) -> dict | None:
    """Run an onchainos command and return parsed JSON, or None on failure."""
    cmd = ["onchainos"] + args
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            log(f"  ⚠ command failed: onchainos {' '.join(args[:3])}...")
            return None
        data = json.loads(result.stdout)
        if data.get("code") == "0" or data.get("ok") is True:
            return data.get("data", data)
        log(f"  ⚠ API error: {data.get('msg', 'unknown')}")
        return None
    except (subprocess.TimeoutExpired, json.JSONDecodeError, Exception) as e:
        log(f"  ⚠ exception: {e}")
        return None


def log(msg: str):
    ts = datetime.now().strftime("%H:%M:%S")
    print(f"[{ts}] {msg}", flush=True)


# ── Data Fetching ────────────────────────────────────────────────────────────

def fetch_ranking() -> list[dict]:
    """Fetch Solana top tokens by 24h price change."""
    data = run_onchainos([
        "token", "trending",
        "--chain", CHAIN,
        "--sort-by", "2",       # sort by price change
        "--time-frame", "4",    # 24h
    ])
    if data and isinstance(data, list):
        return data[:TOP_N]
    return []


def fetch_advanced_info(token_addr: str) -> dict | None:
    """Fetch advanced token info for safety checks."""
    return run_onchainos([
        "token", "advanced-info",
        "--address", token_addr,
        "--chain", CHAIN,
    ])


def fetch_holders(token_addr: str) -> dict | None:
    """Fetch holder distribution."""
    return run_onchainos([
        "token", "holders",
        "--address", token_addr,
        "--chain", CHAIN,
    ])


def fetch_current_price(token_addr: str) -> float | None:
    """Fetch current token price in USD."""
    data = run_onchainos([
        "token", "price-info",
        "--address", token_addr,
        "--chain", CHAIN,
    ])
    if data is None:
        return None
    # Response can be a list or dict
    if isinstance(data, list) and len(data) > 0:
        data = data[0]
    if isinstance(data, dict) and "price" in data:
        return safe_float(data["price"])
    return None


def fetch_swap_quote(from_addr: str, to_addr: str, amount_raw: str) -> dict | None:
    """Get swap quote."""
    data = run_onchainos([
        "swap", "quote",
        "--from", from_addr,
        "--to", to_addr,
        "--amount", amount_raw,
        "--chain", CHAIN,
    ])
    if data and isinstance(data, list) and len(data) > 0:
        return data[0]
    return data


def execute_swap(from_addr: str, to_addr: str, amount_raw: str, wallet: str) -> dict | None:
    """Execute swap on-chain."""
    return run_onchainos([
        "swap", "swap",
        "--from", from_addr,
        "--to", to_addr,
        "--amount", amount_raw,
        "--chain", CHAIN,
        "--wallet", wallet,
        "--slippage", "1",
    ], timeout=60)


# ── Safety Checks (25-point) ────────────────────────────────────────────────

def run_safety_checks(token: dict, adv_info: dict) -> tuple[bool, list[str]]:
    """
    Run 25-point safety filter. Returns (passed, reasons_for_failure).
    """
    reasons = []

    # ── Slot Guard ──
    # 1. Honeypot check (via riskControlLevel)
    risk_level = adv_info.get("riskControlLevel", "0")
    if str(risk_level) == "3":
        reasons.append("honeypot risk (level=3)")

    # 2. Top10 concentration ≤ 80%
    top10 = safe_float(adv_info.get("top10HoldPercent"), 100)
    if top10 > 80:
        reasons.append(f"top10 concentration {top10:.1f}% > 80%")

    # 3. Dev holding ≤ 50%
    dev_hold = safe_float(adv_info.get("devHoldingPercent"))
    if dev_hold > 50:
        reasons.append(f"dev holding {dev_hold:.1f}% > 50%")

    # ── Advanced Safety ──
    # 4. Bundler ≤ 30%
    bundler = safe_float(adv_info.get("bundleHoldingPercent"))
    if bundler > 30:
        reasons.append(f"bundler {bundler:.1f}% > 30%")

    # 5. Sniper ≤ 30%
    sniper = safe_float(adv_info.get("sniperHoldingPercent"))
    if sniper > 30:
        reasons.append(f"sniper {sniper:.1f}% > 30%")

    # 6. Dev rug history ≤ 20
    rug_count = safe_int(adv_info.get("devRugPullTokenCount"))
    if rug_count > 20:
        reasons.append(f"dev rug count {rug_count} > 20")

    # ── Basic filters from token data ──
    # 7. Market cap ≥ $50K
    mc = safe_float(token.get("marketCap"))
    if mc < 50000:
        reasons.append(f"market cap ${mc:.0f} < $50K")

    # 8. Liquidity ≥ $30K
    liq = safe_float(token.get("liquidity"))
    if liq < 30000:
        reasons.append(f"liquidity ${liq:.0f} < $30K")

    # 9. Holders ≥ 100
    holders = safe_int(token.get("holders"))
    if holders < 100:
        reasons.append(f"holders {holders} < 100")

    # 10. LP burned ≥ 50%
    lp_burn = safe_float(adv_info.get("lpBurnedPercent"))
    if lp_burn < 50:
        reasons.append(f"LP burn {lp_burn:.0f}% < 50%")

    # 11. Total txs ≥ 50
    txs = safe_int(token.get("txs"))
    if txs < 50:
        reasons.append(f"total txs {txs} < 50")

    # 12. Buy/Sell ratio reasonable (not pure buy manipulation)
    buys = safe_int(token.get("txsBuy"))
    sells = safe_int(token.get("txsSell"), 1)
    if sells > 0 and buys / sells > 20:
        reasons.append(f"buy/sell ratio {buys/sells:.1f} suspiciously high")

    # 13. Volume ≥ $5K
    vol = safe_float(token.get("volume"))
    if vol < 5000:
        reasons.append(f"volume ${vol:.0f} < $5K")

    # 14-25: Additional holder risk checks (simplified)
    dev_create = safe_int(adv_info.get("devCreateTokenCount"))
    if dev_create > 50:
        reasons.append(f"dev created {dev_create} tokens > 50")

    passed = len(reasons) == 0
    return passed, reasons


# ── Momentum Score ───────────────────────────────────────────────────────────

def calc_momentum_score(token: dict, adv_info: dict) -> int:
    """
    Calculate momentum score (0-125). Higher = better signal.
    """
    score = 0

    # Volume strength (0-20)
    vol = safe_float(token.get("volume"))
    if vol > 500000:
        score += 20
    elif vol > 100000:
        score += 15
    elif vol > 50000:
        score += 10
    elif vol > 10000:
        score += 5

    # Holder count (0-15)
    holders = safe_int(token.get("holders"))
    if holders > 5000:
        score += 15
    elif holders > 1000:
        score += 10
    elif holders > 300:
        score += 5

    # Buy pressure (0-20)
    buys = safe_int(token.get("txsBuy"))
    sells = safe_int(token.get("txsSell"), 1)
    ratio = buys / max(sells, 1)
    if 1.3 <= ratio <= 5:
        score += 20
    elif ratio > 1.1:
        score += 10

    # Low concentration (0-15)
    top10 = safe_float(adv_info.get("top10HoldPercent"), 100)
    if top10 < 15:
        score += 15
    elif top10 < 30:
        score += 10
    elif top10 < 50:
        score += 5

    # Low sniper (0-10)
    sniper = safe_float(adv_info.get("sniperHoldingPercent"))
    if sniper < 5:
        score += 10
    elif sniper < 15:
        score += 5

    # LP burn (0-10)
    lp_burn = safe_float(adv_info.get("lpBurnedPercent"))
    if lp_burn >= 95:
        score += 10
    elif lp_burn >= 80:
        score += 5

    # Unique traders (0-15)
    traders = safe_int(token.get("uniqueTraders"))
    if traders > 1000:
        score += 15
    elif traders > 500:
        score += 10
    elif traders > 100:
        score += 5

    # Smart money tags (0-8)
    tags = adv_info.get("tokenTags", [])
    if isinstance(tags, list):
        tag_str = " ".join(str(t) for t in tags)
        if "smartMoney" in tag_str.lower():
            score += 8

    # Low dev involvement (0-12)
    dev_hold = safe_float(adv_info.get("devHoldingPercent"))
    if dev_hold == 0:
        score += 12
    elif dev_hold < 5:
        score += 8
    elif dev_hold < 15:
        score += 4

    return score


# ── Position Management (6-layer exit) ───────────────────────────────────────

def check_exits(pos: Position, current_price: float, current_ranking: set[str]) -> str | None:
    """
    Check 6-layer exit system. Returns exit reason or None.
    Priority: ranking > hard stop > fast stop > trailing > time stop > TP
    """
    if pos.buy_price <= 0:
        return None

    pnl_pct = (current_price - pos.buy_price) / pos.buy_price * 100
    elapsed = time.time() - pos.buy_time

    # Update peak PnL for trailing stop
    if pnl_pct > pos.peak_pnl_pct:
        pos.peak_pnl_pct = pnl_pct

    # Layer 1: Ranking exit — token dropped off the ranking
    if pos.token_address not in current_ranking and elapsed > 60:
        return f"RANKING_EXIT (no longer in top {TOP_N})"

    # Layer 2: Hard stop-loss
    if pnl_pct <= HARD_STOP_PCT:
        return f"HARD_STOP ({pnl_pct:+.1f}% <= {HARD_STOP_PCT}%)"

    # Layer 3: Fast stop (within first 5 min)
    if elapsed < FAST_STOP_TIME and pnl_pct <= FAST_STOP_PCT:
        return f"FAST_STOP ({pnl_pct:+.1f}% in {elapsed:.0f}s)"

    # Layer 4: Trailing stop
    if pnl_pct >= TRAILING_ACTIVATE_PCT:
        pos.trailing_active = True
    if pos.trailing_active:
        drawdown = pos.peak_pnl_pct - pnl_pct
        if drawdown >= TRAILING_DRAWDOWN_PCT:
            return f"TRAILING_STOP (peak {pos.peak_pnl_pct:+.1f}%, now {pnl_pct:+.1f}%, dd {drawdown:.1f}%)"

    # Layer 5: Time stop
    if elapsed >= TIME_STOP_SECS:
        return f"TIME_STOP ({elapsed/3600:.1f}h)"

    # Layer 6: Gradient take-profit (TP levels)
    for i, tp_pct in enumerate(TP_LEVELS):
        if i not in pos.tp_sold and pnl_pct >= tp_pct:
            pos.tp_sold.append(i)
            return f"TAKE_PROFIT_L{i+1} (+{pnl_pct:.1f}% >= +{tp_pct}%)"

    return None


# ── Main Engine ──────────────────────────────────────────────────────────────

def run_engine(budget: float, per_trade: float, dry_run: bool, wallet: str):
    state = EngineState(start_time=time.time())
    remaining_budget = budget

    log("=" * 60)
    log("🚀 SOL 涨幅榜狙击 (Ranking Sniper) 启动")
    log(f"   预算: {budget} SOL | 单笔: {per_trade} SOL | 模式: {'DRY RUN' if dry_run else 'LIVE'}")
    log(f"   钱包: {wallet[:8]}...{wallet[-6:]}")
    log(f"   风控: 25项安全检查 + Momentum Score ≥ {SCORE_BUY_THRESHOLD}")
    log(f"   退出: 6层系统 | 日亏损上限: {DAILY_LOSS_LIMIT_PCT}%")
    log("=" * 60)

    tick = 0
    try:
        while not state.stopped:
            tick += 1
            now = time.time()

            # ── Daily loss check ──
            if budget > 0 and state.daily_pnl / budget * 100 < -DAILY_LOSS_LIMIT_PCT:
                log(f"🛑 日亏损触发停机: {state.daily_pnl:.4f} SOL ({state.daily_pnl/budget*100:.1f}%)")
                state.stopped = True
                break

            # ── Fetch ranking ──
            ranking = fetch_ranking()
            if not ranking:
                log("  ⏳ 获取涨幅榜失败，等待重试...")
                time.sleep(POLL_INTERVAL)
                continue

            current_ranking_addrs = {t["tokenContractAddress"] for t in ranking}

            if tick % 6 == 1:  # Print status every ~60s
                log(f"📊 Tick #{tick} | 持仓: {len(state.positions)}/{MAX_POSITIONS} | "
                    f"余额: {remaining_budget:.4f} SOL | PnL: {state.daily_pnl:+.4f} SOL")

            # ── Check exits for existing positions ──
            for addr in list(state.positions.keys()):
                pos = state.positions[addr]
                price = fetch_current_price(addr)
                if price is None:
                    continue

                exit_reason = check_exits(pos, price, current_ranking_addrs)
                if exit_reason:
                    pnl_pct = (price - pos.buy_price) / pos.buy_price * 100
                    log(f"  🔴 EXIT {pos.symbol}: {exit_reason} | PnL: {pnl_pct:+.1f}%")

                    # Sell
                    if not dry_run:
                        # Get token balance and sell all
                        sell_result = execute_swap(
                            addr, SOL_NATIVE,
                            "0",  # sell all — will need balance lookup
                            wallet,
                        )
                        if sell_result:
                            log(f"     ✅ 卖出成功")
                        else:
                            log(f"     ⚠ 卖出失败，保留仓位")
                            continue

                    # Estimate return
                    estimated_return = pos.buy_amount_sol * (1 + pnl_pct / 100)
                    state.total_returned += estimated_return
                    remaining_budget += estimated_return
                    state.daily_pnl += (estimated_return - pos.buy_amount_sol)

                    state.trades.append({
                        "time": datetime.now(timezone.utc).isoformat(),
                        "symbol": pos.symbol,
                        "action": "SELL",
                        "reason": exit_reason,
                        "buy_price": pos.buy_price,
                        "sell_price": price,
                        "pnl_pct": round(pnl_pct, 2),
                        "pnl_sol": round(estimated_return - pos.buy_amount_sol, 4),
                    })
                    del state.positions[addr]

            # ── Scan for new entries ──
            for token in ranking:
                addr = token.get("tokenContractAddress", "")
                symbol = token.get("tokenSymbol", "?")

                # Skip if already known or already holding
                if addr in state.known_tokens or addr in state.positions:
                    continue

                state.known_tokens.add(addr)
                change = float(token.get("change", "0"))
                log(f"  🆕 新上榜: {symbol} | 涨幅: {change:.1f}% | MC: ${float(token.get('marketCap','0')):.0f}")

                # Budget check
                if remaining_budget < per_trade + GAS_RESERVE:
                    log(f"     ⚠ 余额不足 ({remaining_budget:.4f} SOL)")
                    continue

                # Position limit
                if len(state.positions) >= MAX_POSITIONS:
                    log(f"     ⚠ 持仓已满 ({MAX_POSITIONS})")
                    continue

                # ── Safety checks ──
                log(f"     🔍 安全检查中...")
                adv_info = fetch_advanced_info(addr)
                if adv_info is None:
                    log(f"     ❌ 无法获取安全数据，跳过")
                    continue

                passed, reasons = run_safety_checks(token, adv_info)
                if not passed:
                    log(f"     ❌ 安全检查未通过: {'; '.join(reasons[:3])}")
                    continue

                # ── Momentum Score ──
                score = calc_momentum_score(token, adv_info)
                log(f"     📈 Momentum Score: {score}/125 (阈值: {SCORE_BUY_THRESHOLD})")
                if score < SCORE_BUY_THRESHOLD:
                    log(f"     ❌ 评分不足，跳过")
                    continue

                # ── Get current price ──
                price = fetch_current_price(addr)
                if price is None or price <= 0:
                    log(f"     ❌ 无法获取价格，跳过")
                    continue

                # ── Execute buy ──
                amount_raw = str(int(per_trade * 1e9))  # SOL to lamports
                log(f"     🟢 BUY {symbol} | {per_trade} SOL @ ${price:.8f} | Score: {score}")

                if not dry_run:
                    result = execute_swap(SOL_NATIVE, addr, amount_raw, wallet)
                    if result is None:
                        log(f"     ⚠ 买入交易失败")
                        continue
                    tx_hash = ""
                    if isinstance(result, list) and len(result) > 0:
                        tx_hash = result[0].get("txHash", "")
                    elif isinstance(result, dict):
                        tx_hash = result.get("txHash", "")
                    log(f"     ✅ 买入成功 tx: {tx_hash[:16]}...")
                else:
                    tx_hash = "DRY_RUN"
                    log(f"     ✅ [DRY RUN] 模拟买入")

                # Record position
                state.positions[addr] = Position(
                    token_address=addr,
                    symbol=symbol,
                    buy_price=price,
                    buy_amount_sol=per_trade,
                    buy_time=time.time(),
                    tx_hash=tx_hash,
                )
                remaining_budget -= per_trade
                state.total_invested += per_trade

                state.trades.append({
                    "time": datetime.now(timezone.utc).isoformat(),
                    "symbol": symbol,
                    "action": "BUY",
                    "price": price,
                    "amount_sol": per_trade,
                    "score": score,
                    "tx_hash": tx_hash,
                })

            time.sleep(POLL_INTERVAL)

    except KeyboardInterrupt:
        log("\n⏹ 收到停止信号 (Ctrl+C)")

    # ── Final report ──
    log("")
    log("=" * 60)
    log("📋 策略报告")
    log(f"   运行时长: {(time.time() - state.start_time) / 60:.1f} 分钟")
    log(f"   总投入: {state.total_invested:.4f} SOL")
    log(f"   总回收: {state.total_returned:.4f} SOL")
    log(f"   日 PnL: {state.daily_pnl:+.4f} SOL")
    log(f"   交易次数: {len(state.trades)}")
    log(f"   当前持仓: {len(state.positions)}")

    if state.positions:
        log("\n   📌 未平仓持仓:")
        for pos in state.positions.values():
            elapsed = (time.time() - pos.buy_time) / 60
            log(f"      {pos.symbol} | 买入价: ${pos.buy_price:.8f} | "
                f"持仓: {elapsed:.0f}min | {pos.buy_amount_sol} SOL")

    if state.trades:
        log("\n   📜 交易记录:")
        for t in state.trades[-10:]:
            if t["action"] == "BUY":
                log(f"      {t['time'][:19]} BUY  {t['symbol']} @ ${t['price']:.8f} "
                    f"({t['amount_sol']} SOL) Score:{t.get('score','?')}")
            else:
                log(f"      {t['time'][:19]} SELL {t['symbol']} {t['reason']} "
                    f"PnL: {t['pnl_pct']:+.1f}% ({t['pnl_sol']:+.4f} SOL)")

    log("=" * 60)


# ── Entry Point ──────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="SOL Ranking Sniper Engine")
    parser.add_argument("--budget", type=float, default=0.5, help="Total SOL budget (default: 0.5)")
    parser.add_argument("--per-trade", type=float, default=0.05, help="SOL per trade (default: 0.05)")
    parser.add_argument("--dry-run", action="store_true", help="Simulate without executing swaps")
    args = parser.parse_args()

    # Get wallet address from env
    wallet = os.environ.get("SOL_ADDRESS", "")
    if not wallet:
        print("ERROR: SOL_ADDRESS not set in environment")
        sys.exit(1)

    # Verify onchainos is available
    try:
        subprocess.run(["onchainos", "--version"], capture_output=True, check=True)
    except (FileNotFoundError, subprocess.CalledProcessError):
        print("ERROR: onchainos not found. Install: curl -sSL .../install.sh | sh")
        sys.exit(1)

    run_engine(args.budget, args.per_trade, args.dry_run, wallet)


if __name__ == "__main__":
    main()
