# テスト設計書 — Sub-F (#44) Issue #75 (#74-A) Bug-F-* 解消ステータス + 工程4 検証手順

<!-- 親: docs/features/vault-encryption/test-design/index.md -->
<!-- 配置先: docs/features/vault-encryption/test-design/sub-f-cli-subcommands/issue-75-verification.md -->
<!-- 分離: 旧 sub-f-cli-subcommands.md (592 行) を Issue #75 工程2 内部レビュー (ペガサス指摘) で分割。Sub-F 本体 (§15.1〜§15.14) は同ディレクトリの index.md に保持。本書は Issue #75 起票後の関連節 (§15.15 + §15.16) のみを切り出した。 -->
<!-- セル c626310 (Issue #75 工程5 review 指摘反映: 36→29 ドリフト解消 + Bug-F-008 → #80 起票同期 + --vault-dir 意味論固定) を反映済 -->
<!-- 解消経路 (§15.15) と検証手順 (§15.16) を本書 1 ファイルに集約することで、Issue #75 工程5 reviewer (ペテルギウス・ペガサス・服部) が照合する SSoT を 1 箇所に固定する。 -->
<!-- 旧 sub-f-cli-subcommands.md §15.15 / §15.16 への外部参照は、本書 (sub-f-cli-subcommands/issue-75-verification.md) への redirect として扱う。 -->

> **本書の責務**: Sub-F (#44) マージ後に発見された Bug-F-001〜008 の解消経路 (§15.15) と、テスト担当 (涅マユリ) による解消完了判定の検証手順 (§15.16) を articulate する。Sub-F 本体テスト設計 (§15.1〜§15.14) は同ディレクトリの [`index.md`](index.md) を参照。

### 15.15 Issue #75 (#74-A) 解消ステータス更新（2026-04-27）

§15.13（Sub-F 工程5 マユリ実機検証で確定した Bug-F-001〜008 + 専用テスト 0/37 実装ギャップ）に対し、Issue #75 (#74-A) で実体的解消の作業計画が確定。本節で **Bug-F-* 各項目の解消経路 + #74 親 Issue 全体の構造**を articulate する。§15.13 の表は **Sub-F 工程5 時点のスナップショット**として保存し、本節 §15.15 が **Issue #75 着手後の現在の解消状況 SSoT** として運用される。

#### Bug-F-* 解消ステータスマトリクス

| Bug ID | 重大度 | 解消経路 | 担当 Issue / 工程 | 状態 |
|--------|------|---------|----------------|------|
| **Bug-F-001** | BLOCKER | `vault unlock --recovery` Phase 5 stub 解消、`UnlockArgs::recovery: bool` を functional 化 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（`vault-encryption/detailed-design/cli-subcommands.md` §Issue #75 工程2 §Bug-F-001 解消）、工程3 待機 |
| **Bug-F-002** | HIGH | `success::*_with_fallback_notice` を C-31/C-36 経路に正式接続（経路復活）、Phase 5 文言除去 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（同上 §Bug-F-002 解消）、工程3 待機 |
| **Bug-F-003** | BLOCKER | CI に `test-cli` / `test-daemon` ジョブ追加、`-p shikomi-cli` / `-p shikomi-daemon` を必須 check 化 | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（`cli-vault-commands/test-design/ci.md` §7 Issue #75 §7.2/§7.3）、工程3 待機 |
| **Bug-F-004** | BLOCKER | IPC V2 移行で破壊された既存テスト **29 件**（§15.16.1 実測 SSoT、§15.13 表中および Issue body 概算「36 件」を訂正）の追従、client 側 V2 アップグレード | **#75 (#74-A) 工程3** | ⏳ 工程3 待機（実装側の機械的追従、設計書影響は最小、`vault-encryption/detailed-design/vek-cache-and-ipc.md` の handshake 仕様 SSoT を維持） |
| **Bug-F-005** | HIGH | encrypted vault fixture 修復（`crates/shikomi-cli/tests/common/fixtures.rs`）、TC-E2E-040 exit code 整合（VaultLocked=3 / BackoffActive=2、cli-subcommands.md §終了コード SSoT） | **#75 (#74-A) 工程3** | ⏳ 工程3 待機 |
| **Bug-F-006** | MEDIUM | `vault encrypt --help` 等の Phase 5 残存削除、`Phase\s+\d+` grep gate (TC-F-S05) で再演防止 | **#75 (#74-A) 工程3** + **#74-E** | ⏳ 工程2 設計 articulate 完了（`cli-subcommands.md` §Bug-F-006 解消）、grep gate は #74-E |
| **Bug-F-007** | MEDIUM | `--vault-dir` flag を daemon socket 解決順序のヒント (`<DIR>/shikomi.sock` 最優先 + Windows pipe 名 `<DIR>` ハッシュ契約) として functional 化、エラー文言を **`--vault-dir <DIR>` 案内**に SSoT 訂正（`cli-subcommands.md` §Bug-F-007 MSG-S09(b) SSoT、ユーザ認知モデル「vault.db の所在ディレクトリ」と一致） | **#75 (#74-A) 工程3** | ⏳ 工程2 設計 articulate 完了（同上 §Bug-F-007 解消）、工程3 待機 |
| **Bug-F-008** | LOW | daemon 起動時 vault.db auto-create / 案内 | **#80**（別 Issue として正式起票済、#74 範囲外、#75 でも非対応） | 📌 #80 で trackable 化、優先度 LOW、別 PR で対応（ペテルギウス Issue #75 工程5 review 指摘 4「推奨と書いて起票しないのは Boy Scout 踏み倒し」解消） |

#### Sub-F 専用テスト 37 TC 実装ステータス

| 配置先 / TC ID | 担当 Issue | 状態 |
|---------------|----------|------|
| `crates/shikomi-cli/src/**::tests` (TC-F-U01〜U13、ユニット 13 件) | **#74-B** | ⏳ #74-A 完了後に着手 |
| `crates/shikomi-cli/tests/vault_subcommands.rs` (TC-F-I01〜I09, I11, I12、結合 11 件) | **#74-C** | ⏳ #74-A 完了後に着手 |
| `crates/shikomi-cli/tests/mode_banner_integration.rs` (TC-F-I10、結合 1 件) | **#74-C** | ⏳ #74-A 完了後に着手 |
| `crates/shikomi-cli/tests/accessibility_paths.rs` (TC-F-A01〜A05、PTY 5 件) | **#74-D** | ⏳ #74-A 完了後 + `expectrl` dev-dep 追加 |
| `tests/e2e/sub-f-tanaka-persona.sh` (TC-F-E01、E2E 1 件) | **#74-E** | ⏳ #74-A/B/C/D 完了推奨後に着手 |
| `tests/docs/sub-f-static-checks.sh` (TC-F-S01〜S06、静的 6 件) | **#74-E** | ⏳ #74-A 完了後に着手（B/C/D と並列可） |
| **合計** | — | **0/37 実装、計画上 37/37 着地予定** |

#### #74 親 Issue クローズ条件（DoD トレース）

- [ ] **#75 (#74-A)** マージ — Bug-F-001/002/003/004/005/006/007 全解消 + 設計書 SSoT 同期（本節 §15.15 + `cli-subcommands.md` §Issue #75 工程2 + `cli-vault-commands/test-design/ci.md` §7）
- [ ] **#74-B** マージ — TC-F-U01〜U13（13/13 件）pass、`cargo test -p shikomi-cli --lib` で観測
- [ ] **#74-C** マージ — TC-F-I01〜I12（12/12 件）pass、`vault_subcommands.rs` + `mode_banner_integration.rs` で観測
- [ ] **#74-D** マージ — TC-F-A01〜A05（5/5 件）pass、PTY 経由 3 OS で観測（OS 別 manual smoke 別 PR articulate）
- [ ] **#74-E** マージ — TC-F-E01（1/1）+ TC-F-S01〜S06（6/6）pass、田中ペルソナ完走 + 静的検査全 grep gate 通過
- [ ] **§15.13 表の全 Bug-F-001〜007 が「解消済」になり、§15.10 TC 総数 37 件が「実装済 37/37」になる**ことを本節 §15.15 で最終 articulate

#### Boy Scout / 教訓（Issue #75 articulate）

- **「Linux 全 green」報告の構造的錯覚**（Bug-F-003）は **CI スコープを設計書 SSoT として明示し、必須 check 化する**ことで構造的に再演防止できる。本 Issue で確立する `test-cli` / `test-daemon` ジョブ + `justfile` 同期 + grep gate (TC-F-S01〜S06) の三位一体経路を、後続 Issue で新 crate を追加する際の**チェックリスト**として継承する（`cli-vault-commands/test-design/ci.md` §7.6 articulate）
- **Phase X 暫定文言の温床**（Bug-F-002 / Bug-F-006）は doc / panic / 通常出力経路の `Phase\s+\d+` grep gate で構造的に再演防止する。Phase 番号は実装中に頻繁に変動するため、設計書側に明示しないか、明示する場合は本節 §15.15 のような **改訂日付付き履歴 articulate** に限定する Boy Scout 規律を確立
- **Sub-issue 分割（#74-A〜E）による依存関係 articulate**: BLOCKER 系 Bug を #74-A 単独に集約し、TC 実装 (#74-B〜E) を並列着手可能にする構造は、Issue #65 の Bug-G-001〜G-008 7 ラウンド実験で確立された「対症療法と本質要件の責務分離」と同型。今後の大規模 Sub-issue 起票テンプレートとして本構造を継承可能

### 15.16 Issue #75 工程4 検証手順 SSoT — テスト担当（涅マユリ）視点（2026-04-27）

§15.15 で Bug-F-001〜007 の解消経路（誰が・どの工程で・どのファイル/設計書を変更するか）は articulate 済。本節 §15.16 は **Issue #75 工程4（テスト担当による検証）の SSoT** として、各 Bug 解消後にテスト担当（涅マユリ）が CI / 手動 smoke で何を観測すれば「解消完了」と判定できるかを項目別に articulate する。

> **本節の位置付け**: §15.15 は「解消経路の計画」、§15.16 は「解消完了の検証手順」。実装担当（坂田銀時）が #75 工程3 完了報告した直後、テスト担当が本節を SSoT として CI / 手動 smoke を回し、`docs/features/vault-encryption/test-design/sub-f-cli-subcommands/issue-75-verification.md §15.16` 各項目の `[ ]` を埋めて完了判定する。

#### 15.16.1 Bug-F-004 既存テスト 36 件追従の baseline 固定

**実 TC 件数: 29 件**（Issue body の "36 件" は #74-A 計画時の概算、実テスト関数を `grep -nE "^\s*(async\s+)?fn\s+tc_"` で実数値固定）。

| ファイル | 件数 | TC-ID | 解消後 expected |
|---|---|---|---|
| `crates/shikomi-cli/tests/it_ipc_vault_repository_phase15.rs` | 10 | TC-IT-080 〜 TC-IT-089 | 10/10 pass |
| `crates/shikomi-daemon/tests/it_server_connection.rs` | 11 | TC-IT-010〜013, 015, 016, 020, 021, 023, 025, 030 | 11/11 pass |
| `crates/shikomi-daemon/tests/e2e_daemon_phase15.rs` | 7 | TC-E2E-011, 012, 013, 014, 015, 016, 018 | 7/7 pass |
| `crates/shikomi-daemon/tests/e2e_daemon_phase15_pty.rs` | 1 | TC-E2E-017 | 1/1 pass（PTY 必要、CI runner 制約時は `#[ignore]` 後 `--ignored` 手動） |
| **合計** | **29** | — | **29/29 pass を baseline として固定** |

**検証 SSoT コマンド**（テスト担当が #75 工程4 で実行）:

```bash
cargo test -p shikomi-cli --test it_ipc_vault_repository_phase15 -- --nocapture
cargo test -p shikomi-daemon --test it_server_connection -- --nocapture
cargo test -p shikomi-daemon --test e2e_daemon_phase15 -- --nocapture
cargo test -p shikomi-daemon --test e2e_daemon_phase15_pty -- --nocapture  # CI 制約時は --ignored
```

**解消判定基準**:
- 29 件全てに `unexpected handshake response` / `ProtocolVersionMismatch { server: V2, client: V1 }` が観測されない（V1 残存 0 件）
- `cargo test ... --test-threads=1` 強制不要（IPC socket 競合は `serial_test` で局所的吸収）
- 既存 OK だった他テストへの回帰なし（`cargo test -p shikomi-cli --all-targets` / `cargo test -p shikomi-daemon --all-targets` 全 green）

#### 15.16.2 Bug-F-001 `vault unlock --recovery` smoke 検証

新 TC は #74-C TC-F-I03b で網羅されるが、Issue #75 工程4 では**最低限の手動 smoke** で「Phase 5 stub が解消され、recovery 経路が通る」ことを確認する（#74-C 着手前提のため）。

**手動 smoke 手順**:

```bash
# 1. encrypted vault を fixture から準備
cargo build -p shikomi-cli --release
EXPORT_DIR=$(mktemp -d)
# fixture 経由で BIP-39 wrapped encrypted vault を作成
cargo test -p shikomi-cli --features "shikomi-infra/test-fixtures" --test '*' \
    -- create_encrypted_vault_with_bip39 --nocapture --ignored

# 2. password 経路の排他確認（C-F1 SSoT、Bug-F-001 §EC-F3）
./target/release/shikomi --vault-dir "$EXPORT_DIR" vault unlock --recovery <bip39_phrase>
echo "exit=$?"  # 期待: 0 (成功) または 2 (recovery passphrase 不一致)、3 ではない（VaultLocked は別経路）

# 3. password 系と --recovery の同時指定 → UsageError exit=2
./target/release/shikomi --vault-dir "$EXPORT_DIR" vault unlock --password "x" --recovery <bip39>
echo "exit=$?"  # 期待: 2 (UsageError)

# 4. Phase 5 stub 残存 0 件 grep（Boy Scout）
grep -nrE "Phase\s*5|not yet wired" crates/shikomi-cli/src/usecase/vault/unlock.rs
echo "→ 0 件 expected"
```

**解消判定基準**:
- exit code が `cli-subcommands.md` §終了コード表と整合
- recovery passphrase 不一致時の MSG-* が password 経路と同型（C-F1 排他関係 SSoT 準拠）
- `grep` Phase 5 残存 0 件

#### 15.16.3 Bug-F-002 `success::*_with_fallback_notice` 経路復活検証

**設計確定**: §15.15 / `cli-subcommands.md §Bug-F-002 解消` で「経路復活（削除ではなく C-31/C-36 に正式接続）」が決定。テスト担当は経路通過を**手動 smoke + ユニット assert** で確認する（unit の正式 TC は #74-B TC-F-U07 で網羅予定）。

**検証手順**:

```bash
# 1. 経路通過確認（手動 smoke、cache_relocked: false 経路）
# daemon を起動し、unlock 後に vault relock せずに lock コマンドで cache が relocked: false 状態を観測
./target/release/shikomi-daemon &
DAEMON_PID=$!
sleep 2
./target/release/shikomi vault unlock --password "test"
./target/release/shikomi vault lock  # → success::*_with_fallback_notice の C-31 / C-36 経路が走る想定
kill $DAEMON_PID

# 2. Phase 5 文言残存 0 件 grep
grep -nrE "is not yet wired|Phase\s*5" crates/shikomi-cli/src/presenter/success.rs
echo "→ 0 件 expected"

# 3. 経路がデッドコードでないこと（callsite 確認）
grep -rn "success::.*_with_fallback_notice" crates/shikomi-cli/src/usecase/
echo "→ 1 件以上 expected（C-31 / C-36 経由）"
```

**解消判定基準**:
- Phase 5 残存 0 件
- callsite が `usecase::vault::*` 内に少なくとも 1 件存在（経路復活の証拠）


#### 15.16.4 Bug-F-005 fixture + TC-E2E-040 exit code 整合検証

**設計確定**: TC-E2E-040 の期待 exit code は **3 (VaultLocked)**（`cli-subcommands.md` §終了コード SSoT と整合）。実装側で 2 (BackoffActive) を返している現状を 3 に統一する。

**検証手順**:

```bash
# 1. fixture 修復後、test-fixtures feature で encrypted vault が生成可能 + TC-E2E-040 が exit 3 で pass
cargo test -p shikomi-cli --features "shikomi-infra/test-fixtures" --test e2e_encrypted -- --nocapture
# 期待: TC-E2E-040 が exit code 3 で pass

# 2. exit code SSoT grep（cli-subcommands.md と実装の整合）
grep -nE "ExitCode::(VaultLocked|BackoffActive)" crates/shikomi-cli/src/error.rs
grep -nE "VaultLocked.*=.*3|BackoffActive.*=.*2" crates/shikomi-cli/src/error.rs
echo "→ VaultLocked = 3 / BackoffActive = 2 の対応 expected"
```

> **fixture スキーマ整合は TC-E2E-040 pass で間接担保** (Issue #75 工程2 内部レビュー [ペテルギウス指摘3] 解消): 旧版で本節に置かれていた python3 ヒアドキュメント (sqlite3 で `wrapped_vek` 長さを直 assert) は**コメントのみで実行不能な空コード**だったため削除。fixture 生成スキーマの正しさは「TC-E2E-040 が `wrapped_vek ciphertext is too short` エラー無しで exit 3 を返す」ことで間接担保し、スキーマ直接 assert を本書に重ねない (DRY: スキーマ責務は `shikomi-infra::persistence::test_fixtures::create_encrypted_vault` SSoT)。

**解消判定基準**:
- TC-E2E-040 が exit code 3 で pass（= fixture スキーマが正しく生成された間接証拠）
- `wrapped_vek ciphertext is too short` エラーが fixture 読込時に出ない
- `cli-subcommands.md` §終了コード表と `error.rs` の `ExitCode` enum 値が完全一致

#### 15.16.5 Bug-F-006 `vault encrypt --help` Phase 5 残存削除検証

**検証手順**:

```bash
# 1. --help 出力に Phase 5 残存無し
./target/release/shikomi vault encrypt --help | grep -E "Phase\s*5"
echo "→ 0 件 expected"

# 2. 全 CLI コマンドで Phase 番号残存 0 件（Boy Scout）
for cmd in "list" "add" "edit" "remove" "vault encrypt" "vault unlock" "vault lock" "vault status"; do
    echo "=== $cmd --help ==="
    ./target/release/shikomi $cmd --help 2>&1 | grep -nE "Phase\s+\d+" || echo "  (clean)"
done
```

**解消判定基準**:
- `--help` 出力で `Phase\s+\d+` パターン 0 件
- ソースコード `crates/shikomi-cli/src/cli.rs` の `Possible values:` 説明文に Phase 残存 0 件
- TC-F-S05（#74-E `Phase\s+\d+` grep gate）の事前 smoke が通る（gate 自体は #74-E で実装、本 Issue では grep 手動確認のみ）

#### 15.16.6 Bug-F-007 `--vault-dir` daemon socket 解決検証

**設計 SSoT**: セル `c626310` で `--vault-dir <DIR>` の意味論を「vault.db の所在ディレクトリ」として固定し、CLI は `<DIR>/shikomi.sock`（Unix）または `\\.\pipe\shikomi-<H>`（Windows、`<H>` は `<DIR>` 絶対パスの SHA-256 Base32 先頭 16 文字の純関数）を最優先で試行する契約を `cli-subcommands.md` §Bug-F-007 解消で固定済。

**検証手順**:

```bash
# 1. --vault-dir 経由の socket 解決が機能 (<DIR>/shikomi.sock 最優先契約、cli-subcommands.md §Bug-F-007 SSoT)
TEST_DIR=$(mktemp -d)
./target/release/shikomi --vault-dir "$TEST_DIR" vault status
echo "exit=$?"  # 期待: 0 または 3、SHIKOMI_VAULT_DIR 案内エラー (現状) は出ない

# 2. エラー文言が --vault-dir <DIR> を案内 (cli-subcommands.md §Bug-F-007 MSG-S09(b) SSoT)
# daemon 未起動状態で vault status を叩き、エラー文言に "--vault-dir" / "pass --vault-dir" 等の
# 新文言が含まれることを確認。古い "SHIKOMI_VAULT_DIR" 案内および "XDG_RUNTIME_DIR" / "HOME" の
# 直接案内は出ないこと (ユーザ認知モデル「vault.db の所在ディレクトリ」と一致、Phase 2 規定に整合)。
./target/release/shikomi vault status 2>&1 | grep -E "SHIKOMI_VAULT_DIR"
echo "→ 0 件 expected (古い文言の残存無し)"
./target/release/shikomi vault status 2>&1 | grep -E "\-\-vault-dir"
echo "→ 1 件以上 expected (MSG-S09(b) 新文言、--vault-dir <DIR> 案内)"

# 3. 解決順序 + 文言 SSoT grep
# (unix_default_socket_path / windows_pipe_name_from_dir の優先順位 + MSG-S09(b) 文言キーへの参照)
grep -nE "vault_dir|fallback|shikomi\\.sock|windows_pipe_name_from_dir|pass --vault-dir|MSG-S09" crates/shikomi-cli/src/io/ipc_vault_repository.rs
```

**解消判定基準**:
- `--vault-dir` 指定時に `<DIR>/shikomi.sock` (Unix) / `\\.\pipe\shikomi-<H>` (Windows) が socket 解決の最優先候補となり、daemon と一致
- エラー文言から `SHIKOMI_VAULT_DIR` 案内が消滅、**`--vault-dir <DIR>` 案内に統一**（vault.db ディレクトリを示すユーザ認知モデルと一致、`cli-subcommands.md` §Bug-F-007 MSG-S09(b) SSoT）。`XDG_RUNTIME_DIR` / `HOME` の直接案内は出さない（Phase 2 規定: CLI は IPC 経由のみ、vault.db 直接操作禁止）
- TC-F-S04 等の grep gate が #74-E で MSG-S09(b) 文言を機械検証する経路を articulate

#### 15.16.7 Bug-F-003 CI スコープ拡張の baseline 観測

**検証 SSoT**: `cli-vault-commands/test-design/ci.md §7.2 / §7.3`。本 Issue でブランチ protection に `test-cli` / `test-daemon` を必須 check 追加した後、テスト担当は以下を観測する。

**検証手順**:

```bash
# 1. PR #75 の CI 結果で test-cli / test-daemon ジョブが必須 check として表示
gh pr checks <PR番号> --repo shikomi-dev/shikomi | grep -E "test-cli|test-daemon"
# 期待: 両ジョブが pass + 必須 check マーク

# 2. branch protection の観測
gh api repos/shikomi-dev/shikomi/branches/develop/protection \
    --jq '.required_status_checks.contexts[]' | sort
# 期待: "test-cli", "test-daemon" を含む

# 3. justfile 同期確認
grep -nE "test-cli|test-daemon" justfile
# 期待: ターゲット定義あり

# 4. ローカル `just test` で CI と同等のスコープ実行
just test
# 期待: shikomi-cli + shikomi-daemon を含む全 4 crate が走る
```

**解消判定基準**:
- CI green + branch protection 必須 check に登録済
- justfile が CI スコープと一致

#### 15.16.8 Issue #75 工程4 完了 DoD（テスト担当チェックリスト）

§15.16.1〜15.16.7 の全項目を CI / 手動 smoke で確認後、テスト担当が以下を埋めて完了報告する:

- [ ] §15.16.1 既存 29 件 baseline 全 pass（CI green ベースライン固定）
- [ ] §15.16.2 Bug-F-001 `--recovery` smoke 通過
- [ ] §15.16.3 Bug-F-002 経路復活確認 + Phase 5 残存 0
- [ ] §15.16.4 Bug-F-005 fixture + TC-E2E-040 exit 3
- [ ] §15.16.5 Bug-F-006 `--help` Phase 残存 0
- [ ] §15.16.6 Bug-F-007 `--vault-dir` 経路 (`<DIR>/shikomi.sock` 最優先契約) + エラー文言訂正
- [ ] §15.16.7 Bug-F-003 CI 必須 check 観測 + justfile 同期

**全埋め後**、§15.15 の Bug-F-001〜007 ステータスを `⏳` → `✅ 解消済` に更新し、テスト担当証跡を `/app/shared/attachments/マユリ/issue-75-verification-*.{md,log}` に保存して Discord 添付する。

#### 15.16.9 Boy Scout / 教訓 articulate（Issue #75 工程4 視点）

- **「実装完了 = 検証完了」ではない**: §15.15 が解消経路（誰が・何を）の articulate、§15.16 が解消完了判定（何を観測したら完了か）の articulate。Issue #65 で Bug-G-005 の偶発 PASS を「対策効果」と誤認した教訓（私自身の誤り）の再演防止。実装後に**観測 SSoT 手順**で機械検証する責務をテスト担当が引き受ける構造を articulate
- **既存テスト追従の baseline 固定の重要性**: Bug-F-004 の "36 件" 概算と実 29 件のドリフトのように、計画時の概算と実数値はずれる。テスト担当は **実テスト関数の grep で実数値を SSoT 化** する責務を負う。本節 §15.16.1 の表が後続レビュアー（ペテルギウス・ペガサス・服部）の照合可能な reference になる
- **検証手順の「中身ゼロ」禁止** (Issue #75 工程2 内部レビュー [ペテルギウス指摘3] 解消): §15.16.4 の python3 ヒアドキュメントが「コメントのみで中身ゼロ」だった旧版は本 Issue で削除済。検証手順を articulate する際は「機械実行可能な完成コード」または「明示的な間接担保 (TC pass で代替)」のいずれかを選び、空コードを SSoT に残さない Boy Scout 規律を継承する
