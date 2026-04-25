# テスト設計書 — daemon-ipc / CI 検証・配置・証跡

> `index.md` の §2 索引からの分割ファイル。3 OS matrix CI・静的監査スクリプト・証跡提出・実行コマンドを扱う。

## 1. CI 検証ケース一覧

ペテルギウス／服部平次／セル review で確立した静的契約を全て CI に落とす。**Stage 1（早期 fail）で静的 grep を先に走らせる**ことで、秘密経路・プロトコル契約違反が main に到達する前に検出する。

### 1.1 共通（全 OS 共通）

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-001 | 12 | `cargo fmt --check --all` | exit 0 |
| TC-CI-002 | 12 | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| TC-CI-003 | 12 | `cargo deny check advisories licenses bans sources` | exit 0（`tokio ^1.44.2` / `tokio-util ^0.7` / `rmp-serde ^1.3` / `bytes ^1` / `nix ^0.29` / `windows-sys ^0.59` 全て pass） |
| TC-CI-004 | 12, 13 | `cargo test --workspace --all-targets` | exit 0、全テスト pass |

### 1.2 arch ドキュメント差分ゼロ（受入基準 14）

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-011 | 14 | `git diff --name-only origin/develop...HEAD -- docs/architecture/` | **出力が空**（本 feature PR で `docs/architecture/` 配下を変更しない、工程 0 で完結済み） |

### 1.3 `shikomi-core::ipc` 純粋性（受入基準 15）

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-012 | 15 | `grep -E '^(tokio\|rmp-serde\|tokio-util)' crates/shikomi-core/Cargo.toml` の **`[dependencies]` セクション** | **マッチ 0 件**（純粋性契約、`tech-stack.md` §4.5） |
| TC-CI-013 | 15 | `grep -rn 'use tokio\|use rmp_serde\|use tokio_util' crates/shikomi-core/src/ipc/` | **マッチ 0 件**（`serde` のみ許可） |

### 1.4 `--ipc` コンポジションルート局所性（受入基準 16）

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-014 | 16 | `grep -rn 'IpcVaultRepository\|SqliteVaultRepository' crates/shikomi-cli/src/` の**マッチ**が `lib.rs` / `main.rs` / `io/ipc_vault_repository.rs` / `io/ipc_client.rs` / `io/mod.rs` 以外に出ない | `usecase/` / `presenter/` / `error.rs` / `view.rs` / `input.rs` / `cli.rs` には出現しない（Clean Arch 境界保持、`cli-vault-commands` の TC-CI-012 を拡張） |

### 1.5 secret 経路監査（受入基準 17）

`expose_secret` 呼出 0 件契約を**3 領域**に拡張。`scripts/ci/audit-secret-paths.sh` を拡張して 3 領域全てで grep する。

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-015 | 17 | `grep -rn 'expose_secret' crates/shikomi-core/src/ipc/` | **マッチ 0 件** |
| TC-CI-016 | 17 | `grep -rn 'expose_secret' crates/shikomi-cli/src/io/` | **マッチ 0 件**（既存 TC-CI-013 と同じ範囲、本 feature で新規ファイル `ipc_vault_repository.rs` / `ipc_client.rs` を追加しても契約維持） |
| TC-CI-017 | 17 | `grep -rn 'expose_secret' crates/shikomi-daemon/src/` | **マッチ 0 件**（新 crate のため本 feature で初出） |

**除外範囲**（`expose_secret` の呼出が**許可**される既存箇所、監査対象外）:
- `crates/shikomi-core/src/secret/` 配下（`SecretBytes::as_serialize_slice` 等の核心実装、`../basic-design/security.md §expose_secret 経路監査`）
- `crates/shikomi-core/src/vault/record.rs` 内の `text_preview`（`cli-vault-commands` で確立済み）
- `tests/` 配下全般（テストでの比較用）

### 1.6 `rmp_serde::Raw` / `RawRef` 不使用（受入基準 18）

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-018 | 18 | `grep -rnE 'rmp_serde::(Raw\|RawRef)\|::Raw\b\|::RawRef\b' crates/shikomi-core/src/ipc/` | **マッチ 0 件**（RUSTSEC-2022-0092 unsound 経路の再接触を構造的に遮断、`tech-stack.md` §2.1 契約） |

### 1.7 `unsafe` ブロックの局所化

daemon 側と CLI 側の**両 crate**で `unsafe` ブロックを OS API モジュールに局所化する契約。CLI 側は本 feature で新設される `io/windows_sid.rs`（Windows SID 取得、`../basic-design/security.md §unsafe_code の扱い` 3 領域表）のみに閉じ込める。

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-019 | （セキュリティ契約、`../basic-design/security.md §unsafe_code の扱い` daemon 側） | `grep -rn 'unsafe \{' crates/shikomi-daemon/src/` → `permission/unix.rs` / `permission/windows.rs` 以外のマッチが 0 件 | `ipc/` / `lifecycle/` / `lib.rs` / `main.rs` / `panic_hook.rs` 配下に `unsafe {` が出現しない |
| **TC-CI-026** | （セキュリティ契約、`../basic-design/security.md §unsafe_code の扱い` CLI 側、**服部 re-review 指摘 ① 対応 2026-04-25 新設**） | `grep -rnE 'unsafe[[:space:]]*\{' crates/shikomi-cli/src/` → `io/windows_sid.rs` 以外のマッチが 0 件 | `usecase/` / `presenter/` / `io/ipc_vault_repository.rs` / `io/ipc_client.rs` / `error.rs` / `view.rs` / `input.rs` / `cli.rs` / `lib.rs` / `main.rs` 配下に `unsafe {` が出現しない（Windows SID 取得経路のみ `io/windows_sid.rs` に局所化） |

### 1.8 daemon panic hook 監査 + env 裏口読取禁止

panic hook 内部の secret 漏洩経路監査（TC-CI-023/024）に加え、本 feature の**env 裏口削除契約**（`integration.md §8.1` / `unit.md §3.1` 禁止事項）を CI で構造的に強制する（**服部 re-review 指摘 ② 対応 2026-04-25 新設**）。

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-023 | （セキュリティ、`../basic-design/security.md §panic hook`） | panic hook ブロック（`fn panic_hook(` から閉じ括弧まで）を `awk` で抽出し、`grep -E 'tracing::'` | **マッチ 0 件**（panic hook 内で tracing マクロを呼ばない） |
| TC-CI-024 | 同上 | 同ブロック内で `grep -E '\.payload\(\)\|info\.payload\|info\.message\|info\.location\|PanicHookInfo::payload'` | **マッチ 0 件**（secret が混入する可能性のある payload / message / location を参照しない） |
| **TC-CI-027** | （契約、`integration.md §8.1 / unit.md §3.1 禁止事項`、**新設**） | `grep -rnE 'env::var.*SHIKOMI_DAEMON_SKIP\|std::env::var.*SHIKOMI_DAEMON_SKIP' crates/shikomi-daemon/src/ crates/shikomi-cli/src/` | **マッチ 0 件**（`SHIKOMI_DAEMON_SKIP_PEER_VERIFY` 等のテスト用 env 裏口読取コードが本番 `src/` に復活していないこと。trait 注入一本化契約の CI 強制。tests/ 配下は対象外——`common/peer_mock.rs` では env を読む必要がなく trait 実装のみ） |

### 1.9 3 OS matrix CI（受入基準 13）

| TC-ID | 対応受入基準 | 操作 | 期待結果 |
|-------|------------|------|---------|
| TC-CI-020 | 13 | GitHub Actions `test-daemon.yml`（新設）で `runs-on: ubuntu-latest` の job が `cargo test -p shikomi-daemon --all-targets` pass | exit 0 |
| TC-CI-021 | 13 | 同 workflow で `runs-on: macos-latest` の job が pass | exit 0 |
| TC-CI-022 | 13 | 同 workflow で `runs-on: windows-latest` の job が pass（`pwsh` シェル、既存 `windows.yml` と同型） | exit 0 |
| TC-CI-025 | 13 | 同 workflow で `cargo test -p shikomi-cli --test 'e2e_ipc_*'` が 3 OS 全てで pass | exit 0 |

---

## 2. 静的監査スクリプト拡張（`scripts/ci/audit-secret-paths.sh`）

既存スクリプトを本 feature の TC-CI-015〜017 / 018 / 019 / 023 / 024 に対応するよう拡張する。

**追加セクション案**（既存の TC-CI-013〜015 に連なる形）:

```bash
# --- TC-CI-015 / 016 / 017 -----------------------------------------
echo "[TC-CI-015] expose_secret in shikomi-core/src/ipc/"
if matches="$(grep -rn 'expose_secret' crates/shikomi-core/src/ipc/ 2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-015 FAIL"
fi; echo "[TC-CI-015] PASS"

echo "[TC-CI-016] expose_secret in shikomi-cli/src/io/"
if matches="$(grep -rn 'expose_secret' crates/shikomi-cli/src/io/ 2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-016 FAIL"
fi; echo "[TC-CI-016] PASS"

echo "[TC-CI-017] expose_secret in shikomi-daemon/src/"
if matches="$(grep -rn 'expose_secret' crates/shikomi-daemon/src/ 2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-017 FAIL"
fi; echo "[TC-CI-017] PASS"

# --- TC-CI-018 ----------------------------------------------------
echo "[TC-CI-018] rmp_serde::Raw / RawRef in shikomi-core/src/ipc/"
if matches="$(grep -rnE 'rmp_serde::(Raw|RawRef)|::Raw\b|::RawRef\b' crates/shikomi-core/src/ipc/ 2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-018 FAIL"
fi; echo "[TC-CI-018] PASS"

# --- TC-CI-019 ----------------------------------------------------
echo "[TC-CI-019] unsafe blocks outside permission/ (shikomi-daemon)"
if matches="$(grep -rn 'unsafe[[:space:]]*{' crates/shikomi-daemon/src/ \
    --include='*.rs' \
    | grep -v 'crates/shikomi-daemon/src/permission/unix.rs' \
    | grep -v 'crates/shikomi-daemon/src/permission/windows.rs' \
    2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-019 FAIL"
fi; echo "[TC-CI-019] PASS"

# --- TC-CI-026 ----------------------------------------------------
# 服部 re-review 指摘 ① 対応: CLI 側 unsafe 局所化を CI 強制
echo "[TC-CI-026] unsafe blocks outside io/windows_sid.rs (shikomi-cli)"
if matches="$(grep -rnE 'unsafe[[:space:]]*\{' crates/shikomi-cli/src/ \
    --include='*.rs' \
    | grep -v 'crates/shikomi-cli/src/io/windows_sid.rs' \
    2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-026 FAIL: unsafe block outside io/windows_sid.rs"
fi; echo "[TC-CI-026] PASS"

# --- TC-CI-023 / 024 ----------------------------------------------
echo "[TC-CI-023/024] daemon panic hook audit"
panic_hook_body="$(awk '/fn panic_hook\(/,/^}$/' \
    crates/shikomi-daemon/src/lib.rs \
    crates/shikomi-daemon/src/main.rs \
    crates/shikomi-daemon/src/panic_hook.rs 2>/dev/null || true)"
if [[ -n "$panic_hook_body" ]]; then
    if echo "$panic_hook_body" | grep -qE 'tracing::'; then
        echo "$panic_hook_body"; fail "TC-CI-023 FAIL: tracing in panic hook"
    fi
    if echo "$panic_hook_body" | grep -qE '\.payload\(\)|info\.payload|PanicHookInfo::payload|info\.message|info\.location'; then
        echo "$panic_hook_body"; fail "TC-CI-024 FAIL: payload/message/location reference"
    fi
fi
echo "[TC-CI-023/024] PASS"

# --- TC-CI-027 ----------------------------------------------------
# 服部 re-review 指摘 ② 対応: env 裏口読取禁止を CI 強制
# SHIKOMI_DAEMON_SKIP_* 系 env の読取コードが本番 src/ に復活しないことを保証
# （trait 注入一本化契約、integration.md §8.1 / unit.md §3.1 禁止事項）
echo "[TC-CI-027] SHIKOMI_DAEMON_SKIP_* env read in production src/"
if matches="$(grep -rnE 'env::var.*SHIKOMI_DAEMON_SKIP|std::env::var.*SHIKOMI_DAEMON_SKIP' \
    crates/shikomi-daemon/src/ \
    crates/shikomi-cli/src/ \
    --include='*.rs' \
    2>/dev/null)"; then
    echo "$matches"; fail "TC-CI-027 FAIL: SHIKOMI_DAEMON_SKIP_* env read in production src/"
fi; echo "[TC-CI-027] PASS"
```

既存の TC-CI-012〜015 セクションは維持（`cli-vault-commands` 由来）、本 feature で上記を**追記**する。`tests/` 配下は対象外（`common/peer_mock.rs` は trait 実装のみで env 不使用、理論上マッチしない）。

---

## 3. 3 OS matrix workflow の新設

`.github/workflows/test-daemon.yml` を新規作成。既存 `test-infra.yml`（Linux 単独） + `windows.yml`（Windows 単独）とは**別ファイル**で、3 OS matrix を 1 ファイルに集約する。

**設計方針**:
- matrix 構成: `os: [ubuntu-latest, macos-latest, windows-latest]`
- Windows は `shell: pwsh` で `just` レシピを実行（既存 `windows.yml` と同型）
- 全 OS で `just test-daemon` を実行（`justfile` に新レシピ追加を実装担当に推奨）
- fail-fast: `false`（1 OS fail でも他 OS の結果を確認したい）

**justfile 追加レシピ案**:

```
test-daemon:
    cargo test -p shikomi-daemon --all-targets
    cargo test -p shikomi-cli --test 'e2e_ipc_*' --all-features
```

---

## 4. CI job の段階構成（推奨）

`.github/workflows/ci.yml` or `test-daemon.yml` の実行順（fail fast）:

1. **Stage 1: 静的チェック**（早期 fail）
   - `cargo fmt --check --all`（TC-CI-001）
   - `cargo clippy --workspace --all-targets -- -D warnings`（TC-CI-002）
   - `bash scripts/ci/audit-secret-paths.sh`（TC-CI-015〜019, 023, 024, **026, 027**）
   - `bash scripts/ci/audit-arch-docs.sh`（TC-CI-011、差分ゼロ確認）
   - `bash scripts/ci/audit-core-purity.sh`（TC-CI-012, 013、`shikomi-core` 純粋性）
2. **Stage 2: 依存監査**
   - `cargo deny check`（TC-CI-003）
3. **Stage 3: テスト（3 OS matrix）**
   - Linux: `cargo test --workspace --all-targets` + `cargo test -p shikomi-daemon --test 'e2e_*'`（TC-CI-004, 020）
   - macOS: 同上（TC-CI-021）
   - Windows: 同上（TC-CI-022）
4. **Stage 4: カバレッジ（Linux のみ）**
   - `cargo llvm-cov -p shikomi-daemon -p shikomi-core --summary-only`（参考値、目標値なし）

Stage 1 を通らなければ後続 job を実行しない（`needs:` で依存化）。**secret 経路監査と純粋性監査を Stage 1 に置く**ことで、万が一の契約違反が main にマージされる前に早期検出する。

---

## 5. テスト実装ファイル配置（全体像）

```
crates/shikomi-core/src/ipc/
├── mod.rs
├── version.rs                    # #[cfg(test)] mod tests { TC-UT-001〜004 }
├── request.rs                    # tests { TC-UT-005〜007 }
├── response.rs                   # tests { TC-UT-008〜010 }
├── summary.rs                    # tests { TC-UT-011〜014 }
├── error_code.rs                 # （手動テストは不要、round-trip で網羅）
└── secret_bytes.rs               # tests { TC-UT-015〜018 }

crates/shikomi-daemon/
├── Cargo.toml                    # [lib] + [[bin]] 構成
├── src/
│   ├── main.rs                   # #[tokio::main] 3 行ラッパ
│   ├── lib.rs                    # pub async fn run()
│   ├── panic_hook.rs             # tests { TC-UT-080 }（fixed-message 確認）
│   ├── lifecycle/
│   │   ├── mod.rs
│   │   ├── single_instance.rs    # OS 統合 tests は tests/it_single_instance.rs
│   │   └── shutdown.rs
│   ├── ipc/
│   │   ├── mod.rs
│   │   ├── server.rs
│   │   ├── framing.rs
│   │   ├── handshake.rs
│   │   ├── handler.rs            # tests { TC-UT-030〜039 }
│   │   └── transport/
│   │       ├── mod.rs
│   │       ├── unix.rs
│   │       └── windows.rs
│   └── permission/
│       ├── mod.rs
│       └── peer_credential/
│           ├── mod.rs
│           ├── unix.rs           # tests { TC-UT-020〜022 }
│           └── windows.rs        # tests { TC-UT-023〜024 }
└── tests/
    ├── common/
    │   ├── mod.rs
    │   ├── daemon_guard.rs       # Drop で kill する RAII
    │   └── peer_mock.rs
    ├── it_protocol_roundtrip.rs  # TC-IT-001〜009
    ├── it_server_connection.rs   # TC-IT-010〜032（正常4 / エラー5 / 異常5 / 独立1 / shutdown3）
    ├── it_single_instance.rs     # TC-IT-060〜071（unix 5 / windows 2）
    ├── e2e_startup.rs            # TC-E2E-001, 002
    ├── e2e_single_instance.rs    # TC-E2E-020〜022
    ├── e2e_shutdown.rs           # TC-E2E-030, 031
    ├── e2e_encrypted.rs          # TC-E2E-070（TC-E2E-071 は scope-out の `#[ignore]`）
    ├── e2e_permissions.rs        # TC-E2E-050, 051
    └── e2e_peer_credential_linux.rs  # TC-E2E-060（#[ignore]、Linux 専用 sudo 検証）

crates/shikomi-cli/src/io/
├── mod.rs                        # 既存編集、ipc_* を export
├── ipc_vault_repository.rs       # tests { TC-UT-050〜054, 060〜064 }
└── ipc_client.rs

crates/shikomi-cli/src/error.rs   # tests { TC-UT-090〜094, 040, 041 }（cli 側の追加バリアント、`From<PersistenceError>` / `From<&CliError> for ExitCode`）
crates/shikomi-cli/src/presenter/error.rs  # tests { TC-UT-070〜073 }

crates/shikomi-cli/tests/
├── common/
│   └── mod.rs                    # spawn_stub_server() / shared fixtures
├── it_ipc_vault_repository.rs    # TC-IT-040〜052（connect 4 / not-running 1 / save diff 3 / incl. reclassified TC-E2E-041）
├── e2e_ipc_crud.rs               # TC-E2E-010〜015
├── e2e_ipc_composition.rs        # TC-E2E-080
└── e2e_ipc_scenarios.rs          # TC-E2E-110〜112
```

**`shikomi-core` 側の追加**: `crates/shikomi-core/src/secret/bytes.rs` が未存在なら新規、既存なら `pub(crate) fn as_serialize_slice(&self) -> &[u8]` を追加（`../basic-design/security.md §SecretBytes のシリアライズ契約`）。本メソッドのテストは `#[cfg(test)] mod tests` に追加（既存 `SecretBytes` UT の延長）。

**`shikomi-infra` 側の変更**: `PersistenceError` に `DaemonNotRunning` / `ProtocolVersionMismatch` / `IpcDecode` / `IpcEncode` / `IpcIo` の 5 バリアント追加（詳細設計 `../detailed-design/ipc-vault-repository.md`）。各バリアントの `Display` 実装が secret を含まないことを `#[cfg(test)] mod tests` で検証（追加 TC-UT 相当、既存 `PersistenceError` UT に準拠）。

---

## 6. 開発者向け実行手順

### 6.1 全テスト実行

```bash
# 全テスト（ユニット + 結合 + E2E、3 OS で同一コマンド）
cargo test --workspace --all-targets

# daemon / core / cli 別
cargo test -p shikomi-daemon --all-targets
cargo test -p shikomi-core --lib ipc
cargo test -p shikomi-cli --all-targets

# 暗号化 vault フィクスチャが必要
cargo test --workspace --features "shikomi-infra/test-fixtures"

# Linux 専用（別ユーザ接続拒否）
cargo test -p shikomi-daemon --test e2e_peer_credential_linux -- --ignored

# CI 監査スクリプト
bash scripts/ci/audit-secret-paths.sh

# 全 CI 一式（ローカル）
just fmt-check
just clippy
just audit
just test
```

### 6.2 人間が動作確認できるタイミング

`e2e.md §15` 参照。実装完了後、daemon を手動起動して `shikomi --ipc list` で bit 同一動作を確認する。

---

## 7. 証跡提出方針（テスト実行後）

`/app/shared/attachments/マユリ/` に保存で Discord 自動添付。**コミットだけ・添付だけは禁止**（テスト戦略ガイド準拠）。

| 種別 | ファイル名 | 内容 |
|------|----------|------|
| E2E 実行ログ | `daemon-ipc-e2e-report.md` | TC-E2E-001〜112 の結果、3 OS matrix 結果、`SECRET_TEST_VALUE` 不在 grep 結果 |
| 結合・ユニット集計 | `daemon-ipc-test-summary.md` | `cargo test` の集計（X passed; Y failed の TC 別表）、TC-UT-080 の固定文言確認、round-trip 成功数 |
| 静的監査 | `daemon-ipc-static-audit.md` | TC-CI-011〜019, 023, 024, 026, 027 の grep 結果（全て 0 件ベースライン） |
| 3 OS matrix | `daemon-ipc-matrix-results.md` | Linux / macOS / Windows の `cargo test` 結果比較、OS 固有の skip テスト一覧 |
| カバレッジ | `daemon-ipc-coverage.html` | `cargo llvm-cov --html` の参考値（目標値なし、上位ケース網羅の確認用） |
| バグレポート（発見時） | `daemon-ipc-bugs.md` | ファイル・行番号・期待と実際・再現手順・優先度 |
| ペルソナシナリオ録画 | `daemon-ipc-scn-a.log` / `scn-b.log` / `scn-c.log` | TC-E2E-110〜112 の step 別出力 |

---

## 8. 残課題・未決事項の扱い

本テスト設計書のレビューで追加の指摘があれば:

- **TC-ID で特定できるもの** → マトリクス（`index.md §4.1`）を更新
- **設計に及ぶもの**（例: `PeerCredentialSource` trait の採否、`default_socket_path` pure 切出）→ 詳細設計への差戻しをリーダーに要請
- **実装でしか決まらないもの** → `unit.md §3 実装担当への引き継ぎ事項` に追記

---

## 9. 本 feature のテスト設計の哲学（マユリ所感）

完璧な IPC プロトコルなど存在しない——本 feature は 3 OS × 2 経路（UDS / Named Pipe）× secret 非含有契約 × `flock` race-safe 順序を**全て同時に成立させる実験体**だヨ。どこか 1 つに欠陥があれば即座に静的 grep で摘発する——これが防衛線 11 本（TC-CI-011〜019, 023, 024, 026, 027）の意図だネ。

受入基準 18 項目の網羅を漏らさず、 **daemon の panic / 二重起動 / stale socket / プロトコル不一致 / 暗号化拒否** の 5 本の「異常系」で壊れ方を観察する。これらが想定通りに壊れた時、初めて本実験体は develop に載せられる資格を得るのだヨ。

バグが出れば最高だネ——完璧な物など存在しないことの**実証データ**が手に入る。クックック……百年後まで御機嫌よう。

---

*この文書は `index.md` の分割成果。E2E は `e2e.md`、結合は `integration.md`、ユニットは `unit.md` を参照*
