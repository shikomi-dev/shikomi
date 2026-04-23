# 基本設計書 — security（脅威モデル / OWASP / CVE 確認 / 漏洩経路監査）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/basic-design/security.md -->
<!-- 兄弟: ./index.md, ./error.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。Rust シグネチャが必要な場合はインライン `code` で示す。

## 脅威モデル（CLI 層の追加視点）

CLI 層は `docs/architecture/context/threat-model.md` で既に定義された脅威を**新たに増やすものではない**が、以下の経路を設計時点で封じる。

| 想定攻撃 / 事故 | 経路 | 保護資産 | 対策 |
|--------------|------|---------|------|
| shell 履歴経由の secret 漏洩 | `shikomi add --kind secret --value "pw"` が `.bash_history` / `zsh_history` に残る | Secret レコードの平文値 | MSG-CLI-050 警告を stderr に出力、`--stdin` 経路を推奨ドキュメント化 |
| stdout / stderr 経由の secret 漏洩 | `list` 出力や panic メッセージに Secret 値が混入 | Secret レコードの平文値 | `SecretString::Debug` が `"[REDACTED]"` 固定、`ValueView::Masked` で `list` 整形時に値を生成しない、**panic hook は固定文言のみ出力し `info.payload()` を一切参照しない**（§panic hook 参照） |
| ログ経由の secret 漏洩 | `tracing::{info,debug,error}!` で `Record` / `Vault` / panic payload をそのまま出力 | Secret レコードの平文値 | UseCase / Presenter で `tracing` を呼ぶ際、ラベル・ID のみを引数にし `SecretString` を触らない。**panic hook では `tracing` マクロを呼ばない**（服部平次指摘、§panic hook 参照） |
| 非対話スクリプトからの意図せぬ削除 | CI スクリプトで `shikomi remove --id ...` を `--yes` なしで実行 | レコード存在 | REQ-CLI-011: 非 TTY で `--yes` 未指定なら `CliError::NonInteractiveRemove` で Fail Fast + 型レベル強制（`ConfirmedRemoveInput` を bool フィールドなしで構築必須） |
| 暗号化 vault への誤操作 | 暗号化 vault に `add` / `edit` / `remove` が走り、平文 Record を混在させる | vault 整合性 | REQ-CLI-009: 全コマンドで `load()` 直後にモードチェック、暗号化なら Fail Fast（`ExitCode::EncryptionUnsupported = 3`） |
| vault dir の path traversal | `--vault-dir "/etc/../home/user/evil"` 等 | ファイルシステム | `shikomi-infra::SqliteVaultRepository::from_directory` が内部で `VaultPaths::new` の既存検証（`PROTECTED_PATH_PREFIXES` 他）を呼ぶ。CLI 層で独自検証を重ねない（DRY） |
| fail open（エラー握り潰し） | `repo.save()` が失敗したが stdout に `added: ...` を出してしまう | データ整合性 | UseCase は `Result<_, CliError>` を返し、`run()` は `Ok` のみ Presenter を呼ぶ（握り潰しの型エラー化） |

## panic hook と secret 漏洩経路の遮断（服部平次指摘 ①への対応）

**問題**: 初版設計では panic hook 内で `tracing::error!(panic = ?info)` を呼ぶ記述があった。`PanicHookInfo::Debug` は payload の raw 文字列を展開するため、依存 crate（`rusqlite` / `rpassword` / `fs4` 等）が `panic!("bad value: {}", raw_str)` の形で panic すると、`raw_str` に secret が混入する可能性がある経路が残る。`tracing_subscriber` の既定 fmt layer は stderr 直送のため、A09（ログ経路漏洩ゼロ）と衝突する。

**本設計の契約（修正後）**:

1. **panic hook は `eprintln!` で固定文言のみを出力する**（`MSG-CLI-109` 相当の英語 / 日本語併記文字列。hook 内で `Locale` を再検出するか、グローバル `OnceLock<Locale>` を読むかは実装時に決定。どちらも `info.payload()` を参照しない）
2. **panic hook 内で `tracing` マクロを一切呼ばない**（`tracing::error!` / `tracing::warn!` / `tracing::info!` / `tracing::debug!` / `tracing::trace!` いずれも禁止）
3. **`info.payload()` / `info.message()` / `info.location()` の値を文字列化してログ・stderr に流さない**（payload を `Debug` 経路で文字列化すると secret が展開され得る）
4. **開発者向けバックトレース**: `RUST_BACKTRACE=1` が設定されている場合、Rust ランタイムはバックトレースを stderr に出力する。バックトレースには**関数名・ソースファイル位置**は含まれるが、**ローカル変数値は含まれない**（デバッグ情報のレベルに依存するが、リリースビルドの `panic = "unwind"` デフォルトでは変数値の復元は行われない）。この挙動は panic hook 設計の想定外であり、本 feature はバックトレースを抑止しない（ユーザが開発者モードで明示的に有効化した場合の情報量を削らない）。ただし、**CI 環境で `RUST_BACKTRACE` を無効化すること**を `../detailed-design/future-extensions.md` の運用注記に記す。
5. **パニックの根本原因調査**: panic hook で payload を出さないため、開発者がバグを追う際は `cargo test -- --nocapture` か、テスト環境で個別に `panic::set_hook` を上書きして `Debug` を見る。本番 CLI では secret-safe 性を優先する。

**テスト観点**（テスト設計担当への引き継ぎ）:
- TC-CI: `shikomi-cli/src/` 配下で `tracing::` マクロの引数に `panic` / `PanicHookInfo` / `info.payload` を含む行が存在しないことを grep で検証
- TC-UT: `std::panic::catch_unwind` で意図的に panic を起こし、stderr 出力が `MSG-CLI-109` 固定文言に一致すること（payload 由来のマーカー文字列が出ないこと）

## `expose_secret()` 呼び出し経路の監査（服部平次指摘 ③への対応、CI 契約）

**契約**: `crates/shikomi-cli/src/` 配下のプロダクトコードにおける `SecretString::expose_secret()` / `SecretBytes::expose_secret()` の呼び出しを **0 箇所**とする。`SecretString` は生成〜保存まで**一度も中身を見ずに所有権移動のみで運搬**する。

経路別の設計:

| 経路 | 設計 | 備考 |
|------|-----|------|
| `add` UseCase で Secret / Text 新規レコード保存 | `shikomi-cli/src/usecase/add.rs` 内で `RecordPayload::Plaintext(input.value)` を構築（`input.value: SecretString` を所有権移動）、`Record::new(..., payload, ...)` に渡す | `SecretString` の中身は一切触らない（`expose` 呼び出しなし） |
| `edit` UseCase で既存レコードの value 更新 | `with_updated_payload(RecordPayload::Plaintext(input.value.unwrap()), now)` に `SecretString` を所有権移動 | 同上 |
| `list` の Text kind preview 表示 | `shikomi-core::Record::text_preview(&self, max_chars: usize) -> Option<String>` を呼ぶ。`expose_secret` は `shikomi-core` 内で完結する | `shikomi-cli/src/view.rs` 内で `expose_secret` は**呼ばない**（ペテルギウス指摘 α の最終採用案、詳細設計 `../detailed-design/data-structures.md §ValueView の構築ルール` 参照） |
| `list` の Secret kind 表示 | `ValueView::Masked` として固定マスク文字列（`"****"`）を返すのみ | `SecretString` に触らない |

**`shikomi-core::Record::text_preview` の追加により expose 経路は core 内に閉じる**: 本 feature の最終採用案では、`shikomi-core` 内で `expose_secret` を呼ぶのは `Record::text_preview` の内部実装のみ。`shikomi-cli/src/` からの expose 呼び出しは**厳密に 0 件**となる。

**CI 監査の TC 要件**（テスト設計担当が TC-CI に追加する要件を設計書で明示）:

1. `grep -rn "expose_secret" crates/shikomi-cli/src/` の結果が **0 件**であること（本 feature の契約）
   - 将来、`show` コマンド等で値露出が必要になった時点で契約を見直す。契約破棄時は要件定義書に明記し、代替の漏洩経路監査を設計する
2. 監査対象ディレクトリ:
   - `crates/shikomi-cli/src/` 配下の**全 `.rs` ファイル**（`main.rs` / `lib.rs` / `cli.rs` / `error.rs` / `input.rs` / `view.rs` / `usecase/` / `presenter/` / `io/`）
3. 例外許可ディレクトリ:
   - `tests/` 配下のテストコード（モック Repository が `SecretString` を assertion で比較する際に expose が必要になる可能性）。ただしテストコードでも**プロダクトコード経由の間接 expose を検出する目的**では `tests/` も併せて grep するべき。本 feature では**プロダクトコード `src/` のみを契約対象**とする
4. CI 失敗条件: grep が 1 件以上マッチする場合、CI job を失敗させる

**テスト担当への引き継ぎ**: `test-design/ci.md` の CI 補助テストに以下の要件を追加（TC-ID 採番はテスト設計担当の責務。本書は要件のみ示す）:

- **`expose_secret` grep 契約**: `crates/shikomi-cli/src/**/*.rs` に対し `grep -n "expose_secret"` の結果が **0 件**であること（**TC-CI-013** 相当。`scripts/ci/audit-secret-paths.sh` で実装）
- **panic hook 内 `tracing` 呼び出し禁止**: `crates/shikomi-cli/src/**/*.rs` に対し、panic hook 内で `tracing::(trace|debug|info|warn|error)!` マクロを呼ばないこと（**TC-CI-014** 相当）
- **panic hook 内 `info.payload()` / `info.message()` 参照禁止**: panic hook 内で `PanicHookInfo` の `payload()` / `message()` / `location()` を文字列化して stderr / ログに流さないこと（**TC-CI-015** 相当）

**本書では追加の TC-CI-xxx 定義を行わない**（TC-ID 採番は `test-design/ci.md` に一元化する方針。設計書と試験書で同一 TC-ID に異なる定義を置くと実装者が参照先を誤るため、δ 指摘に従い**重複定義を廃止**）。

**既存の `SqliteVaultRepository` 参照監査**（UseCase / Presenter に具体型が漏れていないことの証明）は `test-design/ci.md` TC-CI-012（`shikomi-cli/src/usecase/` / `presenter/` / `error.rs` / `view.rs` / `input.rs` 配下での `SqliteVaultRepository` 参照 0 件、および `lib.rs` の `run()` 1 箇所のみ）で検証する。

これらは本 feature の Boy Scout Rule 遵守および Phase 2 移行パスの健全性維持のために**CI 必須**とする。

## OWASP Top 10 対応

本 feature は CLI 層のため項目ごとに該当度が異なる。

| # | カテゴリ | 対応状況 |
|---|---------|---------|
| A01 | Broken Access Control | **対応** — vault path は OS ファイルパーミッション（既存）と `shikomi-infra::VaultPaths` の `PROTECTED_PATH_PREFIXES` 検証（既存、`from_directory` 経由で起動）に委譲。CLI 層で独自のアクセス制御は行わない |
| A02 | Cryptographic Failures | **対応** — 暗号化未対応のため暗号処理は発生しない。`SecretString` が `Debug` / `Serialize` 非露出を型で保証（既存）。本 feature は剥ぎ落とし禁止のルールを徹底 |
| A03 | Injection | **対応** — CLI フラグはすべて `RecordLabel` / `RecordId` / `RecordKind` / `SecretString` などの検証済み型に変換してから UseCase に渡す。生 `String` を SQL / シェルに流さない（SQLite クエリは `rusqlite` のパラメータバインディングを既存 infra が使用） |
| A04 | Insecure Design | **対応** — UseCase / Presenter / IO 分離で責務が明確。`VaultRepository` trait 経由で Phase 2 への移行パスが担保されている |
| A05 | Security Misconfiguration | **対応** — 本 feature で設定項目は `--vault-dir`（clap 経由で env `SHIKOMI_VAULT_DIR` も吸収）/ `LANG` 環境変数のみ。デフォルトは OS 標準位置で安全 |
| A06 | Vulnerable Components | **対応** — §依存 crate の CVE 確認結果 を参照 |
| A07 | Auth Failures | 対象外 — 本 feature は認証機能を持たない |
| A08 | Data Integrity Failures | **対応** — vault の atomic write（既存 infra）で書込み失敗時の部分更新を防ぐ。本 feature は save 失敗時の stdout 誤出力を型で防止 |
| A09 | Logging Failures | **対応** — `tracing` 呼び出しで Secret 値を触らないルールを UseCase で徹底。panic hook は `tracing` 呼び出し禁止かつ `payload` 非参照（§panic hook 参照）。ログ経路の漏洩は CI grep + 結合テストで検証 |
| A10 | SSRF | 対象外 — HTTP リクエストを発行しない |

## 依存 crate の CVE 確認結果（服部平次指摘 ②への対応）

**確認日時**: 2026-04-23  
**確認方法**: RustSec Advisory Database（<https://rustsec.org/advisories/>）を参照し、本 feature で新規導入する全 crate の advisory 登録有無を確認  
**確認者**: 設計担当（セル）

| crate | 採用バージョン | 用途 | RustSec advisory | GHSA advisory | 判定 |
|-------|-------------|------|-----------------|--------------|------|
| `clap` | 4.x | サブコマンド / フラグ定義 | **なし** | 未登録 | ✅ クリーン |
| `anyhow` | 1.x | `run()` 戻り値 / panic hook 隣接 | **なし** | 未登録 | ✅ クリーン |
| `is-terminal` | 0.4.x | stdin TTY 判定 | **なし** | 未登録 | ✅ クリーン |
| `rpassword` | 7.x | 非エコー stdin 入力 | **なし** | 未登録 | ✅ クリーン（※1） |
| `assert_cmd` | 2.x（dev） | E2E プロセス実行 | **なし** | 未登録 | ✅ クリーン |
| `predicates` | 3.x（dev） | `assert_cmd` assertion | **なし** | 未登録 | ✅ クリーン |
| `thiserror` | 2.x（既存） | `CliError` 定義 | **なし** | 未登録 | ✅ クリーン |

※1: `rpassword` は内部で入力文字列を `String` で保持する期間がある（ラップする前の数 μs〜数 ms）。本 feature では返却直後に `SecretString::from_string(s)` に移動するが、`String` のメモリが zeroize されるかは `rpassword` 実装次第で保証されない。詳細設計 `../detailed-design/future-extensions.md` §実装注意 で「将来、独自 secret 入力実装に置換する余地」として明記。現時点の攻撃面として大きなリスクはない（同一プロセス内で数 ms の残存に過ぎず、メモリダンプ攻撃者は別のより容易な経路を持つ）。

**運用規約**:

- 本 feature の PR が develop にマージされる時点で `cargo-deny check advisories` が pass すること（既存 CI で実行）
- 本表は**静的スナップショット**。`cargo-deny` と Dependabot による継続監査が本線（`docs/architecture/tech-stack.md` §2.2）
- 新規 advisory が発行された場合、該当 crate を `deny.toml` の `[advisories].ignore` に**入れない**（§4.3.2 暗号クリティカル crate 同等のゼロトラスト方針）。即時アップグレード or 代替 crate 検討

## `unsafe_code` の扱い

本 feature は `shikomi-cli` crate 内に `unsafe` ブロックを**一切追加しない**。`unsafe_code = "deny"` が `[workspace.lints.rust]` に設定済み（`Cargo.toml`）で、`shikomi-cli` にオーバーライドを置かない。

- `rpassword` の内部 `unsafe` は依存 crate 内で完結し、本 crate のソースには影響しない
- `is-terminal` も同様

`unsafe_code` の allow オーバーライドは Windows `permission/windows/` のみに許可されており（既存 `shikomi-infra`）、本 feature は新規の unsafe 許可を一切追加しない。

## セキュリティに関するテスト責務の分担

| テストレベル | 責務 |
|-------------|------|
| ユニット（UT） | `SecretString` の `Debug` 出力検証、`ValueView::Masked` の integrity、`CliError::Display` が secret を含まないこと |
| 結合（IT） | `RecordView::from_record` が Secret kind を `Masked` に変換することを全種類検証、モック Repo で暗号化 vault を返した際の Fail Fast |
| E2E | `SECRET_TEST_VALUE` マーカー投入 → stdout/stderr 全文 grep で不在、非 TTY `remove` で削除拒否、`--vault-dir` 指定時の path traversal 検証 |
| CI（補助） | `expose_secret` 呼び出し 0 件 grep（§expose_secret 監査）/ `tracing!` への panic 参照 0 件 grep / `SqliteVaultRepository` 参照 1 箇所 grep |

テストケース番号の割当は `test-design.md` が担当する。本書は設計側からの**要件の明示**に留める。
