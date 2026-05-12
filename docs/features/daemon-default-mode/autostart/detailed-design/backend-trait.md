# 詳細設計書 — autostart / AutostartBackend trait・共通ヘルパー

<!-- feature: daemon-default-mode / sub-feature: autostart / Issue #127 -->
<!-- 配置先: docs/features/daemon-default-mode/autostart/detailed-design/backend-trait.md -->
<!-- Vモデル対応: 階層 3（sub-feature 詳細設計）-->
<!-- 親目次: ./index.md -->

## `crates/shikomi-cli/src/autostart/mod.rs` の詳細

### `AutostartBackend` trait 型シグネチャ

```rust
pub trait AutostartBackend {
    /// バックエンド識別名（tracing::info! の `backend=` フィールドで使用）。
    fn name(&self) -> &'static str;

    /// OS 自動起動に daemon を登録する。冪等（登録済みなら再登録せず Ok を返す）。
    fn install(&self) -> Result<(), AutostartError>;

    /// OS 自動起動登録を解除する。冪等（未登録なら Ok を返す）。
    fn uninstall(&self) -> Result<(), AutostartError>;

    /// 自動起動登録状態を返す。probe 失敗時は `false`（Fail Safe）。
    fn is_registered(&self) -> bool;

    /// install 成功時に stdout へ追記する OS 固有の hint（None なら追記なし）。
    fn install_hint(&self) -> Option<String> {
        None
    }
}
```

**設計判断**:
- `fn name(&self) -> &'static str` を追加する（A09 監査証跡の `tracing::info!` で Backend 種別を記録するため）
- `install_hint()` にデフォルト実装（`None` 返却）を提供する（UnsupportedBackend 等の実装コスト低減）

### `AutostartError` 型定義

```rust
#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    #[error("command failed: `{cmd}`: {stderr_excerpt}")]
    CommandFailed {
        cmd: String,
        /// stderr の最初の 80 文字のみ（secret 非含有 / security.md §脅威モデル「stderr 情報漏洩」）
        stderr_excerpt: String,
    },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("unsupported: {reason}")]
    Unsupported { reason: String },
}
```

**`stderr_excerpt` の切り詰め処理**:
- `Command::output()` で取得した `stderr` を `String::from_utf8_lossy` で変換する
- 先頭 80 文字（`&stderr_str[..stderr_str.char_indices().nth(80).map(|(i,_)|i).unwrap_or(stderr_str.len())]`）を格納する
- `char_indices().nth(80)` でマルチバイト境界を考慮する

**設計判断**:
- `thiserror` crate を使用する（既存 `CliError` と同じ依存）
- `IoError` に `#[from]` を付与することで `?` 演算子によるエラー伝搬が可能

### `detect()` 関数定義

OS を判定して適切な `AutostartBackend` 実装を返す（コンパイル時 `#[cfg]` 分岐、実行時 OS 文字列比較ではない）:

```
#[cfg(target_os = "macos")] → LaunchdBackend を返す
#[cfg(target_os = "windows")] → WindowsTaskSchedulerBackend を返す
#[cfg(target_os = "linux")] → SystemdBackend::is_available() が true なら SystemdBackend、
                               false なら XdgAutostartBackend を返す
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
                             → UnsupportedBackend を返す（FreeBSD 等）
```

**`UnsupportedBackend`** は `AutostartBackend` を実装し、`install()` / `uninstall()` で `AutostartError::Unsupported { reason: "platform not supported" }` を返す。`is_registered()` は `false` を返す。

**設計判断**:
- `#[cfg(target_os = ...)]` ブロックはコンパイル時分岐。OS 不正解パスのコードがバイナリに混入しない
- `UnsupportedBackend` で panic なし（Fail Fast だが panic 回避）

### `resolve_daemon_path()` 共通ヘルパー

全 Backend から参照する共通ヘルパー関数を `mod.rs` に定義する:

処理手順:
1. `std::env::current_exe()` で現在の実行ファイル（`shikomi` CLI）のパスを取得する
2. `.canonicalize()` でシンボリックリンクを解決し real path を取得する（**シンボリックリンク攻撃防止 / `security.md §脅威モデル`**）
3. `.parent()` でディレクトリを取得する（失敗時 → `AutostartError::IoError(NotFound)`）
4. `if cfg!(target_os = "windows") { "shikomi-daemon.exe" } else { "shikomi-daemon" }` でバイナリ名を決定する
5. `dir.join(daemon_name)` でパスを構築する
6. `daemon_path.exists()` を確認する（**Fail Fast**: 不在なら `AutostartError::IoError(NotFound)`）
7. `daemon_path` を返す

**設計判断**:
- `PATH` 検索（`which::which`）ではなく同ディレクトリ固定解決を採用（配布パッケージでは `shikomi` と `shikomi-daemon` が必ず同ディレクトリに置かれる前提）
- `canonicalize()` でシンボリックリンクを解決してから同ディレクトリを参照する
- `exists()` による存在確認で install 途中のパス解決失敗を早期検知する

### モジュール宣言

```
#[cfg(target_os = "macos")]
mod launchd;

#[cfg(target_os = "linux")]
mod systemd;

#[cfg(target_os = "linux")]
mod xdg;

#[cfg(target_os = "windows")]
mod windows;
```
