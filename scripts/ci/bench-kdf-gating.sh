#!/usr/bin/env bash
# Sub-B (#40) BC-3 リリースブロッカ — KDF 性能ベンチ gating。
#
# 設計書 `docs/features/vault-encryption/detailed-design/kdf.md` §性能契約 で
# 「Argon2id `FROZEN_OWASP_2024_05` の単一 derive 時間 p95 ≤ 1.0 秒、超過で
# リリースブロッカ」を凍結。本スクリプトは `cargo bench` の出力 (bencher
# format) から **median** を抽出し、`750 ms` の **proxy 閾値** と比較して
# 超過時に exit 1 で CI を fail させる。
#
# proxy 閾値 750 ms の根拠:
#   - criterion デフォルト出力では p95 を直接得られない (median + 信頼区間のみ)。
#   - 真の p95 ≤ 1.0 s を保証するには median ≤ 750 ms 程度 (1000 ÷ 1.33 安全係数)
#     を満たす必要がある。本値は kdf.md §性能契約 の「p50 < 100 ms は逆に弱すぎ
#     警告」と整合的に下限ガードと併用 (本スクリプトは上限のみ、下限は将来追加)。
#   - 4 年再評価サイクル (OWASP 推奨更新) で本閾値も同時に再評価する。
#
# 使い方: `just bench-kdf` または `bash scripts/ci/bench-kdf-gating.sh`。
# CI workflow `.github/workflows/bench-kdf.yml` から呼ばれる。

set -euo pipefail

THRESHOLD_MS=750
LOG=$(mktemp)
trap 'rm -f "$LOG"' EXIT

echo "=== Sub-B (#40) BC-3 KDF bench gating (threshold: ${THRESHOLD_MS} ms) ==="
echo

# bencher format で出力すると `bench: 11_452_xxx ns/iter (+/- xxx)` 形式の
# 1 行が各ベンチ 1 つに対応するため shell パースしやすい。
# `--measurement-time` を 5 秒、`--warm-up-time` を 2 秒に短縮して CI 時間を抑制。
# default (10s + 3s) でも閾値判定は変わらないため CI コスト最小化を優先。
cargo bench -p shikomi-infra --bench kdf_bench -- \
    --measurement-time 5 --warm-up-time 2 --output-format bencher 2>&1 | tee "$LOG"

echo
echo "=== Parsing bench results ==="

# `test argon2id_derive_kek_pw_frozen_owasp_2024_05 ... bench: 11_452_xxx ns/iter (+/- xxx)`
# から ns 単位の median を抽出。
extract_ns() {
    local name="$1"
    local line
    line=$(grep -E "test ${name}\s+\.\.\.\s+bench:" "$LOG" || true)
    if [[ -z "$line" ]]; then
        echo "[FAIL] could not find bench output for: $name"
        return 1
    fi
    # `bench: 11_452_xxx ns/iter (+/- xxx)` の数値部分を抽出して `_` を除去
    local ns
    ns=$(echo "$line" | sed -E 's/.*bench:[[:space:]]+([0-9_]+)[[:space:]]+ns\/iter.*/\1/' | tr -d '_')
    if [[ -z "$ns" || ! "$ns" =~ ^[0-9]+$ ]]; then
        echo "[FAIL] could not parse ns value from: $line"
        return 1
    fi
    echo "$ns"
}

ARGON2_NS=$(extract_ns "argon2id_derive_kek_pw_frozen_owasp_2024_05")
BIP39_NS=$(extract_ns "bip39_derive_kek_recovery_24_words")

# ms 換算 (整数除算、切り捨て)。
# `1000000` のリテラルにアンダースコアを使わない: bash 5.x (Linux) は arithmetic
# 内の `1_000_000` を syntax error として拒否、bash 3.2 (macOS デフォルト) は
# 古いパーサで underscore を無視し `1000000` と等価扱いするため OS 間で挙動が
# 分岐する。CI 移植性のためアンダースコアを除去して `1000000` と書き下す。
ARGON2_MS=$((ARGON2_NS / 1000000))
BIP39_MS=$((BIP39_NS / 1000000))

echo
echo "Argon2id derive_kek_pw (FROZEN_OWASP_2024_05): ${ARGON2_MS} ms (raw: ${ARGON2_NS} ns)"
echo "Bip39  derive_kek_recovery  (24 words)        : ${BIP39_MS} ms (raw: ${BIP39_NS} ns)"
echo

FAIL=0

# 上限閾値 gating (BC-3 リリースブロッカ)
if [[ $ARGON2_MS -gt $THRESHOLD_MS ]]; then
    echo "[FAIL] Argon2id derive median ${ARGON2_MS} ms > ${THRESHOLD_MS} ms (BC-3 release blocker)"
    echo "       → tech-stack.md §4.7 + kdf.md §性能契約 + Argon2idParams::FROZEN_OWASP_2024_05 の同時改訂が必要"
    FAIL=1
else
    echo "[PASS] Argon2id derive median ${ARGON2_MS} ms <= ${THRESHOLD_MS} ms (BC-3 within budget)"
fi

# Bip39 経路は KEK 派生時に random 入力なし (24 語固定)、想定 < 50 ms。
# 上限は同じ 750 ms を適用 (UX として「unlock 全体 1 秒」内に収まるべき)。
if [[ $BIP39_MS -gt $THRESHOLD_MS ]]; then
    echo "[FAIL] Bip39 derive median ${BIP39_MS} ms > ${THRESHOLD_MS} ms (BC-3 release blocker)"
    FAIL=1
else
    echo "[PASS] Bip39 derive median ${BIP39_MS} ms <= ${THRESHOLD_MS} ms (BC-3 within budget)"
fi

echo
if [[ $FAIL -ne 0 ]]; then
    echo "=== bench-kdf gating FAILED ==="
    exit 1
fi

echo "=== bench-kdf gating PASSED ==="
