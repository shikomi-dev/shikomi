# テスト設計書 — daemon-ipc / E2E

> `index.md` の §2 索引からの分割ファイル。E2E テスト全 TC とペルソナシナリオ + 証跡方針を扱う。

## 1. ツール選択根拠

| 候補 | 採用可否 | 理由 |
|------|---------|------|
| `assert_cmd` + `predicates` + `tempfile` | **採用** | `cli-vault-commands` E2E と同手法。`assert_cmd::Command::cargo_bin("shikomi-daemon")` / `cargo_bin("shikomi")` で本物のバイナリを呼ぶ完全ブラックボックス。`.env()` で環境変数注入、`.stdin(Stdio::piped())` で非 TTY 化 |
| `tokio::process::Command` | 限定採用 | daemon を async で spawn して `Child` を kill する用途。`assert_cmd` は sync なので daemon spawn と CLI 実行を 2 経路に分ける際に併用（`tokio::process::Command::new(cargo_bin("shikomi-daemon")).spawn()?` + `assert_cmd::Command::cargo_bin("shikomi")` の組合せ） |
| Playwright | 不採用 | Web UI 用。対象外 |

## 2. テスト共通前提

- **daemon プロセスの spawn**: `tokio::process::Command::new(assert_cmd::cargo::cargo_bin("shikomi-daemon"))` で起動 → stdout/stderr を `Stdio::piped()` で捕捉 → 起動完了を `"listening on"` ログで検知（`BufReader::lines()` で stdout を 5 秒以内に読む、タイムアウトで fail）
- **一意なソケット / pipe パス**: 各テストで `tempfile::TempDir` を作り `SHIKOMI_VAULT_DIR=<tmp>` を注入。Unix は `SHIKOMI_DAEMON_SOCKET_DIR=<tmp>`（本 feature で daemon に env 追加を推奨、後述）、Windows は pipe 名に pid / uuid suffix
  - **設計上の追加**: daemon にソケットパス上書き用 env `SHIKOMI_DAEMON_SOCKET_DIR`（Unix）または `SHIKOMI_DAEMON_PIPE_NAME`（Windows）を**テスト専用**で追加することを実装担当に推奨。本番では未使用（ユーザ向けは既定パスのみ、`requirements.md §shikomi-daemon バイナリの起動`）
- **daemon のクリーンアップ**: テスト終了時に `child.kill().await` or `SIGTERM` 送信 → `child.wait()` で exit 確認。`Drop` impl で自動 kill する `DaemonGuard` ヘルパーを `tests/common/daemon_guard.rs` に置く
- **secret マーカー**: `SECRET_TEST_VALUE` 固定。全 stdout / stderr / daemon stdout / daemon stderr で `contains("SECRET_TEST_VALUE").not()` を**横串アサート**
- **exit code**: `.code(N)` で厳密一致
- **bit 同一比較**: `shikomi list` vs `shikomi --ipc list` の stdout を `assert_eq!(stdout_direct, stdout_ipc)` で strict compare（列順序・改行・スペース全一致）

---

## 3. 基本動作（単一 daemon）

### TC-E2E-001: daemon 起動 → listen ログ確認（Linux / macOS）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1 |
| 対応 REQ | REQ-DAEMON-001, REQ-DAEMON-004 |
| 種別 | 正常系 |
| 前提条件 | `tempfile::TempDir` 内に空 vault dir |
| 操作 | `shikomi-daemon` バイナリを spawn（env `SHIKOMI_VAULT_DIR=<tmp>`、`SHIKOMI_DAEMON_LOG=info`） |
| 期待結果 | 5 秒以内に stdout に `listening on ` を含む行、ソケットファイル `<tmp>/daemon.sock`（または `$XDG_RUNTIME_DIR/shikomi/daemon.sock`）が存在、パーミッション `0600` |
| 検証アサート | `std::fs::metadata(&sock).mode() & 0o777 == 0o600` + 親ディレクトリ `0700` |

### TC-E2E-002: daemon 起動（Windows）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 1, 9 |
| 対応 REQ | REQ-DAEMON-003, REQ-DAEMON-004 |
| 種別 | 正常系 |
| 前提条件 | 空 vault dir |
| 操作 | `shikomi-daemon` を spawn（`SHIKOMI_DAEMON_PIPE_NAME=\\.\pipe\shikomi-test-{uuid}`） |
| 期待結果 | stdout に `listening on \\.\pipe\shikomi-test-{uuid}`、`GetNamedSecurityInfoW` で DACL を取得し owner-only（ACE 1 個 + 所有者 SID） |
| 検証アサート | Windows crate 経由で DACL 列挙、owner SID のみ |

---

## 4. `--ipc` 経路での CRUD

### TC-E2E-010: `shikomi --ipc list` vs `shikomi list` bit 同一比較

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 2 |
| 対応 REQ | REQ-DAEMON-007, REQ-DAEMON-015, REQ-DAEMON-016 |
| 種別 | 正常系（MVP 核） |
| 前提条件 | 事前に `shikomi --vault-dir <tmp> add` で Text 2 + Secret 1 を投入（SQLite 直結で作成）。daemon を起動し同じ vault dir を読む |
| 操作 | (1) `shikomi --vault-dir <tmp> list` で stdout 取得 `S1` / (2) daemon 起動後 `shikomi --vault-dir <tmp> --ipc list` で stdout 取得 `S2` |
| 期待結果 | `S1 == S2`（bit 同一、改行含む strict）、両方に Secret の `****` マスクが出現、`SECRET_TEST_VALUE` 非含有 |
| 検証アサート | `assert_eq!(S1, S2)` + `contains("****").and(contains("SECRET_TEST_VALUE").not())` |

### TC-E2E-011: `shikomi --ipc add --kind text` + list 反映

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 2 |
| 対応 REQ | REQ-DAEMON-008 |
| 種別 | 正常系 |
| 前提条件 | daemon 起動済、空 vault |
| 操作 | `shikomi --ipc --vault-dir <tmp> add --kind text --label L --value V` → `--ipc list` |
| 期待結果 | add exit 0、stdout に `added: <uuid>`、list で当該 uuid + L + V が出る |

### TC-E2E-012: `shikomi --ipc add --kind secret --stdin` で secret 露出ゼロ

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 17 |
| 対応 REQ | REQ-DAEMON-008, REQ-DAEMON-020 |
| 種別 | セキュリティ |
| 前提条件 | daemon 起動済 |
| 操作 | `echo "SECRET_TEST_VALUE" \| shikomi --ipc --vault-dir <tmp> add --kind secret --label S --stdin` |
| 期待結果 | exit 0、CLI stdout/stderr + **daemon の stdout/stderr**（`tracing` ログ含む）全てで `SECRET_TEST_VALUE` 非含有 |
| 検証アサート | `client.stdout().contains("SECRET_TEST_VALUE").not()` + `client.stderr().contains("SECRET_TEST_VALUE").not()` + daemon の captured stdout/stderr にも同様 |

### TC-E2E-013: `shikomi --ipc edit --label NEW`

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 2 |
| 対応 REQ | REQ-DAEMON-009 |
| 種別 | 正常系 |
| 操作 | 事前追加 → `--ipc edit --id <uuid> --label NEW` → `--ipc list` |
| 期待結果 | list に NEW ラベルで表示される |

### TC-E2E-014: `shikomi --ipc remove --yes`

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 2 |
| 対応 REQ | REQ-DAEMON-010 |
| 種別 | 正常系 |
| 操作 | 事前追加 → `--ipc remove --id <uuid> --yes` → `--ipc list` |
| 期待結果 | 削除成功、list に当該 uuid が出ない |

### TC-E2E-015: `shikomi --ipc edit --id <非存在>`

| 項目 | 内容 |
|------|------|
| 対応受入基準 | — |
| 対応 REQ | REQ-DAEMON-009, REQ-DAEMON-021 |
| 種別 | 異常系 |
| 操作 | `--ipc edit --id 00000000-0000-0000-0000-000000000000 --label X` |
| 期待結果 | daemon が `IpcResponse::Error(NotFound)` → CLI が `MSG-CLI-106` または相当 + exit 1 |

---

## 5. シングルインスタンス / stale socket

### TC-E2E-020: daemon 二重起動が exit 2

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 3 |
| 対応 REQ | REQ-DAEMON-002, REQ-DAEMON-003 |
| 種別 | 異常系（起動競合） |
| 前提条件 | daemon A を起動、listening 確認 |
| 操作 | 同じ `SHIKOMI_DAEMON_SOCKET_DIR` で daemon B を spawn |
| 期待結果 | B が **exit code 2**（`SingleInstanceUnavailable`）、stderr に `another daemon is running` または同意のメッセージ / A は継続 |
| 検証アサート | `B.wait().await?.code() == Some(2)` + A の stdout が続いて出る |

### TC-E2E-021: stale socket 存在下での初回起動

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 4 |
| 対応 REQ | REQ-DAEMON-002 |
| 種別 | 正常系（race-safe） |
| 前提条件 | `<tmp>/shikomi/` を事前作成、`daemon.sock` を手動で touch（ファイル存在だが listen なし） |
| 操作 | daemon 起動 |
| 期待結果 | daemon が `flock → unlink → bind` の 3 段でソケットを再作成、exit せず listen ログが出る |

### TC-E2E-022: SIGKILL された daemon の残留でも次起動成功

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 4 |
| 対応 REQ | REQ-DAEMON-002 |
| 種別 | 正常系（カーネル flock 自動解放） |
| 前提条件 | daemon A を起動 → SIGKILL で強制終了（Drop ハンドラ不発火） |
| 操作 | 同じ `SHIKOMI_DAEMON_SOCKET_DIR` で daemon B を起動 |
| 期待結果 | B が listening に到達（`flock` は OS が A の終了時に release 済み、`daemon.lock` / `daemon.sock` が残存していても獲得可能） |

---

## 6. graceful shutdown

### TC-E2E-030: SIGTERM で graceful shutdown + ソケット削除

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5 |
| 対応 REQ | REQ-DAEMON-014 |
| 種別 | 正常系 |
| 前提条件 | daemon 起動済み、`shikomi --ipc list` でハンドシェイク完了後の接続 1 本（放置） |
| 操作 | daemon に SIGTERM（Unix）/ `taskkill /PID /T`（Windows）送信 |
| 期待結果 | exit 0、ソケットファイル削除（Unix）または pipe close（Windows）、stdout に `graceful shutdown complete` |
| 検証アサート | `!sock.exists()` + exit code 0 |

### TC-E2E-031: shutdown 中の in-flight Add 完了

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 5 |
| 対応 REQ | REQ-DAEMON-014 |
| 種別 | 正常系（in-flight 保護） |
| 前提条件 | daemon 起動済み |
| 操作 | (1) 別プロセスで `shikomi --ipc add --kind text --label L --value V` を spawn（応答待機中） / (2) 直後に daemon に SIGTERM |
| 期待結果 | add は exit 0 で完了（`added: <uuid>` を出す）、その後 daemon が graceful shutdown |
| 検証アサート | `add.wait().code() == Some(0)` かつ `add.stdout.contains("added: ")` |

---

## 7. プロトコル不一致

### TC-E2E-040: クライアントが V99 を送った場合（offline fabrication）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 6 |
| 対応 REQ | REQ-DAEMON-006, REQ-DAEMON-019 |
| 種別 | 異常系 |
| 前提条件 | daemon 起動済 |
| 操作 | **バイナリ改変ではなく test-only クライアントスタブ**: `crates/shikomi-daemon/tests/e2e_protocol_mismatch.rs` 内で `tokio::net::UnixStream::connect` → 手動で `IpcRequest::Handshake` を V99 相当のバイト列（`rmp_serde::to_vec(&{"handshake": {"client_version": "v99"}})` 等）で送る |
| 期待結果 | daemon が `IpcResponse::ProtocolVersionMismatch { server: V1, client: V99 }` を返して切断、stub が受信成功、daemon stderr に `protocol mismatch` ログ |
| 検証アサート | 受信 frame の decode で `ProtocolVersionMismatch` variant + `server == V1` |

### TC-E2E-041: CLI が `MSG-CLI-111` を出す（実 CLI + 偽 daemon スタブ）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 6 |
| 対応 REQ | REQ-DAEMON-017 |
| 種別 | 異常系（CLI 側観測） |
| 前提条件 | test-only 偽 daemon スタブを立て、`ProtocolVersionMismatch { server: V2, client: V1 }` を即返す（将来の V2 daemon vs V1 client 相当の模擬） |
| 操作 | `shikomi --ipc --vault-dir <tmp> list` |
| 期待結果 | exit 1、stderr に `MSG-CLI-111` 相当（英語: `protocol version mismatch (server=v2, client=v1)`）、`hint: rebuild shikomi-cli and shikomi-daemon` |

---

## 8. パーミッション検証

### TC-E2E-050: UDS `0600` / 親ディレクトリ `0700`

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 8 |
| 対応 REQ | REQ-DAEMON-004 |
| 種別 | セキュリティ |
| 操作 | daemon 起動後、`std::fs::metadata(&sock).permissions().mode()` + 親ディレクトリ mode を取得 |
| 期待結果 | socket `0600`、親 `0700` |

### TC-E2E-051: Named Pipe DACL owner-only

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 9 |
| 対応 REQ | REQ-DAEMON-003 |
| 種別 | セキュリティ（Windows のみ） |
| 操作 | daemon 起動後、`GetNamedSecurityInfoW` で DACL を取得し ACE 列挙 |
| 期待結果 | DACL に所有者 SID のみの ACE、Everyone / Anonymous / NetworkService は不在 |

### TC-E2E-060: 別ユーザから UDS 接続拒否（Linux）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 10 |
| 対応 REQ | REQ-DAEMON-005 |
| 種別 | セキュリティ |
| 前提条件 | daemon を user A で起動、`sudo -u nobody` で user B から接続試行。CI 環境では `sudo` 使用が困難なため **`#[ignore]` 付与**、ローカル手動検証 |
| 操作 | `sudo -u nobody shikomi --ipc --vault-dir <tmp> list` |
| 期待結果 | 接続は OS レイヤで `EACCES`（UDS 0600）→ CLI が `DaemonNotRunning` 扱いで exit 1 / または接続できた場合（パーミッションが甘い場合）は daemon がピア UID 不一致で即切断、CLI は I/O エラーで exit 1 |

---

## 9. 暗号化 vault（Fail Fast）

### TC-E2E-070: 暗号化 vault → daemon 起動失敗 exit 3

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 11 |
| 対応 REQ | REQ-DAEMON-013 |
| 種別 | Fail Fast |
| 前提条件 | `shikomi-infra` の `test-fixtures::create_encrypted_vault(&tmp)` で暗号化ヘッダのみ持つ vault を事前作成 |
| 操作 | daemon を spawn（`SHIKOMI_VAULT_DIR=<tmp>`） |
| 期待結果 | daemon が exit 3、stderr に `vault is encrypted; daemon does not support encrypted vaults yet`、ソケット非作成 |

### TC-E2E-071: `--ipc add` を暗号化 vault へ発行した場合（防御的）

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 11 |
| 対応 REQ | REQ-DAEMON-013 |
| 種別 | 異常系 |
| 前提条件 | daemon 起動時は平文だが、**起動後**に別プロセスで vault を再暗号化（競合シナリオ、後続 Issue 想定）。本 TC では **平文で起動した daemon のハンドラ防御コード**を検証するため、test-only な `--vault-protection-mode encrypted` フラグを daemon に追加するか、mock repo で代替 |
| 代替案 | daemon 側のハンドラ防御的検査はユニット TC-UT-039 / IT TC-IT-015 で網羅。本 E2E は **scope-out**（`#[ignore]`） |

---

## 10. 単一ルート化 / コンポジションルート

### TC-E2E-080: `--ipc` 分岐による経路切替

| 項目 | 内容 |
|------|------|
| 対応受入基準 | 16 |
| 対応 REQ | REQ-DAEMON-016 |
| 種別 | 正常系（Clean Arch 体現） |
| 操作 | 同一 vault dir で `shikomi list`（直結）と `shikomi --ipc list`（daemon 経由）を交互に実行 |
| 期待結果 | 両経路で bit 同一出力（TC-E2E-010 と重複するが、UseCase / Presenter 不変の assert として独立化）、**`shikomi` バイナリの同一プロセスで `--ipc` の有無を切り替えて動作**すること（別々プロセスでの独立実行） |

---

## 11. ペルソナシナリオ

`../requirements-analysis.md §ペルソナ` 由来の**ユーザ視点シナリオ**。

### TC-E2E-110: SCN-A（野木 拓海 — 後続 feature 実装担当）

**シナリオ**: daemon を手動起動、疎通確認、graceful shutdown を経験する。

| ステップ | 操作 | 期待観測 |
|--------|------|-------|
| 1 | `shikomi-daemon &` で起動 | stdout に `listening on {path}` |
| 2 | `shikomi --ipc list` | exit 0、空 vault なら "(no records)" |
| 3 | `shikomi --ipc add --kind text --label L --value V` | `added: <uuid>` |
| 4 | `shikomi --ipc list` | 追加値が反映 |
| 5 | daemon に SIGTERM | daemon stdout に `graceful shutdown complete`、exit 0、ソケット削除 |
| 6 | `shikomi --ipc list` | `MSG-CLI-110`（daemon not running）、exit 1 |

### TC-E2E-111: SCN-B（山田 美咲 — 日常ユーザ兼 OSS コントリビュータ）

**シナリオ**: Phase 1（直結）と Phase 2（`--ipc`）の**挙動同一性**を自分の環境で確認する。

| ステップ | 操作 | 期待観測 |
|--------|------|-------|
| 1 | `shikomi add --kind text --label "ssh-a" --value "ssh user@host"` × 3 件（直結）| 各 exit 0 |
| 2 | `shikomi list` → `$S1` | Text 3 件 |
| 3 | `shikomi-daemon &` 起動 | listen OK |
| 4 | `shikomi --ipc list` → `$S2` | `$S1` と bit 同一 |
| 5 | `shikomi --ipc add --kind text --label "ssh-d" --value "new"` | exit 0 |
| 6 | daemon 停止 → `shikomi list` | step 5 の新規 record も含む 4 件（daemon が SQLite に書いたため直結経路からも見える） |

### TC-E2E-112: SCN-C（志摩 凛 — OSS コントリビュータ、初回接触）

**シナリオ**: `--ipc` を試して daemon 未起動エラーを経験 → ヒントに従って起動 → 成功する、の 1 サイクル。

| ステップ | 操作 | 期待観測 |
|--------|------|-------|
| 1 | `shikomi --ipc list`（daemon 未起動） | exit 1、stderr に `MSG-CLI-110` 英日 2 段、`hint: start the daemon` |
| 2 | ヒントに従い `shikomi-daemon &` | listen OK |
| 3 | `shikomi --ipc list` 再実行 | exit 0、空 vault メッセージ |

---

## 12. テストコード配置

```
crates/shikomi-daemon/tests/
  common/
    mod.rs                          # spawn_daemon(tmp) -> DaemonGuard, kill_daemon(), wait_for_listening()
    daemon_guard.rs                 # Drop で kill する RAII
  e2e_startup.rs                    # TC-E2E-001, 002
  e2e_single_instance.rs            # TC-E2E-020, 021, 022
  e2e_shutdown.rs                   # TC-E2E-030, 031
  e2e_encrypted.rs                  # TC-E2E-070, 071
  e2e_permissions.rs                # TC-E2E-050, 051
  e2e_protocol_mismatch.rs          # TC-E2E-040, 041
  e2e_peer_credential_linux.rs      # TC-E2E-060（#[ignore]）

crates/shikomi-cli/tests/
  e2e_ipc_crud.rs                   # TC-E2E-010〜015
  e2e_ipc_composition.rs            # TC-E2E-080
  e2e_ipc_scenarios.rs              # TC-E2E-110〜112
```

---

## 13. 証跡提出方針

テスト戦略ガイド準拠。`/app/shared/attachments/マユリ/` に保存で Discord 添付。

| 種別 | ファイル名 | 内容 |
|------|----------|------|
| E2E 実行ログ | `daemon-ipc-e2e-report.md` | TC-E2E-001〜112 の `assert_cmd` 出力（stdout/stderr/exit code/diff）+ `SECRET_TEST_VALUE` 不在 grep 結果 + 3 OS matrix の結果まとめ |
| ペルソナシナリオ録画 | `daemon-ipc-scn-a.log` / `scn-b.log` / `scn-c.log` | 各シナリオの全ステップの shell 出力（`script` コマンドで記録） |
| パーミッション監査 | `daemon-ipc-permissions.md` | TC-E2E-050（UDS mode）/ TC-E2E-051（Windows DACL ACE list） |
| プロトコル不一致シミュレーション | `daemon-ipc-protocol-mismatch.md` | TC-E2E-040 のフレーム 16 進ダンプ + receive 値 |
| graceful shutdown 観測 | `daemon-ipc-shutdown.md` | SIGTERM → 完了までのタイムスタンプ差、in-flight add の応答時刻 |
| バグレポート（発見時） | `daemon-ipc-bugs.md` | ファイル名・行番号・期待動作・実際動作・再現手順 |

---

## 14. 実行コマンド

```bash
# E2E（3 OS 共通）
cargo test -p shikomi-daemon --test 'e2e_*'
cargo test -p shikomi-cli --test 'e2e_ipc_*'

# 暗号化 vault フィクスチャ
cargo test -p shikomi-daemon --features "shikomi-infra/test-fixtures" --test e2e_encrypted

# Linux 専用（別ユーザ接続拒否）
cargo test -p shikomi-daemon --test e2e_peer_credential_linux -- --ignored

# bit 同一比較の精査（TC-E2E-010）
cargo test -p shikomi-cli --test e2e_ipc_crud bit_identical
```

---

## 15. 人間が動作確認できるタイミング

実装完了後、以下のコマンドで **初めて daemon 経由の shikomi が実機で動作**する。README "Try it" に追記推奨:

```bash
# ビルド
cargo build -p shikomi-daemon -p shikomi-cli --release

# daemon 起動（端末 1）
./target/release/shikomi-daemon
# => "shikomi-daemon listening on /run/user/1000/shikomi/daemon.sock"

# CLI 実行（端末 2）
./target/release/shikomi --ipc list
./target/release/shikomi --ipc add --kind text --label "via-ipc" --value "hello"
./target/release/shikomi --ipc list
echo "super-secret" \| ./target/release/shikomi --ipc add --kind secret --label "api" --stdin

# 直結経路と bit 同一であることを確認
diff <(./target/release/shikomi list) <(./target/release/shikomi --ipc list)

# 停止（端末 1）
# Ctrl+C => "graceful shutdown complete" → exit 0
```

これが Issue #26 完了後、**後続 feature（ホットキー / 暗号化 / GUI）が daemon 上に積める**最初のマイルストーン。

---

*この文書は `index.md` の分割成果。結合は `integration.md`、ユニットは `unit.md`、CI は `ci.md` を参照*
