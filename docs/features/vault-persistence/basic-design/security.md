# 基本設計書 — security（セキュリティ設計 / 監査ログ / vault ディレクトリ検証 / Windows DACL / OWASP）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/basic-design/security.md -->
<!-- 兄弟: ./index.md（モジュール / クラス / フロー）, ./error.md（エラーハンドリング方針） -->

## 記述ルール（必ず守ること）

基本設計に**疑似コード・サンプル実装（python/ts/go等の言語コードブロック）を書くな**。
ソースコードと二重管理になりメンテナンスコストしか生まない。

## セキュリティ設計

本 Issue は**パスワードと認証情報を初めてディスクに落とす層**であり、永続化境界の防御はここで決まる。`docs/architecture/context/threat-model.md` §7 / §7.0 / §7.1 に沿って脅威モデルを具体化する。

### 脅威モデル

| 想定攻撃者 | 攻撃経路 | 保護資産 | 対策 |
|-----------|---------|---------|------|
| 同ユーザ内の他プロセス（マルウェア等） | `~/.local/share/shikomi/vault.db` を直接 open | レコード平文（平文モード） | **OS パーミッション `0600` / ACL 所有者のみ**。REQ-P06 / P07 で強制・検証。ただし平文モードの残存リスク（§7.0）は暗号化オプトインで解除する設計判断としてユーザに明示 |
| 別ユーザ / root | 同上 | 同上 | 平文モードでは防御不能（§7.0 で明記）。暗号化モード（別 Issue）で対応 |
| 悪意あるスクリプトによる vault.db 差替え | `vault.db` を偽ファイルで上書き | レコード完全性 | 平文モードでは暗号学的改竄検出不可（AEAD がない、§7.0）。`PRAGMA application_id` で「shikomi 形式でない」ファイルは拒否するが、同形式の偽 vault は検出できない（受容リスク） |
| 電源断・クラッシュ | save 中の強制終了 | vault.db 本体の可用性 | **atomic write**（REQ-P04）: `.new` → fsync → rename。rename は POSIX atomic、`ReplaceFileW` は同一ボリューム内 atomic |
| 前回 save の `.new` 残存 | 前回クラッシュ痕 | vault.db の一貫性（古い vs 新しい） | **起動時検出**（REQ-P05）: `.new` が残っていたら `load` は `OrphanNewFile` を返し、ユーザ明示操作を待つ（Fail Secure、勝手に削除しない） |
| SQL インジェクション | 悪意ある label 文字列が SQL に連結される | vault.db 破壊・情報漏洩 | **parameter binding のみ**（REQ-P12）: `rusqlite::params!` マクロで全値をバインド。生 SQL 連結は全面禁止、grep と lint で検証 |
| ファイル差替えによる権限昇格 | 攻撃者が `0777` の vault.db を置く | データ改竄・読取 | 起動時のパーミッション検証（REQ-P06）で `0600` 以外を拒否。攻撃者が正しいパーミッションで作り直しても上記「悪意あるスクリプト」の枠（§7.0） |
| テンポラリファイル経由のレース | `.new` 作成から rename までの間に攻撃者が介入 | vault.db 差替え | `.new` は作成時に `0600` / ACL 所有者のみ。属しないトラスティが書込不可であることで TOCTOU を狭める。rename 自体は atomic |
| 起動時のドメイン整合性違反 | vault.db の行が壊れている | ドメイン不変条件 | **復元時検証**（REQ-P09）: 全 newtype の `try_new` を通す。`RecordId` / `RecordLabel` / `VaultHeader` / `RecordPayloadEncrypted` / `NonceBytes` / `KdfSalt` / `WrappedVek` 全て検証済み型でしか `Vault` に入らない |
| `SHIKOMI_VAULT_DIR` の悪用 | 環境変数で `../../etc` / `/proc/self/root` / シンボリックリンクを指定 | システム保護領域・任意ディレクトリへの書込、TOCTOU 差替え | **`VaultPaths::new` バリデーション**（§vault ディレクトリ検証）: 絶対パス必須、`..` 早期拒否、シンボリックリンク全面禁止、`canonicalize` 後の保護領域 prefix 一致拒否、ディレクトリ判定 |
| 並行書込レース（daemon 未起動時） | CLI / リカバリツール / 別 CLI が同時に `save` を呼ぶ | vault.db 壊れ、`.new` 錯綜 | **プロセス間 advisory lock**（`VaultLock::acquire_exclusive`）: `fs4` / `LockFileEx` で非ブロッキング排他取得、失敗時は `Locked { holder_hint }` で即 return（待機・再試行しない、Fail Fast） |
| ログ経由の秘密漏洩 | 開発者がデバッグで vault 内容を `tracing::info!("{:?}", record)` してしまう | plaintext_value / ciphertext / VEK が journal に流れる | 多層防御 — ①`SecretString`/`SecretBytes` の `Debug` は `"[REDACTED]"`（Issue #7）、②`audit.rs` 経由以外の tracing 呼出を clippy lint で禁止、③`PersistenceError::Display` は全バリアント秘密を含めない、④`tracing-test` による CI 検証（AC-15） |

### 監査ログ規約（`tracing` の使用ルール、OWASP A09 対応）

**目的**: 本 crate の全 I/O 操作に一貫した監査証跡を残し、かつ秘密値を一切ログに載せない。エラー調査・フォレンジック・退行検出を可能にする。記録は `tracing` crate のスパン / イベントで行い、daemon 側の subscriber 実装（別 Issue）に従ってフォーマット・出力先が決まる。

| 操作 | レベル | 記録タイミング | 必須フィールド | 禁止フィールド |
|-----|------|-------------|-------------|-------------|
| `load` エントリ | `info` | `SqliteVaultRepository::load` 冒頭 | `vault_dir`（絶対パス、秘密ではない） | レコード内容、パスワード、VEK |
| `load` 成功 | `info` | 戻り値 `Ok(vault)` 直前 | `record_count`, `protection_mode`, `elapsed_ms` | レコード内容、ラベル、plaintext_value |
| `save` エントリ | `info` | `SqliteVaultRepository::save` 冒頭 | `vault_dir`, `record_count`（入力 vault から） | ラベル、plaintext_value、ciphertext、nonce、aad |
| `save` 成功 | `info` | `Ok(())` 直前 | `record_count`, `elapsed_ms`, `bytes_written`（`.new` サイズ） | 同上 |
| `exists` 呼出 | `debug` | 戻り値直前 | `vault_dir`, `found: bool` | — |
| `PersistenceError` 全バリアント | `warn`（`InvalidPermission` / `OrphanNewFile` / `Locked` / `UnsupportedYet`）／ `error`（`Sqlite` / `Corrupted` / `AtomicWriteFailed` / `SchemaMismatch` / `Io` / `InvalidVaultDir` / `CannotResolveVaultDir`） | return の直前 | エラーバリアント名、`path`（秘密でない）、`stage`（atomic write 時）、`table`（Corrupted 時）、`reason`（列挙の variant 名のみ） | 下位 `#[source]` の `Debug` 文字列全体（`SecretString` の `Debug` は `"[REDACTED]"` 固定だが、SQLite エラーメッセージにパラメータ値が混入する可能性があるため、`source` は `display_redacted()` ヘルパ経由で記録し、SQL パラメータは `?` 化して記録） |
| atomic write 中間段階 | `debug` | 各 stage（`PrepareNew` / `WriteTemp` / `FsyncTemp` / `FsyncDir` / `Rename` / `CleanupOrphan`）遷移時 | `stage` 名、`elapsed_ms` | ファイル内容 |

**秘密値マスクの型保証**:

- `SecretString` / `SecretBytes` の `Debug` 実装は `shikomi-core` で `"[REDACTED]"` 固定（Issue #7 完了済み）。`tracing::info!` の `?value` / `%value` に誤って渡しても平文は出ない
- **防衛線として**: `audit.rs` モジュールが公開する **5 関数**（`entry_load` / `entry_save` / `exit_ok_load` / `exit_ok_save` / `exit_err`）のみ呼出可。シグネチャは `&VaultPaths` / `usize` / `u64` / `ProtectionMode` / `&PersistenceError` など**秘密を含まない型**のみ（完全シグネチャは `../detailed-design/data.md` §`Audit` 参照）。直接 `tracing::info!` を vault payload に対して発行することは clippy `disallowed-methods` lint で機械的禁止
- **load と save の終了ログを分離する根拠**: load 成功時は `protection_mode`（daemon のモード遷移判定）、save 成功時は `bytes_written`（ディスク消費監視）を記録。共通の `exit_ok` にまとめると `Option` まみれで型保証が緩む（Fail Safe by type > YAGNI）
- `PersistenceError::Display` は全バリアントで秘密を含めない。`Sqlite` 下位エラー等は `display_redacted()` で SQL パラメータを `?1` と `***` にマスク
- **検証（AC-15）**: `tracing-test` で全ログ文字列を収集 → 秘密値の生文字列が 1 文字も現れないことを grep 検証

**監査ログと通常 I/O の分離**:

- 本 crate は**ログ出力先を選ばない**。`tracing` subscriber の設定は daemon / CLI / GUI 側の責務
- 監査証跡の保管場所（ファイルローテーション、改竄対策）は別 Issue（daemon Issue）で決定

### vault ディレクトリ検証（`VaultPaths::new` の設計、OWASP A01 対応）

**目的**: `SHIKOMI_VAULT_DIR` 環境変数による任意パス指定機能が悪用されないよう、危険なパスパターンを入口で拒否する。パストラバーサル・シンボリックリンク経由の権限昇格・システム保護領域への書込を設計レベルで排除する。

**検証アルゴリズム**（`VaultPaths::new(dir: PathBuf) -> Result<Self, PersistenceError>`、7 段階）:

1. **絶対パス必須** → `NotAbsolute`（相対パス経由の `..` 辿り攻撃面を消す）
2. **`..` 要素早期拒否** → `PathTraversal`（`canonicalize` 前の生値判定で、存在しないパス経由の `..` も拒否）
3. **シンボリックリンク検出** → `SymlinkNotAllowed`（`dir` 自身と全親要素に `fs::symlink_metadata` / `is_symlink`。リンク先張替え攻撃対策）
4. **`canonicalize` 適用** → `Canonicalize { source }`（初回起動でディレクトリ未存在なら親の最長存在部分のみ正規化）
5. **保護領域チェック** → `ProtectedSystemArea { prefix }`（`PROTECTED_PATH_PREFIXES_{UNIX,WIN}` と prefix 照合、Windows は case-insensitive）
6. **ディレクトリ判定** → `NotADirectory`（存在するなら `is_dir()` を要求）
7. **合格** → `VaultPaths` に `vault.db` / `vault.db.new` / `vault.db.lock` を派生結合して返す

**設計判断**:

- **二段階検査**: `..` の早期拒否 + `canonicalize` 後の保護領域チェックを両方行うのは、`canonicalize` がシンボリックリンクを追って無害に見えるパスに化ける（例: `/tmp/shikomi → /etc/shikomi`）のを防ぐため。リンク自体を拒否することで TOCTOU を封じる
- **リンク完全禁止**: vault ディレクトリは「ユーザーデータ直下の実ディレクトリ」という単純契約（個人利用 OSS の YAGNI）
- **Zero Trust**: `dirs::data_dir()` の戻り値も同じバリデーションを通す（OS の戻り値だからと無検証にしない）
- **`with_dir`**: `#[doc(hidden)]` の内部 API はバリデーションを通さない（tempdir の無用な失敗回避）。正規 API（`new()`）のみが検証責任を負う

### Windows owner-only DACL の適用戦略（REQ-P07 の設計、A01 / A05 対応）

Unix `0600` / `0700` に相当する「所有者のみ read/write」を NTFS で達成し、同ユーザ内他プロセス・グループ権限・継承 ACE 経由の open を**作成時強制・検証時再確認の二段階**で封じる。検証時は次の 4 不変条件を全て満たさなければ `InvalidPermission`: **(1)** `SE_DACL_PROTECTED` bit が立つ（親 `%APPDATA%` 等からの継承 ACE を破棄済み）、**(2)** ACE 数 = 1 かつ `ACCESS_ALLOWED_ACE_TYPE`（Deny ACE 不使用、暗黙拒否で十分、KISS）、**(3)** ACE のトラスティ SID がファイル所有者 SID と `EqualSid` で一致（`BUILTIN\Administrators` 等の組込みトラスティ混入拒否）、**(4)** `AccessMask` が期待値と完全一致（ファイル `FILE_GENERIC_READ \| FILE_GENERIC_WRITE` ／ ディレクトリ `+ FILE_TRAVERSE`、`DELETE` / `WRITE_DAC` / `WRITE_OWNER` / `FILE_EXECUTE` 等の過剰ビットは即拒否。`WRITE_DAC` は DACL 再書換を許し壁突破可）。所有者 SID は**ファイル側**の `OWNER_SECURITY_INFORMATION` を `GetNamedSecurityInfoW` で取得し（`OpenProcessToken` + `TokenOwner` は使わない）、UAC 昇格で作成されたファイルの所有者が `BUILTIN\Administrators` になるケース（`SE_CREATE_OWNER_NAME` ポリシー）に対応する。`ensure_*` は所有者を touch せず DACL のみ書換（`SecurityInfo` から `OWNER_SECURITY_INFORMATION` を落とす）。詳細アルゴリズムは `../detailed-design/flows.md` §Windows、クラス分解は `../detailed-design/classes.md` §13、CI 前提は REQ-P07 受入観点 3 / 4 を参照。

### `unsafe_code` 整合方針（REQ-P07 実装時の lint 例外）

**前提**: workspace `Cargo.toml` の `[workspace.lints.rust]` に `unsafe_code = "forbid"` が設定されている（`forbid` は `#[allow(unsafe_code)]` でも解除できない強い規約）。一方、Windows の `SetNamedSecurityInfoW` / `GetNamedSecurityInfoW` 等の Win32 API 呼出は `windows` crate 経由でも `unsafe fn` のため、`permission/windows.rs` で `unsafe { ... }` ブロックが必須になる。両者を整合させる選択肢を比較する:

| 方式 | 影響範囲 | 採否 |
|-----|---------|------|
| ① workspace lint を `deny` に弱める | 全 crate で `#![allow(unsafe_code)]` が通る。`shikomi-core`（pure ドメイン）も含めて unsafe 許容の可能性を開き、防御壁が崩れる | **不採用** |
| ② `shikomi-infra/Cargo.toml` の `[lints.rust] unsafe_code = "allow"` で crate 全体を許容 | `shikomi-infra` 全モジュールで unsafe 許容。`sqlite/` / `paths.rs` / `lock.rs` / `audit.rs` にも unsafe の窓口が開く | **不採用** |
| ③ `permission/windows.rs` 冒頭に **`#![allow(unsafe_code)]` モジュール属性** を置き、ファイル 1 枚だけ例外化 | unsafe が許容されるのは `permission/windows.rs` のみ。他ファイルは `forbid` のまま | **採用** |

**採用: ③**。実装時のルール:

- `permission/windows.rs` 冒頭に、crate level ではなく**ファイル level**の属性として以下を置く（必ずファイルの**1 行目付近**、モジュール doc コメントの直後）:
  - `#![allow(unsafe_code)]` — このモジュールだけ `forbid` を解除
  - 根拠コメント — 「Microsoft 公式 `windows` crate が Win32 Security API を `unsafe fn` で公開しているため、owner-only DACL の `SetNamedSecurityInfoW` / `GetNamedSecurityInfoW` / `SetEntriesInAclW` 等を呼ぶ本ファイルに限り `unsafe_code` lint を許容する。他モジュールは `forbid` を保持」と明記、Microsoft Learn 出典 URL も 1 行添える
- `unsafe` ブロックは**関数単位に最小化**する。1 関数 1 ブロックを原則とし、複数 Win32 呼出をまとめる場合も各呼出の直前・直後にコメントで**ポインタ寿命・解放責務**を書く（`../detailed-design/classes.md` §13 §RAII ガード）
- **`unix.rs` は `unsafe` を使わない** — `std::os::unix::fs` で mode 操作が完結するため、`#![allow(unsafe_code)]` は置かない。`forbid` のまま維持
- **workspace lint を触らない** — PR #15 の本タスクで `[workspace.lints.rust]` の値を弄らない。実装 PR（REQ-P07 本実装）で `permission/windows.rs` にのみ属性を追加する、という差分の局所化を保つ（Boy Scout Rule: 広域の影響を出さず目的に必要な最小変更）

**検証手段**: 実装 PR の CI で `grep -rn '#!\[allow(unsafe_code)\]' crates/` を走らせ、`permission/windows.rs` 以外に属性が出現したら fail するスクリプトジョブを追加する（本 Issue のスコープではスクリプト追加まではしない、実装 Issue で対応）。それまでは服部の目視レビューで担保する。

### OWASP Top 10 対応

| # | カテゴリ | 対応状況 |
|---|---------|---------|
| A01 | Broken Access Control | **主対応** — ①vault ファイル / ディレクトリを OS パーミッション（Unix `0600`/`0700`）・NTFS owner-only DACL（所有者のみ、継承破棄、ACE ちょうど 1 個、完全マスク）で保護、起動時検証（REQ-P06 / P07、§Windows owner-only DACL の適用戦略）。②`SHIKOMI_VAULT_DIR` の **パストラバーサル・シンボリックリンク・保護領域アクセスを `VaultPaths::new` で拒否**（§vault ディレクトリ検証、`InvalidVaultDir`）。③プロセス間 advisory lock（`VaultLock`）で daemon 未起動時の並行書込レースを封じる |
| A02 | Cryptographic Failures | **本 Issue 範囲外**（平文モード前提）。暗号化モードは別 Issue。ただし `kdf_salt` / `wrapped_vek_*` / `ciphertext` / `nonce` / `aad` のスキーマは先行定義し、将来の暗号化実装で atomic write・パーミッション層をそのまま再利用できる |
| A03 | Injection | **主対応** — 生 SQL 連結禁止、`rusqlite::params!` マクロ経由のみ（REQ-P12）。`PRAGMA` は静的リテラルのみ。コードレビュー + grep + clippy で機械的検証 |
| A04 | Insecure Design | **主対応** — atomic write / `.new` 残存検出 / Fail Secure（勝手に復旧しない）を設計レベルで強制。暗号化モードを静かにスキップせず `UnsupportedYet` で明示拒否（REQ-P11） |
| A05 | Security Misconfiguration | **主対応** — パーミッション設定を作成時に**強制**し、起動時に**検証**する。ユーザ誤設定を検知（REQ-P06 / P07）。`journal_mode=DELETE` を明示的に設定（WAL のチェックポイント不整合を避ける）。`unsafe_code` は `permission/windows.rs` のみ §unsafe_code 整合方針で明示例外化 |
| A06 | Vulnerable Components | `rusqlite` バンドル版（`features = ["bundled"]`）で外部 SQLite に依存しない。SQLite 本体のアドバイザリは `cargo deny` で検出。`windows` crate は Microsoft 公式 |
| A07 | Auth Failures | 対象外 — 本 Issue は認証ロジックを持たない。認証は暗号化モード（別 Issue）でマスターパスワード経由 |
| A08 | Data Integrity Failures | **主対応（部分）** — atomic write で部分書込を防ぐ。ドメイン再構築時に全 newtype 検証（REQ-P09）で整合性を担保。**暗号学的改竄検出**は本 Issue 範囲外（平文モードには AEAD がない、§7.0 で明示） |
| A09 | Logging Failures | **主対応** — `§監査ログ規約` で記録対象・レベル・秘密マスクルールを網羅。`audit.rs` 経由のみログを許可し clippy `disallowed-methods` で直接 `tracing::info!` 呼出を禁止。秘密型の `Debug` は Issue #7 で `"[REDACTED]"` 固定、`PersistenceError::Display` は全バリアントで秘密を含めない。検証は AC-15 で `tracing-test` により機械的に行う |
| A10 | SSRF | 対象外 — HTTP リクエストを発行しない |
