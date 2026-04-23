# 要求分析書

<!-- feature: vault-persistence / Issue #10 -->
<!-- 配置先: docs/features/vault-persistence/requirements-analysis.md -->

## 人間の要求

> （Issue #7 vault ドメイン型定義 完了を受けて）
> 開発の続きをお願いします。ドキュメントを見て次のIssueの検討からになリマス。複数PRを同時並行で進めるのは禁止です。
> — 依頼主（まこちゃん）

工程1のタスク分析で Clean Architecture の下層を積み上げる方針に従い、ドメイン完成直後の自然な下方展開として **`shikomi-infra` crate に vault 永続化層を追加**する Issue #10 を発行した。本 Issue はその設計工程。

## 背景・目的

- Issue #7 完了により **`shikomi-core::Vault` 集約と関連ドメイン型が揃った**。しかし `shikomi-infra` は `src/lib.rs` に doc コメント 1 行のみの空殻で、vault をディスクに保存・復元する経路が存在しない
- 後続 Issue（IPC プロトコル・daemon プロセス骨格・CLI・GUI）はいずれも「vault が**ファイルに永続化**されていること」を前提に書かれる。永続化層が無いと daemon が起動直後に必ず空の vault を抱える状態になり、end-to-end フローが成立しない
- 永続化は **Tampering（T）脅威と Information Disclosure（I）脅威の主戦場**。`docs/architecture/context/threat-model.md` §7.0 / §7.1 で確定した atomic write と OS パーミッション `0600`+`0700` をここで型とコードに落とす責務がある
- ビジネス価値: ペルソナ A（田中、Windows 営業職）/ C（佐々木、総務）が「アプリを再起動してもレコードが残る」ことを当然の期待として持つ。永続化層なしでは MVP として成立しない

## 議論結果

Issue #7 マージ直後の工程1で以下を合意:

- 設計担当（セル）: ドメイン完成直後は Clean Architecture の下方展開が自然。`shikomi-infra` に `VaultRepository` trait と SQLite 実装を入れる
- キャプテン決定: 案 A（vault 永続化層、平文モード）を単一 Issue で進める。暗号化アダプタ（`VekProvider` 実装、Argon2id+AES-GCM）は別 Issue に分離
- スコープ境界（重要）: 本 Issue は **平文モードの I/O を対象**とする。ただし **SQLite スキーマは両モード（plaintext / encrypted）対応**で設計する。根拠: スキーマを平文専用にすると、暗号化モード導入 Issue で大規模 ALTER TABLE が必要となり Boy Scout Rule 違反の温床になる。暗号化カラムを先行定義し、本 Issue の実装では暗号化モード vault の load/save を明示的に `UnsupportedYet` でエラー返却する（Fail Fast）
- テスト設計はテスト担当（涅マユリ）が設計書完成後に作成する

## ペルソナ

本 Issue は `shikomi-infra` の内部実装であり、エンドユーザーが直接触れる API ではない。ただし、永続化の信頼性はエンドユーザーのデータ損失リスクに直結するため、エンドユーザーペルソナ（田中/山田/佐々木、`docs/architecture/context/overview.md` §3.2）を間接的な受益者として置く。直接的な開発者ペルソナは以下。

| ペルソナ名 | 役割 | 技術レベル | 利用文脈 | 達成したいゴール |
|-----------|------|-----------|---------|----------------|
| 野木 拓海 | 後続 Issue の daemon 実装担当 | Rust 中級（`tokio` / `rusqlite` / `rmp-serde` 実務経験） | `shikomi-daemon` で vault をロードしホットキー時に読む | `VaultRepository` trait を受け取るだけで vault の I/O を完結できる。内部の SQLite 詳細・atomic write 手順を知らなくて済む |
| 志摩 凛 | OSS コントリビュータ | Rust 初級 | 電源断時のデータ保全テストを追加したい | atomic write の契約がドキュメントに明示されており、黄金値テストを書き足せる |
| 田中 俊介（間接） | Windows 営業職（エンドユーザー） | OS 操作可、CLI 不可 | PC を強制シャットダウンしてから再起動して shikomi を開く | 電源断前に保存したレコードが欠損なく復元される |

## 前提条件・制約

- **Issue #7 完了済み**: `shikomi-core` の `Vault`, `VaultHeader`, `Record`, `RecordPayload`, `VekProvider` trait、各 newtype（`RecordId`, `RecordLabel`, `KdfSalt`, `WrappedVek`, `CipherText`, `Aad`, `NonceBytes`）がすべて利用可能
- **`shikomi-infra` crate に実装を置く**: Clean Architecture の依存方向（`shikomi-core` ← `shikomi-infra`）を守る。`shikomi-infra` から `shikomi-core` を使うが、逆方向の依存は禁止
- **pure Rust 以外が動いてよい層**: `std::fs` / `rusqlite` / OS API 呼出を本 crate で初めて解禁する。ただし他 crate（`shikomi-cli` / `shikomi-daemon` / `shikomi-gui`）は本 crate 経由でのみ I/O を行う
- **SQLite**: `docs/architecture/tech-stack.md` §2.1 で確定済み。`rusqlite` バンドル版（`features = ["bundled"]`）で外部依存ゼロ
- **atomic write**: `docs/architecture/context/threat-model.md` §7.1 で確定済みの `.new` → `fsync` → `rename`/`ReplaceFileW` 方式
- **OS パーミッション**: 同上 + `tech-stack.md` §2.1「Vault 保護（デフォルト）」で確定済みの `0600`（Unix）/ 所有者 ACL（Windows）
- **journal_mode**: `PRAGMA journal_mode=DELETE`（WAL ではない、§7.1 根拠参照）
- **非同期ランタイム**: 本 crate は **同期 I/O API** を提供する。呼び出し側（`shikomi-daemon`）が `tokio::task::spawn_blocking` でラップする方針とし、`shikomi-infra` 自身は `tokio` に依存しない（`rusqlite` が同期 API であり、薄く逸脱しないことでテスト容易性を保つ）
- **暗号計算は行わない**: AES-GCM / Argon2id の実計算は本 Issue 範囲外。`VekProvider` 実装は別 Issue
- **`[workspace.dependencies]` 一元化**: Issue #4 で確定の方針に従い、`rusqlite` / `dirs` / `thiserror` 等は workspace ルート側で指定

## 機能一覧

| 機能ID | 機能名 | 概要 | 優先度 |
|--------|-------|------|--------|
| REQ-P01 | `VaultRepository` trait | `shikomi-infra` に置く永続化抽象。load / save / exists の 3 操作を持つ | 必須 |
| REQ-P02 | `SqliteVaultRepository` 実装 | `rusqlite` バンドル版で `VaultRepository` trait を実装。vault.db 1 ファイルに 1 vault | 必須 |
| REQ-P03 | SQLite スキーマ定義 | `vault_header` 1 行テーブル + `records` テーブル。両モード対応カラム構成 | 必須 |
| REQ-P04 | Atomic Write | `vault.db.new` に書き込み → `fsync` → `rename`/`ReplaceFileW` で差替え。失敗時 `.new` 削除 | 必須 |
| REQ-P05 | `.new` 残存検出 | 起動時の `load()` 呼出時に `.new` が残っていたら破損扱いとして専用エラーを返す | 必須 |
| REQ-P06 | OS パーミッション強制・検証 | 作成時に `0600`（ファイル）/`0700`（ディレクトリ）を設定、起動時 `stat` で検証、異常なら fail fast | 必須 |
| REQ-P07 | Windows ACL 強制・検証 | NTFS ACL 所有者 SID のみに `GENERIC_READ \| GENERIC_WRITE`、他グループ拒否継承破棄 | 必須 |
| REQ-P08 | Vault ディレクトリ解決 | `dirs` crate で OS 標準ディレクトリを解決（`$XDG_DATA_HOME/shikomi/` / `~/Library/Application Support/shikomi/` / `%APPDATA%\shikomi\`）。テスト用に明示パス指定も許可 | 必須 |
| REQ-P09 | ドメイン再構築時の検証 | SQLite の生値を `shikomi-core` の newtype（`RecordId::try_from_str` 等）に通す。失敗は `Corrupted` エラーで Fail Fast | 必須 |
| REQ-P10 | `PersistenceError` 型 | 永続化層固有のエラー列挙。I/O / SQLite / ドメイン違反 / パーミッション異常 / `.new` 残存を排他区別 | 必須 |
| REQ-P11 | 暗号化モード vault の明示拒否 | 暗号化モード vault を load/save しようとすると `PersistenceError::UnsupportedYet` を返す。別 Issue 完成まで静かに壊れない | 必須 |
| REQ-P12 | SQL インジェクション禁止設計 | 生 SQL 連結禁止、`rusqlite` の parameter binding のみ使用 | 必須 |
| REQ-P13 | プロセス間 advisory lock | `VaultLock` が `save` 時排他ロック・`load` 時共有ロックを取得。別プロセス競合時は `Locked` で即 return（Fail Fast、待機禁止） | 必須 |
| REQ-P14 | 監査ログ（秘密漏洩防止） | `tracing` を `audit.rs` 経由でのみ呼び出す。save / load / exists / error を所定レベルで記録し、秘密値を一切含めない。clippy lint と `tracing-test` で二重検証 | 必須 |
| REQ-P15 | `SHIKOMI_VAULT_DIR` バリデーション | `VaultPaths::new` でパストラバーサル（`..`）・シンボリックリンク・保護領域（`/proc` `/etc` `C:\Windows` 等）・非絶対パス・非ディレクトリを拒否。`InvalidVaultDir { reason }` で Fail Fast | 必須 |

## Sub-issue分割計画

該当なし — 理由: 単一Issueで完結。スコープは `VaultRepository` trait 定義と SQLite 実装・atomic write・OS パーミッションに閉じている。trait と実装を分離 PR すると中途半端な `shikomi-infra`（trait はあるが実装なし）が develop に残り、後続 Issue が進められない。

| Sub-issue名 | スコープ | 依存関係 |
|------------|---------|---------|
| — | — | — |

## 非機能要求

| 区分 | 指標 | 目標 |
|-----|------|------|
| パフォーマンス（平文 load） | 1000 レコードの vault を `load()` してドメイン型組立完了 | p95 50 ms 以下（SQLite は O(n)、newtype 検証含む） |
| パフォーマンス（平文 save） | 1000 レコードの vault を atomic write で save | p95 200 ms 以下（`fsync` 含む、`tmpfs` 除く実ディスク） |
| 信頼性（電源断耐性） | save 中の電源断で vault.db 本体が未破損 | 100%（atomic rename の保証に依存、手動 SIGKILL テストで再現） |
| 信頼性（`.new` 残存検出） | 前回 save で `.new` が残っている場合、必ず検出しエラー返却 | 100% |
| 信頼性（SQL インジェクション耐性） | 外部入力（label / payload）が生 SQL に連結されない | 100%（`rusqlite` の `execute`/`query_row` で parameter binding のみ、grep で検証） |
| セキュリティ（パーミッション） | vault.db が他ユーザから読取不可、所有者以外の書込不可 | Unix: `stat` で `0600` 検証 / Windows: ACL 検証で他 SID 拒否 |
| テスト容易性 | SQLite in-memory + tempdir で結合テストが OS 非依存に書ける | 100%（Windows / macOS / Linux の CI matrix で同一テストが通る） |

## 受入基準

> **注**: AC-01〜AC-17 は `docs/features/vault-persistence/requirements-analysis.md` §受入基準と **完全 1:1 対応**（`requirements-analysis.md` 現行体系を正として、セル・マユリ間で合意確定 2026-04-23）。R-01（save 中クラッシュ耐性 / SIGKILL 非決定的）は合格判定軸外のリスク観点として別置き。AC-06 の `write_new_only` テストが R-01 の論理等価を決定的に検証する。

| 受入基準ID | 受入基準 | 検証レベル |
|-----------|---------|-----------|
| AC-01 | 全機能 REQ-P01〜REQ-P15 の型とメソッドが `shikomi-infra` の公開 API または crate 内部 symbol に存在する（`VaultLock`・`audit.rs`・`VaultDirReason` 含む） | 結合 |
| AC-02 | 平文 vault の `save` → `load` で同一 `Vault` が復元される（レコード順含む） | 結合 |
| AC-03 | 暗号化モード vault を `save` すると `PersistenceError::UnsupportedYet` が返る | 結合 |
| AC-04 | 暗号化モード vault を `load` すると `PersistenceError::UnsupportedYet` が返る | 結合 |
| AC-05 | `.new` ファイルを手動で残した状態で `load` を呼ぶと `PersistenceError::OrphanNewFile` が返る | 結合 |
| AC-06 | `AtomicWriter::write_new_only` テストフックを使って `.new` 書込完了後 rename なし状態を再現 → `vault.db` 本体が未変更で `OrphanNewFile` が返る（R-01 参考観点の決定的等価テスト） | 結合 |
| AC-07 | vault ディレクトリが `0777` で作られている状態で `load` すると `PersistenceError::InvalidPermission` が返る | 結合（Unix） |
| AC-08 | vault.db に対し任意の UTF-8 文字列（絵文字含む）の label を保存し復元できる | 結合 |
| AC-09 | 生 SQL 連結を使っていない（`rusqlite::params!` マクロ経由でのみバインドしている） | 結合（静的 grep） |
| AC-10 | `cargo test -p shikomi-infra` が pass、行カバレッジ 80% 以上 | 結合（CI） |
| AC-11 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` pass | 結合（CI） |
| AC-12 | `SqliteVaultRepository::save` 直後に `stat` でファイルパーミッションを確認すると `0600` である | 結合（Unix） |
| AC-13 | 破損した SQLite ファイル（ゼロバイト / 不正バイト列）を渡すと `PersistenceError::Corrupted` または `PersistenceError::Sqlite` が返り panic しない | 結合 |
| AC-14 | `vault.db.new` が残存した状態で `save()` を呼ぶと `PersistenceError::OrphanNewFile` が返る（save 側 Fail Secure、REQ-P05） | 結合 |
| AC-15 | `tracing` ログ（全レベル）に `SecretString` / `SecretBytes` / `plaintext_value` / `kdf_salt` / `wrapped_vek_*` の生値が一切出現しない（REQ-P14 監査ログ秘密漏洩防止） | 結合 |
| AC-16 | `SHIKOMI_VAULT_DIR` に `/etc/` / `..` 含むパス / シンボリックリンクを指定するとそれぞれ `PersistenceError::InvalidVaultDir` で拒否される（REQ-P15 `VaultPaths::new` 7段階バリデーション） | 結合（Unix） |
| AC-17 | `SqliteVaultRepository::save` 中に別プロセスが同ディレクトリで save を試みると `PersistenceError::Locked` が返る（REQ-P13 advisory lock 競合検知） | 結合 |

> **リスク観点**（合格判定軸外）: **R-01**: save 中クラッシュ（SIGKILL 相当）耐性 — 非決定的で CI 不適。論理等価な決定的テストは **AC-06 / TC-I06** で保証済み。手動探索テストとしてのみ残置。

## 扱うデータと機密レベル

本 Issue は永続化層のため、ドメイン層のデータが**初めてディスクに落ちる**工程。機密レベルの扱いは以下に従う。

| データ | 機密レベル | 本 Issue での扱い |
|-------|----------|-----------------|
| レコード平文（平文モード） | 高（ユーザが「平文モードで保存してよい」と明示的に選択した値） | SQLite の `plaintext_value TEXT` カラムに保存。OS パーミッション `0600` のみで保護（`threat-model.md` §7.0 の残存リスクをユーザ受容） |
| vault ヘッダメタデータ（version / created_at / protection_mode） | 低 | 平文で保存。秘匿する意味がない |
| `kdf_salt` / `wrapped_vek_by_pw` / `wrapped_vek_by_recovery` | 中（暗号化モード時のみ。鍵なしでは無価値） | スキーマには定義するが、本 Issue 実装では書き込み経路を持たない（`UnsupportedYet` で拒否） |
| レコード暗号文（`CipherText` / `NonceBytes` / AAD） | 中（鍵なしでは無価値） | 同上。スキーマに BLOB カラムを持つが、書き込みは別 Issue |
| `RecordId` / `RecordKind` / `RecordLabel` / `created_at` / `updated_at` | 低〜中 | 平文で保存。`RecordLabel` は UTF-8 TEXT として保存（バイト列ではなく `str`） |
| マスターパスワード / VEK / KEK / リカバリコード | 最高 | **本 Issue の経路に一切流れない**。暗号計算は別 Issue。ドメインの `SecretString` / `SecretBytes` が永続化層の公開 API に出てこないことを型で検証 |

**永続化境界の型規約**:
- 秘密値（`SecretString` / `SecretBytes`）は `VaultRepository` の入出力に**そのまま**使う。永続化層内部で `expose_secret()` を呼ぶのは「SQLite の parameter binding に値を渡す瞬間」の 1 箇所のみ
- それ以外の経路で `expose_secret()` を呼ぶのは設計上の欠陥とみなす（レビューで却下）
