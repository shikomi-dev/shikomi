# 基本設計書 — security（脅威モデル / OWASP / 漏洩経路監査）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-default-mode / sub-feature: cli / Issue #126 -->
<!-- 配置先: docs/features/daemon-default-mode/cli/security.md -->
<!-- 兄弟: ./basic-design.md, ./detailed-design.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。Rust シグネチャが必要な場合はインライン `code` で示す。

## 脅威モデル

本 feature は `daemon-ipc` / `vault-persistence` で確立された脅威モデルを**基盤として継承する**。Phase 2 移行（IPC 既定化）によって新たに開く攻撃面・状態変化のみを本表に追記する。

| 想定攻撃 / 事故 | 経路 | 保護資産 | 対策 |
|--------------|------|---------|------|
| **daemon 起動中に `--no-ipc` で同時アクセス** | Phase 2 では daemon が常時起動する。ユーザー / スクリプトが `shikomi --no-ipc list` を daemon 起動中に叩くシナリオが Phase 1 より激増する。`--no-ipc` 経路は `SqliteVaultRepository::from_directory` → `VaultLock::acquire_exclusive` へ進むため、daemon が既に `VaultLock` を保持している場合は競合する | vault.db の整合性 | `VaultLock::acquire_exclusive` が `PersistenceError::Locked { holder_hint }` で **Fail Fast**（`vault-persistence/basic-design/security.md §VaultLock` の既存保証を継承）。vault 破損は原理的に発生しない。`--no-ipc` 経路が単独で DACL 検証・VaultLock 確認を行うため多重書き込みによる破壊は構造的に排除されている |
| **`--no-ipc` 経路での vault.db への不正アクセス** | `--no-ipc` 指定時は `SqliteVaultRepository::from_directory` を経由して vault.db に直接アクセスする。Phase 1 の既定経路と同等 | vault.db ファイル（機密エントリの平文値）| `VaultPaths::new` + `verify_dir`（DACL `0700` / Windows ACL）が全経路で実行される。Phase 2 で `--no-ipc` を追加しても新たな権限チェック漏れは発生しない（`vault-persistence/basic-design/security.md §A01` 参照）|
| **REQ-DDM-005: vault サブコマンドの `--no-ipc` バイパス試行** | ユーザーが `--no-ipc vault encrypt` を実行して vault 管理の IPC 強制を回避しようとする | vault 一貫性（vault 管理は daemon 経由が Phase 2 規定）| `run_vault` / `connect_vault_ipc` は `args.no_ipc` を**引数として受け取らない構造**（`vault_dir: Option<&Path>` のみ受け取る）。将来 `run_vault(vault, args)` のようにシグネチャを変更すれば `args.no_ipc` が参照できてしまう構造的リスクが生まれるため、**`run_vault` のシグネチャに `CliArgs` を渡してはならない**（設計契約）。現在の構造的分離は CI grep `"no_ipc" crates/shikomi-cli/src/lib.rs` の期待件数（2 件のみ）で継続監査する |
| **`MSG-CLI-052` 経由の情報漏洩** | vault サブコマンド + `--no-ipc` 組み合わせ検出時の note 出力 | なし（固定文言）| `MSG-CLI-052` は**完全な固定文言**（`"note: vault commands always use IPC; --no-ipc does not apply"`）。動的フィールドなし。secret 非含有が構造的に保証される |
| **`MSG-CLI-110` note / hint 経由のパス漏洩** | daemon 未起動時に `MSG-CLI-110` hint 行で socket パス `{path}` が出力される | socket ファイルパス情報 | `{path}` は socket ファイルの絶対パス（`XDG_RUNTIME_DIR/shikomi.sock` 相当）。Phase 1 からの継続挙動であり、secret 非含有・Phase 1 の脅威モデルで受容済み（`daemon-ipc/basic-design/security.md §脅威モデル` 参照）。Phase 2 で新たなリスクは生じない |
| **`--no-ipc` を使った緊急復旧時の監査証跡欠落** | daemon 未起動 / クラッシュ時に `--no-ipc` で直接 vault を操作した場合、daemon の `tracing` ログに記録されない | 監査証跡（A09）| `--no-ipc` 使用時は `tracing::warn!` レベルで「direct SQLite access requested via --no-ipc」を `target: "shikomi_cli::composition_root"` で出力する（実装時に追加する設計観点。`quiet` フラグの有無に関わらず tracing は出力する）|

## OWASP Top 10 対応

本 feature は `cli-vault-commands` / `daemon-ipc` の対応を継承した上で、Phase 2 移行固有の追加対応を記述する。

| # | カテゴリ | 対応状況 |
|---|---------|---------|
| A01 | Broken Access Control | **対応** — `--no-ipc` 経路でも `VaultPaths::new` + `verify_dir` による DACL 検証（`0700` ディレクトリ + `0600` ファイル、Unix / Windows ともに有効）が `SqliteVaultRepository::from_directory` を通じて実行される（`vault-persistence/basic-design/security.md §A01` 引用）。vault サブコマンドは REQ-DDM-005 の構造的強制で IPC のみ。`--no-ipc` 指定時の vault サブコマンドバイパスは型レベルで不可能 |
| A02 | Cryptographic Failures | **対応（変化なし）** — 本 feature は IPC / SQLite の経路選択のみを変更する。暗号処理は `vault-encryption` feature のスコープ。Phase 2 で暗号化の扱いに変更はない |
| A03 | Injection | **対応（変化なし）** — `--no-ipc` 経路は `SqliteVaultRepository` を経由するため、既存の `rusqlite` パラメータバインディングがすべての SQL インジェクション経路を封じる（`vault-persistence` 継承）|
| A04 | Insecure Design | **対応** — IPC をデフォルトにする（Secure Default）設計。`--no-ipc` は例外的な後退。`MSG-CLI-052` でユーザーへ通知することで「沈黙での上書き」を排除（ペガサス指摘対応）|
| A05 | Security Misconfiguration | **対応** — `build_handle` の既定値（`no_ipc == false`）が IPC 経路（daemon 経由）。Secure Default が維持されている。`--no-ipc` フラグは明示的オプトアウト専用（デフォルト安全）。`CliArgs::no_ipc: bool` の `Default::default()` が `false`（clap / Rust の bool デフォルト）であることで型レベルで保証 |
| A06 | Vulnerable Components | **対応（変化なし）** — 本 feature は新規 crate を追加しない。`clap` のバージョンは `cli-vault-commands` で固定済み（`cargo-deny` + Dependabot で継続監査）|
| A07 | Auth Failures | **対応（変化なし）** — 認証モデルは `daemon-ipc` で定義済み（ピア UID/SID 検証）。`--no-ipc` 経路の認証は OS ファイルシステム権限（DACL）に委ねる（Phase 1 と同等）|
| A08 | Data Integrity Failures | **対応** — `--no-ipc` 経路での同時アクセスは `VaultLock::acquire_exclusive` が Fail Fast で防止（脅威モデル表「daemon 起動中に `--no-ipc` で同時アクセス」参照）。vault の atomic write は `vault-persistence` の既存保証を継承 |
| A09 | Logging Failures | **対応** — `--no-ipc` 使用時は `tracing::warn!` で記録（脅威モデル表「緊急復旧時の監査証跡欠落」参照）。`MSG-CLI-052` / `MSG-CLI-110` は固定文言のみで secret 非含有。`cli-vault-commands` の `expose_secret` 0 件 CI grep 監査対象に `--no-ipc` 経路のコードも含まれる（既存 grep 範囲：`crates/shikomi-cli/src/`）|
| A10 | SSRF | 対象外（変化なし）— CLI / daemon とも HTTP / 外部 URL アクセスを行わない |

## 親 security.md への相互参照

| 参照先 | 参照理由 |
|-------|---------|
| `docs/features/vault-persistence/basic-design/security.md §VaultLock` | `--no-ipc` 経路での VaultLock Fail Fast 保証の一次情報源 |
| `docs/features/vault-persistence/basic-design/security.md §A01` | DACL 検証（`verify_dir`）が `--no-ipc` 経路でも有効であることの一次情報源 |
| `docs/features/daemon-ipc/basic-design/security.md §脅威モデル` | IPC 経路（既定経路）のセキュリティ設計全体の一次情報源。Phase 2 でも引き続き有効 |
| `docs/features/daemon-ipc/basic-design/security.md §A05` | IPC ソケットパーミッション（`0600`）/ Named Pipe SDDL の一次情報源 |
| `docs/architecture/context/threat-model.md` | システム全体の STRIDE 分析・信頼境界の一次情報源。本書は feature スコープの差分のみ記述 |

## `unsafe_code` の扱い

本 feature は `crates/shikomi-cli/src/` に新たな `unsafe` ブロックを追加しない。`--no-ipc` 経路の実装（`build_handle` 分岐反転 / `CliArgs` フィールド変更）はすべて safe Rust で完結する。workspace lint `unsafe_code = "deny"` は本 feature で変更不要。

## CI 監査ゲート（本 feature スコープ）

| 監査項目 | grep コマンド / 期待結果 |
|---------|----------------------|
| `args.ipc` 参照ゼロ件（廃止確認）| `grep -rn "args\.ipc\b" crates/shikomi-cli/src/` → 0 件 |
| `MSG-CLI-051` / `ipc_opt_in` 参照ゼロ件（廃止確認）| `grep -rn "MSG-CLI-051\|ipc_opt_in\|render_ipc_opt_in" crates/shikomi-cli/src/` → 0 件 |
| `no_ipc` 参照件数（vault 経路の IPC 強制確認）| `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` → 2 件のみ（vault dispatch + `build_handle`）|
| `expose_secret` 0 件（secret 漏洩経路の遮断）| `grep -rn "expose_secret" crates/shikomi-cli/src/` → 0 件 |
