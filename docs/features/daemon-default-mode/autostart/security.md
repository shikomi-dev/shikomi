# セキュリティ設計書 — daemon-default-mode / autostart（脅威モデル / OWASP / 漏洩経路監査）

<!-- 詳細設計書とは別ファイル。統合禁止 -->
<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/security.md -->
<!-- 兄弟: ./basic-design.md, ./detailed-design.md -->

## 記述ルール

本書には**疑似コード・サンプル実装を書かない**（設計書共通ルール）。Rust シグネチャが必要な場合はインライン `code` で示す。

## 脅威モデル

本 feature は `daemon-ipc` / `vault-persistence` / `cli` の各 `security.md` が確立した脅威モデルを**基盤として継承する**。`shikomi daemon install/uninstall/status` サブコマンドが新たに開く攻撃面のみを本表に記述する。

| 想定攻撃 / 事故 | 経路 | 保護資産 | 対策 |
|--------------|------|---------|------|
| **plist / unit / .desktop ファイルへのシンボリックリンク攻撃** | `install()` が `~/Library/LaunchAgents/dev.shikomi.daemon.plist`（macOS）/ `~/.config/systemd/user/shikomi-daemon.service`（Linux）/ `~/.config/autostart/shikomi-daemon.desktop`（XDG）に `std::fs::write` で書き込む。攻撃者が事前にシンボリックリンクを設置した場合、リンクをたどって任意ファイルを上書きするリスクがある | OS 外部ファイル（ログ・設定）の整合性 | ホームディレクトリ（`~`）は原則ユーザー所有（Unix `0700`）。他ユーザーが `~/.config/` 以下にシンボリックリンクを設置することは、POSIX 標準の `0700` ホームディレクトリ権限下では構造的に不可能。`create_dir_all` でディレクトリを先に作成してから `write` することで、ディレクトリ自体の置き換えも防ぐ。Windows では `USERPROFILE` 配下は ACL で他ユーザー書き込み禁止（既定）。追加の `canonicalize()` は不要（パス自体がホームディレクトリ相対で固定されているため） |
| **`resolve_daemon_path()` が悪意あるバイナリを解決する** | `std::env::current_exe()` が返すパスにシンボリックリンクを仕掛け、`shikomi-daemon` 以外の実行ファイルを autostart に登録させる | autostart 登録の完全性 | `current_exe()` の直後に `canonicalize()` を呼び出してシンボリックリンクを解決する（real path を取得）。`daemon_path.exists()` で存在確認（Fail Fast）。`PATH` 検索ではなく実行ファイルと同ディレクトリへの固定解決を採用するため、`PATH` hijacking は適用不能（`detailed-design.md §resolve_daemon_path()`）|
| **launchctl / systemctl / schtasks へのコマンドインジェクション** | `daemon_path` にスペース・セミコロン・改行等の特殊文字が含まれる場合、コマンドラインへの展開でインジェクションが起きるリスク | OS の実行権限 | `std::process::Command::new("launchctl").arg("bootstrap").arg("...").arg(plist_path)` の形式で引数を配列渡しする（シェルを経由しない `execve` 直接呼び出し）。シェルメタキャラクタはすべて引数の一要素として扱われ、インジェクションは原理的に不可能。plist / unit ファイルへのパスは `std::fs::write` で書き込み済みの静的テンプレートであり、`daemon_path` は XML エンティティエスケープ（`&` → `&amp;` 等）が不要な `<string>` 要素への埋め込みにとどまる。plist 書き込みは `std::fs::write` で完結するため XML パーサー由来の脆弱性は発生しない |
| **`AutostartError::CommandFailed` の stderr による情報漏洩** | `launchctl` / `systemctl` / `schtasks` コマンドが失敗した際の stderr には OS 固有のエラーメッセージ（サービス名・パス・エラーコード）が含まれる場合がある | 内部パス情報（secret 非含有）| `AutostartError::CommandFailed::stderr_excerpt` は**最初の 80 文字のみ**を格納（`detailed-design.md §AutostartError`）。secret（パスワード・API キー・秘密鍵）はコマンド引数・stderr のいずれにも出現しない（`launchctl` / `systemctl` / `schtasks` は認証情報を引数に取らない）。80 文字上限は OS エラーの識別に十分であり、余剰な内部パス情報の漏洩を抑制する |
| **`shikomi daemon install` の冪等性実装における TOCTOU 競合** | 「登録済み確認 → 登録実行」の間に別プロセスが同コマンドを実行した場合の競合 | autostart 設定ファイルの整合性 | macOS launchd: `launchctl bootout`（事前解除）→ `bootstrap`（再登録）の 2 ステップで原子性を確保。`bootout` の失敗は無視（冪等）。Linux systemd: `systemctl --user enable --now` は unit ファイル上書き後に実行するため、複数同時実行でも最後の `enable` が有効（べき等性は systemd が保証）。XDG: `O_TRUNC` 等価の `std::fs::write` で上書き（POSIX 保証）。Windows schtasks: `/F` フラグで強制上書き。最悪ケースでも設定ファイルが同一内容で上書きされるだけであり、corrupt な中間状態は生じない |
| **`shikomi daemon status` の IPC probe による副作用** | `IpcVaultRepository::connect` が daemon に handshake 以上の副作用を与える可能性 | daemon の実行状態 | `IpcVaultRepository::connect` は Unix socket / Named Pipe への TCP 接続確立のみ。vault 操作・handshake 完了は行わない（接続 `Err` を `not running` と判定するため、成功時も即座にドロップする）。副作用ゼロ。REQ-DDM-012「情報提供のみ、副作用なし」の設計規約を構造的に実現 |
| **自動起動ファイルのパーミッション不備** | plist / unit / .desktop ファイルが過剰なパーミッション（`0777` 等）で作成された場合、他ユーザーが内容を改ざんできる | autostart 設定の完全性 | `std::fs::write` は `umask` の影響を受ける。Unix 標準の `umask 0022` 下では `0644` が既定（ユーザー書き込み可・他ユーザー読み取り可のみ）。これは launchd / systemd の標準的なパーミッション（plist: `0644`、unit: `0644`）と一致する。他ユーザーからの**書き込みは禁止**される。追加の `chmod` 呼び出しは不要 |
| **`shikomi daemon install` 実行時の daemon_path 不在 → 不完全な登録** | `resolve_daemon_path()` 成功後・install 完了前に `shikomi-daemon` バイナリが削除された場合 | autostart 設定の有効性 | Fail Fast 原則（`detailed-design.md §resolve_daemon_path()`）: `resolve_daemon_path()` は `exists()` 確認を含むため、バイナリ不在なら `AutostartError::IoError(NotFound)` で即時失敗する。ファイル書き込み後のバイナリ削除は autostart 登録を無効化するが、これは OS 全般の共通挙動（launchd / systemd は起動時にパスを再解決する）であり、autostart 機能の脅威範囲外 |

## OWASP Top 10 対応

本 feature は `daemon-ipc` / `cli` の各 OWASP 対応を継承した上で、`shikomi daemon install/uninstall/status` が固有に開く攻撃面を追記する。

| # | カテゴリ | 対応状況 |
|---|---------|---------|
| A01 | Broken Access Control | **対応** — `install()` が書き込む autostart ファイルの配置先（`~/Library/LaunchAgents/`、`~/.config/systemd/user/`、`~/.config/autostart/`）はいずれも現在のユーザーのホームディレクトリ配下。OS の POSIX / ACL 権限モデルにより、他ユーザーはこれらのパスに書き込めない。`schtasks /Create /TN "shikomi\shikomi-daemon"` はログオン中のユーザーのタスクスケジューラ名前空間に作成されるため、管理者権限は不要かつ他ユーザーのタスクには影響しない |
| A02 | Cryptographic Failures | **対応（変化なし）** — autostart 機能は暗号処理を行わない。`shikomi-daemon` バイナリ自体のコード署名は Issue #132（本番署名整備）のスコープ。本 feature で暗号処理に変更はない |
| A03 | Injection | **対応** — `std::process::Command` の配列引数渡しにより shell injection は原理的に不可能（脅威モデル表「コマンドインジェクション」参照）。plist / unit / .desktop テンプレートはすべてハードコードされた静的テンプレートであり、ユーザー入力を埋め込まない。`daemon_path` のみが動的だが XML / INI 構造上で無害（`<string>` 要素 / `Exec=` 行への単純文字列展開）|
| A04 | Insecure Design | **対応** — `AutostartBackend::detect()` はコンパイル時 `#[cfg(target_os = ...)]` で OS 別実装を選択する（実行時 OS 文字列比較ではなく型レベルの選択）。Strategy パターンで OS 別実装が差し替え可能。`--no-ipc` フラグは `install` / `uninstall` に影響しない設計（REQ-DDM-013 / `basic-design.md §DaemonSubcommand の CLI 仕様`） |
| A05 | Security Misconfiguration | **対応（Secure Default）** — `shikomi daemon install` を明示実行しない限り autostart は登録されない（Opt-in。自動的に autostart を有効化する初期化コードはない）。`shikomi daemon status` のデフォルト出力は `"autostart: disabled"`（未登録状態が既定）。自動起動ファイルのパーミッションは OS 標準（`umask 0022` → `0644`、他ユーザー書き込み禁止）|
| A06 | Vulnerable Components | **対応** — 新規外部 crate は `dirs = "5"` のみ追加（`dirs` は `home_dir()` 解決専用、I/O を直接行わず OS の標準 API を薄くラップする）。`dirs` crate は `cargo-deny` + Dependabot の継続監査対象に追加する。`which` / `nix` は既存依存のまま変更なし |
| A07 | Auth Failures | **対応（変化なし）** — autostart 登録・解除は現ユーザーのスコープ内のみ（root 権限・管理者権限不要）。systemd user unit、launchd LaunchAgent、XDG Autostart はいずれもユーザーセッションスコープ。Windows schtasks はログオン中のユーザーのタスクとして作成（SYSTEM スコープ不使用）。認証モデルは OS のユーザー分離に完全委譲 |
| A08 | Data Integrity Failures | **対応** — autostart ファイル書き込みは `std::fs::write`（POSIX の `O_TRUNC` 等価）。atomic write ではないが、設定ファイルの部分書き込み失敗は `AutostartError::IoError` で Fail Fast する。不完全な plist / unit ファイルが残留した場合、次回 `shikomi daemon install` で上書きされる（冪等性 / Boy Scout Rule）。vault.db 自体のデータ整合性には autostart は関与しない |
| A09 | Logging Failures | **対応** — `install()` / `uninstall()` の成功は stdout に固定文言（`"shikomi-daemon autostart enabled"` / `"disabled"`）で出力。`AutostartError::CommandFailed` のエラーはシステムログ（launchd の Console.app / systemd の journald / Windows Event Log）にも OS 側で記録される。`stderr_excerpt` の 80 文字上限により過剰な情報の出力を抑制。`shikomi daemon status` は副作用なし・audit 対象外（情報照会のみ）。`install()` / `uninstall()` 自体に `tracing::info!` を追加する（実装担当が確認すること：target `"shikomi_cli::autostart"`、メッセージ: `"autostart install: backend={backend_name}"` / `"autostart uninstall"`）|
| A10 | SSRF | **対象外（変化なし）** — autostart 機能は外部 URL / HTTP リクエストを一切発行しない。IPC probe は Unix socket / Named Pipe への接続であり、外部ネットワークへのアクセスはない |

## 親 security.md への相互参照

| 参照先 | 参照理由 |
|-------|---------|
| `docs/features/daemon-ipc/basic-design/security.md §脅威モデル` | IPC 経路（`shikomi daemon status` の IPC probe）のセキュリティ設計全体の一次情報源 |
| `docs/features/daemon-ipc/basic-design/security.md §A05` | IPC ソケットパーミッション（`0600`）/ Named Pipe SDDL の一次情報源。status の probe が既存ソケット権限を前提とすることの根拠 |
| `docs/features/daemon-default-mode/cli/security.md §脅威モデル` | `--no-ipc` 経路と daemon 起動中の同時アクセス（VaultLock 競合）の一次情報源。autostart install/uninstall は VaultLock 不要（ファイル操作・OS コマンドのみ）のため競合しない |
| `docs/features/daemon-default-mode/cli/security.md §CI 監査ゲート` | `no_ipc` 参照件数（lib.rs で 3 件: vault dispatch + build_handle + daemon status IPC probe 分岐）の監査規約。autostart モジュールは `no_ipc` を直接参照しない（lib.rs の dispatch 層が担う）|
| `docs/architecture/context/threat-model.md` | システム全体の STRIDE 分析・信頼境界の一次情報源。本書は autostart feature スコープの差分のみ記述 |

## `unsafe_code` の扱い

本 feature は `crates/shikomi-cli/src/autostart/` に新たな `unsafe` ブロックを追加しない。各 OS Backend の実装（plist 書き込み / systemctl 呼び出し / schtasks 呼び出し）はすべて safe Rust の `std::process::Command` / `std::fs` / `std::env` で完結する。`nix::unistd::getuid()` は `nix` crate の safe API。workspace lint `unsafe_code = "deny"` は本 feature で変更不要。

## CI 監査ゲート（本 feature スコープ）

| 監査項目 | grep コマンド / 期待結果 |
|---------|----------------------|
| `no_ipc` 参照件数（autostart モジュールが lib.rs の dispatch 層のみを通じて参照されていること）| `grep -n "no_ipc" crates/shikomi-cli/src/lib.rs` → 3 件のみ（vault dispatch + `build_handle` + `daemon status` IPC probe 分岐）|
| `autostart` モジュールが `no_ipc` を直接参照していないこと | `grep -rn "no_ipc" crates/shikomi-cli/src/autostart/` → 0 件 |
| `DaemonSubcommand` が `cli.rs` にのみ定義されていること | `grep -rn "DaemonSubcommand" crates/shikomi-cli/src/` → `cli.rs` の定義のみ（`lib.rs` での参照は許容）|
| `expose_secret` 0 件（secret 漏洩経路の遮断）| `grep -rn "expose_secret" crates/shikomi-cli/src/autostart/` → 0 件 |
| autostart ファイル書き込み先がホームディレクトリ配下のみ（絶対パスハードコード禁止）| `grep -rn '"/etc/\|"/usr/\|"C:\\\\Windows"' crates/shikomi-cli/src/autostart/` → 0 件（システムワイドパスへの書き込みがないこと）|
| `std::process::Command` の `shell` / `.arg(format!(...))` 使用ゼロ件（コマンドインジェクション防止）| `grep -rn "\.command\|::new.*sh.*-c\|shell.*true" crates/shikomi-cli/src/autostart/` → 0 件（シェル経由の実行がないこと）|
