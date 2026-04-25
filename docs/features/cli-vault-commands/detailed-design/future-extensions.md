# 詳細設計書 — future-extensions（将来拡張フック / 実装担当への引き継ぎ）

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/detailed-design/future-extensions.md -->
<!-- 兄弟: ./index.md, ./data-structures.md, ./public-api.md, ./clap-config.md, ./composition-root.md, ./infra-changes.md -->

## 記述ルール

疑似コード禁止。将来拡張は「何を」「なぜ今やらないか」「移行時のインパクト」の 3 点で示す。

## バイナリ正規形仕様 / 外部プロトコル互換性の契約

該当なし — 理由: 本 feature は CLI 層のみで、永続化フォーマット（vault.db）や外部プロトコル（IPC / HTTP）を新たに定義しない。vault.db のバイナリ正規形仕様は `docs/features/vault/detailed-design.md` §バイナリ正規形仕様 で既に定義済みで、本 feature は `shikomi-infra` 経由でのみアクセスするため互換性契約に触れない。

CLI の外部 I/F（サブコマンド名 / フラグ名 / 終了コード / メッセージ ID）は**ユーザ向けの互換性契約**として以下を守る:

- サブコマンド名（`list` / `add` / `edit` / `remove`）の**削除・改名は major バージョンアップ時のみ許可**
- フラグ名（`--kind` / `--label` / `--value` / `--stdin` / `--yes` / `--id` / `--vault-dir` / `--quiet` / `--verbose`）の**削除・意味変更は major バージョンアップ時のみ許可**
- 終了コードの意味（`0` / `1` / `2` / `3`）の**再割当禁止**（追加は可、`4` 以降を将来予約）
- `MSG-CLI-xxx` ID の**削除・意味変更禁止**（メッセージ文言の改善は可、ID は固定）

## 将来拡張のための設計フック

Phase 2（daemon 経由）・将来 feature への移行パスを明示する。

> **更新履歴**: Issue #26（`daemon-ipc` feature）で **「daemon 経由 IPC（Phase 2）」が実体化された**。下表先頭行の「変更インパクト」は Issue #26 で実際に行われた変更内容を反映。本 feature の Phase 2 移行契約（「`run()` の 1 行差し替えで Repository 構築のみ変更」）は `daemon-ipc` の `IpcVaultRepository::connect` 追加と `--ipc` グローバルフラグ追加により完了している。

| 将来機能 | 本 feature の設計フック | 変更インパクト |
|---------|------------------|-------------|
| daemon 経由 IPC（Phase 2、**Issue #26 で実体化済み**） | `UseCase` が `&dyn VaultRepository` にのみ依存、コンポジションルートが `lib.rs::run()` 1 箇所 | **実体化結果**: `run()` の Repository 構築箇所に `match args.ipc` 分岐を 1 箇所追加。`args.ipc == false`（既定）→ `SqliteVaultRepository::from_directory(&path)`、`args.ipc == true` → `IpcVaultRepository::connect(&socket_path)`。`UseCase` / `Presenter` / `input` / `view` / `error` の各レイヤは**無変更**（Phase 2 移行契約の完全達成）。詳細: `docs/features/daemon-ipc/detailed-design/composition-root.md` |
| `shikomi vault encrypt` / `decrypt` | `CliError::EncryptionUnsupported` が明示的な誘導先を保持、`MSG-CLI-103` のヒントが `shikomi vault decrypt` に言及済み | `shikomi-infra` 側の暗号化実装 feature とセットで `usecase::vault::encrypt` / `decrypt` を新設。本 feature の `list` / `add` / `edit` / `remove` は暗号化 vault への対応を `Vault::add_record` 等の既存集約 API 経由で追加可能（`RecordPayload::Encrypted` バリアントを扱うだけ） |
| `shikomi list --json` | `ListPresenter::render_list` が `String` を返す pure function、`presenter::list` に `render_list_json` を追加するだけ | `clap` に `--format json` フラグ追加、`run_list` で分岐。UseCase は無変更 |
| `shikomi export` / `import` | 本 feature の UseCase パターンを踏襲、`usecase::export` / `import` を新設 | `VaultRepository::load` / `save` を使う。`tempfile` で中間ファイル管理 |
| `shikomi --locale ja` 明示指定 | `Locale::detect_from_env` に並んで `Locale::from_flag(arg)` を追加 | `clap` のグローバルフラグ追加、`run()` で `locale` 決定処理に 1 分岐追加 |
| `shikomi show <id>`（単一レコード値表示） | `RecordView::Plain` の full-length 出力モードを追加。Secret の `--reveal` フラグは別途慎重設計 | 新 UseCase `usecase::show`、新 Presenter `render_show`。Secret reveal は要セキュリティレビュー |
| `edit --kind` によるレコード kind 変更 | 本 feature は Phase 1 スコープ外として `EditArgs` に `kind` フィールドを定義しない | 将来 feature で `Record::with_updated_kind` を `shikomi-core` に追加し、`EditInput` に `kind: Option<RecordKind>` を追加。UX 設計（Text → Secret 変換時の履歴・clipboard 扱い）を別途議論 |
| `shikomi-gui` の UseCase 共有 | `shikomi-cli` が `[lib] + [[bin]]` 構成で、全 `pub` 項目に `#[doc(hidden)]` | GUI が UseCase を共有したい場合、`shikomi_cli::usecase::*` を import するか、UseCase を別 crate に切り出すかの判断を GUI feature で行う。本 feature は GUI 側の選択肢を狭めない |

## 実装担当（坂田銀時）への引き継ぎメモ

- **`SqliteVaultRepository::from_directory` のリファクタは既存テストを壊さないこと**。`new()` の既存テストはそのまま通る必要がある（Boy Scout Rule の逆: 既存のテストを落とすリファクタは NG）
- **`std::env::set_var` を本番コードで使わない**。`lib.rs` / `usecase/` / `presenter/` / `io/` のいずれでも禁止。テストでも使わない（`assert_cmd::Command::env()` を使う）
- **`SecretString::expose_secret()` は `crates/shikomi-cli/src/` 内で呼ばない**（CI grep で検証、`../basic-design/security.md §expose_secret 経路監査`）。Text kind の preview は `shikomi_core::Record::text_preview(max)` に委譲
- **`clap` の `#[arg(env = "SHIKOMI_VAULT_DIR")]` は `global = true` と組み合わせて使う**。サブコマンド個別の `#[arg(env)]` は非対応パターンが多いため、トップレベル `CliArgs` に置く
- **panic hook は `run()` の最初の行**で登録する（clap パース前）。hook 内で `info.payload()` / `info.message()` / `info.location()` を**参照しない**、`tracing` マクロを**呼ばない**
- **`anyhow` は `run()` 戻り値ラップにのみ使う**。UseCase / Presenter の戻り値は `CliError` で型固定（情報欠損の防止）
- **`is_terminal` crate のバージョンは `0.4` 系を使う**（`std::io::IsTerminal` の Rust 1.70 安定化と互換）。MSRV は `rust-toolchain.toml` の `1.80.0` のため `std::io::IsTerminal` 直接利用でも可だが、テスト差し替え容易性のため crate 経由を推奨
- **`[lib] + [[bin]]` の Cargo.toml 書き方**: `[lib]` に `name = "shikomi_cli"` / `path = "src/lib.rs"` を明示、`[[bin]]` に `name = "shikomi"` / `path = "src/main.rs"` を明示。bin 側で `shikomi_cli::run()` を呼ぶため、`[dependencies]` には自分自身への path 依存を書かない（bin は同一 crate 内の lib を直接参照可）
- **`#[doc(hidden)]` の適用**: `lib.rs` の全 `pub` 項目（`pub fn run`, `pub mod usecase`, `pub mod presenter`, `pub mod input`, `pub mod view`, `pub mod error`）に `#[doc(hidden)]` を付ける。**テスト側から見える** ことは維持される（`doc(hidden)` は rustdoc の出力から隠すだけで、可視性を制限しない）
- **CI grep の実装**: `.github/workflows/*.yml` に grep step を追加（テスト設計 `test-design/ci.md` 相当で具体化、本設計書では要件のみ明示）

## 運用注記

- **`RUST_BACKTRACE` について**: CI 環境では `RUST_BACKTRACE=0` を推奨（panic hook の固定文言のみを出力するため、バックトレースは不要で、ログ肥大化と secret 漏洩リスクの両方を抑止）。ローカル開発で詳細なデバッグが必要な場合は個別に `RUST_BACKTRACE=1` を設定
- **`RUST_LOG` について**: `shikomi_cli::run()` で `tracing_subscriber::fmt` を初期化する際、`RUST_LOG` を読むか否かは実装時判断。読む場合は `EnvFilter::from_default_env()` を使う。本 feature はユーザが `RUST_LOG=shikomi_cli=debug` で詳細ログを見る可能性を否定しないが、ログ内の secret 露出は CI grep で監査

## 残タスク（本 PR では扱わない）

- **実装 PR**: `shikomi-cli` 本体コードの追加（別 PR、本設計 PR のマージ後）
- **`tech-stack.md` 更新**: 実装 PR で同時に反映（本設計 PR には含めない、変更理由の根拠が実装時の確定バージョンと連動するため）
- **CI ジョブ追加**: TC-CI-013/014/015 を `.github/workflows/` に組み込む（実装 PR で対応）

## 結語

本 feature の設計は、Clean Architecture の縦串を初めて通す**骨格テンプレート**としての役割を担う。Phase 2（daemon 経由）への移行時、`usecase` / `presenter` / `input` / `view` / `error` レイヤは**一切変更不要**で、`lib.rs::run()` 内の Repository 構築 1 行のみが差し替わる。この境界設計が本 feature の最大の価値である。

**Issue #26 で実体化を確認**: `daemon-ipc` feature により、Phase 2 移行契約は実際の実装変更（`--ipc` フラグ + `match args.ipc` 分岐 1 箇所のみ）で完了した。本 feature の設計が予言した「1 行差し替え」が文字通り実現され、骨格テンプレートとしての価値が証明された。後続 feature（ホットキー / クリップボード / 暗号化 / GUI）も `IpcRequest` の `#[non_exhaustive] enum` バリアント追加で同パターンを踏襲する。

疑似コードや実装サンプルを書かずに、**型・責務・依存方向・Fail Fast の契約**のみを設計書に残した。実装担当（坂田銀時）は、本設計書の契約を満たす限りにおいて実装詳細を自由に決定できる。
