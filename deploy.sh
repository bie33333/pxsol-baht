#!/usr/bin/env bash
# 用法：
#   MINTER_ADDRESS=<你的base58地址> ./deploy.sh
#   RPC_URL=https://api.devnet.solana.com MINTER_ADDRESS=... ./deploy.sh
set -euo pipefail

# ============================================================
# 配置
# ============================================================
PROGRAM_NAME="pxsol_baht"
PROGRAM_KEYPAIR="./target/deploy/${PROGRAM_NAME}-keypair.json"
PROGRAM_SO="./target/deploy/${PROGRAM_NAME}.so"
RPC_URL="${RPC_URL:-https://api.devnet.solana.com}"
MINTER_ADDRESS="${MINTER_ADDRESS:-}"

# ============================================================
# 0. 环境检查
# ============================================================
command -v solana            >/dev/null || { echo "❌ 请先安装 Solana CLI";   exit 1; }
command -v cargo             >/dev/null || { echo "❌ 请先安装 Rust";         exit 1; }
command -v cargo-build-sbf   >/dev/null || { echo "❌ 请先安装 cargo-build-sbf（solana-install）"; exit 1; }
command -v python3           >/dev/null || { echo "❌ 需要 python3 做 base58 解码"; exit 1; }

# ============================================================
# 1. 把 MINTER 的 base58 地址转换成 [u8; 32] 并写入 src/lib.rs
# ============================================================
if [[ -n "$MINTER_ADDRESS" ]]; then
  echo "🔑 解析 minter 地址：$MINTER_ADDRESS"
  ARRAY=$(python3 - "$MINTER_ADDRESS" <<'PYEOF'
import sys
s = sys.argv[1]
alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n = 0
for c in s:
    n = n * 58 + alphabet.index(c)
leading = len(s) - len(s.lstrip('1'))
out = []
while n > 0:
    out.append(n & 0xff)
    n >>= 8
out = [0] * leading + out[::-1]
out = [0] * (32 - len(out)) + out
print(', '.join(str(b) for b in out))
PYEOF
)

  echo "   MINTER_PUBKEY = [$ARRAY]"

  if grep -q 'const MINTER_PUBKEY: \[u8; 32\] = \[0u8; 32\];' src/lib.rs; then
    sed -i "s/const MINTER_PUBKEY: \[u8; 32\] = \[0u8; 32\];/const MINTER_PUBKEY: [u8; 32] = [$ARRAY];/" src/lib.rs
    echo "   ✅ 已替换 src/lib.rs 中的占位符"
  else
    echo "   ⚠️ src/lib.rs 中没有找到 [0u8; 32] 占位符，请手动替换"
  fi
else
  echo "⚠️ 未提供 MINTER_ADDRESS，跳过 MINTER_PUBKEY 替换"
  echo "   用法：MINTER_ADDRESS=<base58地址> ./deploy.sh"
fi

# ============================================================
# 2. 构建
# ============================================================
echo ""
echo "🔨 cargo build-sbf ..."
cargo build-sbf

# ============================================================
# 3. 生成 program keypair（如果不存在）
# ============================================================
if [[ ! -f "$PROGRAM_KEYPAIR" ]]; then
  echo ""
  echo "🔑 生成 program keypair ..."
  solana-keygen new -o "$PROGRAM_KEYPAIR" --no-bip39-passphrase --force
fi

PROGRAM_ID=$(solana address -k "$PROGRAM_KEYPAIR")
echo ""
echo "📦 Program ID: $PROGRAM_ID"

# ============================================================
# 4. 部署
# ============================================================
echo ""
echo "🚀 部署到 $RPC_URL ..."
solana program deploy \
  --url "$RPC_URL" \
  --program-id "$PROGRAM_KEYPAIR" \
  "$PROGRAM_SO"

echo ""
echo "✅ 部署完成"
echo "   Program ID:     $PROGRAM_ID"
echo "   Minter 地址:    ${MINTER_ADDRESS:-（未提供）}"
echo ""
echo "   请把上面两个值分别填进 index.html 的："
echo "     PROGRAM_ID     常量"
echo "     MINTER_ADDRESS 常量"