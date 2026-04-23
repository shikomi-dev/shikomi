# 基本設計書 — error（エラーハンドリング方針 / 禁止事項 / 確認強制の型レベル実装）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: cli-vault-commands / Issue #TBD -->
<!-- 配置先: docs/features/cli-vault-commands/basic-design/error.md -->
<!-- 兄弟: ./index.md, ./security.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。Rust シグネチャが必要な場合はインライン `code` で示す。

## エラーハンドリング方針

| 例外種別 | 処理方針 | ユーザへの通知 |
|---------|---------|----------------|
| clap パース失敗（不正フラグ / 未知サブコマンド） | clap の `ErrorKind` を判定。`DisplayHelp` / `DisplayVersion` は clap 自動出力 + 終了コード 0、その他は stderr に clap 生成メッセージを流しつつ**終了コードを 1 に揃える**（clap デフォルトは 2） | clap 生成メッセージ + `ExitCode::UserError` |
| フラグ併用違反（`--value` + `--stdin` 等） | `run()` で検出 → `CliError::UsageError` | MSG-CLI-100 相当（英語 + 日本語） |
| ドメイン検証失敗（`RecordLabel::try_new` / `RecordId::try_from_str`） | `run()` で検出 → `CliError::InvalidLabel(DomainError)` / `InvalidId(DomainError)` | MSG-CLI-101 / 102 |
| vault 未作成（`add` 以外） | UseCase で検出 → `CliError::VaultNotInitialized(PathBuf)` | MSG-CLI-104 |
| 対象レコード不存在 | UseCase で `find_record` → `None` → `CliError::RecordNotFound(id)` | MSG-CLI-106 |
| 暗号化 vault 検出 | UseCase で `protection_mode()` チェック → `CliError::EncryptionUnsupported` | MSG-CLI-103（終了コード 3） |
| 非 TTY で `remove --yes` 未指定 | `run()` で検出 → `CliError::NonInteractiveRemove` | MSG-CLI-105 |
| `PersistenceError::{Io,Locked,Permission}` | UseCase で `CliError::Persistence(...)` にラップ | MSG-CLI-107 |
| `PersistenceError::Corrupted` | 同上 | MSG-CLI-108 |
| 想定外の `DomainError`（集約の整合性バグ等） | UseCase で `CliError::Domain(...)` にラップ | MSG-CLI-109（内部バグ報告誘導） |
| panic（プログラムバグ） | `std::panic::set_hook` で登録した hook が**固定文言**のみ stderr に出力（`info.payload()` を一切参照しない）。`tracing` マクロを呼ばない。終了コード 2 で process exit（`std::process::abort` ではなく通常の panic unwind を経由） | MSG-CLI-109 + 終了コード 2 |

**`CliError` → `ExitCode` 写像**:

| バリアント | `ExitCode` |
|-----------|-----------|
| `UsageError(..)` | `UserError (1)` |
| `InvalidLabel(..)` | `UserError (1)` |
| `InvalidId(..)` | `UserError (1)` |
| `RecordNotFound(..)` | `UserError (1)` |
| `VaultNotInitialized(..)` | `UserError (1)` |
| `NonInteractiveRemove` | `UserError (1)` |
| `Persistence(..)` | `SystemError (2)` |
| `Domain(..)` | `SystemError (2)` |
| `EncryptionUnsupported` | `EncryptionUnsupported (3)` |

**`impl From<&CliError> for ExitCode`** を一方向写像で定義（所有権を奪わないため `&CliError` 受け）。詳細設計 `../detailed-design/data-structures.md` 参照。

## 確認強制の型レベル実装（REQ-CLI-011 の実装契約）

`remove` の削除確認は `bool` + `debug_assert!` ではなく、**型の存在そのもの**で強制する。

- UseCase 入力型: `struct ConfirmedRemoveInput { id: RecordId }`（`input.rs`）
- **`confirmed: bool` フィールドは持たない**
- 構築経路: `ConfirmedRemoveInput::new(id: RecordId) -> ConfirmedRemoveInput`（pub）
- `run()` は以下のいずれかを満たさない限り `ConfirmedRemoveInput::new(..)` を呼ばない:
  - `args.yes == true`
  - `args.yes == false && is_stdin_tty() == true && プロンプトで y/Y 回答`
- それ以外の経路（非 TTY + `--yes` 未指定）は `CliError::NonInteractiveRemove` で Fail Fast し、UseCase 到達前に return

**比較優位**:

| 実装方式 | Fail Fast タイミング | 契約強度 |
|---------|------------------|---------|
| `RemoveInput { confirmed: bool }` + `debug_assert!(input.confirmed)` | **release ビルドでは削除実行される**（debug_assert! が no-op） | 弱（運用バグでスキップ可能） |
| `ConfirmedRemoveInput { id }` 型化（本設計の採用案） | **コンパイル時**に構築経路が制約される | 強（型で保証） |

`debug_assert!` で済ます初版設計はペテルギウス指摘 ⑤で却下された。型で表現可能な事前条件は型で表現する（Parse, don't validate）。

## 禁止事項（本 feature での実装規約）

- `Result<T, String>` / `Result<T, Box<dyn Error>>` をモジュール公開 API で使わない（情報欠損）
- `unwrap()` / `expect()` を本番コードパスで使わない（テストコードは許容、ただし `expect("reason")` で理由必須）
- `?` を使う前に UseCase 層で `CliError` に `From` 変換を明示的に定義する。UseCase の戻り値は必ず `Result<_, CliError>` で揃える
- **`From<DomainError> for CliError` は実装しない**（`DomainError` のバリアントごとに適切な `CliError` へ写像する必要があるため、UseCase 側で明示的に `match` する。`?` で安易にラップしないことで、設計意図の可視化）
- エラーを握り潰さない。`if let Err(_) = ... {}` を無言で通過しない
- `eprintln!` / `println!` を UseCase から呼ばない（`lib.rs::run()` と `presenter` のみ）
- `tracing` マクロに `SecretString` / `PanicHookInfo` / panic payload を含む値を渡さない（`tracing` は type-erase するため `Debug` 経路で redact はされるが、**経路自体を設けない**方針で追加防壁）
- **`std::env::set_var` / `std::env::remove_var` を本番コードで呼ばない**（thread-unsafe）。テストでも使わず `assert_cmd::Command::env()` で環境を注入
- **panic hook 内で `info.payload()` / `info.message()` / `info.location()` の値を参照しない**（`./security.md §panic hook` 参照）
- **`expose_secret()` を `shikomi-cli/src/` 内で呼ばない**（契約、CI grep で検証、`./security.md §expose_secret 経路監査` 参照）

## i18n 扱い

- `CliError::Display` は**英語原文のみ**を返す（単純化）。日本語併記は `presenter::error::render_error(&err, locale)` の責務
- `Locale` は `run()` 起動時に 1 度だけ `Locale::detect_from_env()` で決定し、以降は値として渡す（env 操作のテスト容易性）
- 日本語判定ルール: `LANG` 環境変数の先頭 2 文字を大文字小文字無視で `"ja"` と比較。該当すれば `Locale::JapaneseEn`（英語 + 日本語の 2 段表示）、それ以外は `Locale::English`
- **panic hook 内で `Locale` をどう参照するか**: `run()` で決定した `Locale` を `std::sync::OnceLock<Locale>` に格納し、panic hook 内で読む（副作用的 I/O なしで参照可能）。これは `./security.md §panic hook` の「payload 非参照」と両立する（`OnceLock` は `Copy` type の Locale を返すだけで、panic 情報を触らない）

## テスト設計への引き継ぎ

本 feature のエラー系挙動について、テスト設計担当（涅マユリ）には以下の観点を伝える:

1. `CliError` 全 9 バリアント × `ExitCode` 写像の組合せ（UT）
2. `render_error` の英語単独 / 英日併記の出力内容検証（UT）
3. 暗号化 vault 検出時の終了コード 3 検証（E2E、フィクスチャ vault 使用）
4. 非 TTY `remove` の `NonInteractiveRemove` 経路（E2E、`Stdio::piped()` で非 TTY 化）
5. `ConfirmedRemoveInput` のコンパイル時安全性（UT では構築テスト + doc-test で「bool を渡そうとすると compile error」を示す）
6. panic hook 動作（UT、`std::panic::catch_unwind` で意図的 panic）
7. clap の `ErrorKind` 分岐（`DisplayHelp` / `DisplayVersion` は終了コード 0、他は 1）
8. CI grep 系（TC-CI-013/014/015、`./security.md §expose_secret 経路監査` に詳細）
