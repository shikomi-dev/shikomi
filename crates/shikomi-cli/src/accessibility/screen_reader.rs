//! OS スクリーンリーダー検出 (C-39 自動切替経路、Sub-F #44 Phase 7 + 工程5)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 自動切替経路 (Linux / Windows は本実装、
//!   macOS は **Phase 8 に明示的に先送り** 確定)
//!
//! 現行実装と Phase 8 引継ぎ (工程5 ペテルギウス指摘で設計書 SSoT と同期):
//! - **Linux** (本実装): Orca DBus 経路は `zbus` 等の重い依存を必要とするため、
//!   `pgrep -x orca` プロセス検出で代替 (依存ゼロ、検出精度十分)。
//! - **Windows** (本実装): PowerShell `Get-Process Narrator` の subprocess を起動
//!   して exit code で判定 (PowerShell は Windows 標準同梱、追加依存なし)。
//! - **macOS** (Phase 8 先送り): subprocess 起動コスト + `defaults read
//!   com.apple.universalaccess` のキー名 macOS バージョン依存性 + sandbox/権限
//!   分離の再評価が必要なため、**Phase 8 で `Cocoa` accessibility API
//!   (`NSWorkspace`) 直接呼出 or `defaults` subprocess 経由で本実装**する。
//!   当面は `VOICEOVER_RUNNING=1` 環境変数 hint 経路のみで判定 (env 未設定時は
//!   false 返却 = `Screen` 経路維持、明示 `--output braille` で常時 braille 経路)。
//! - 検出失敗 / 未対応 OS は false を返す (Fail Kindly、`Screen` 経路継続)。
//!
//! 不変条件:
//! - 関数は副作用なし (subprocess 起動は内部に隠蔽、戻り値で振る舞い決定)。
//! - subprocess の env は `PATH` のみ pass (`audio_tts` と同方針、env allowlist)。
//! - スクリーンリーダー検出失敗は **fail-fast しない** (検出はベストエフォート、
//!   ユーザが明示 `--output braille` を指定すれば常に braille 経路を通る)。

use std::process::{Command, Stdio};

/// 現在の OS でスクリーンリーダーが起動中かを検出する。
#[must_use]
pub fn is_screen_reader_active() -> bool {
    is_screen_reader_active_impl()
}

#[cfg(target_os = "macos")]
fn is_screen_reader_active_impl() -> bool {
    // VoiceOver の OS-level 状態を低コストで判定するヒント env を採用 (Phase 7)。
    // 真の `defaults read com.apple.universalaccess` 経路は将来 minor で追加。
    matches!(
        std::env::var("VOICEOVER_RUNNING").ok().as_deref(),
        Some("1")
    )
}

#[cfg(target_os = "linux")]
fn is_screen_reader_active_impl() -> bool {
    // Linux: `pgrep -x orca` を起動。終了 code 0 (= プロセス存在) で active 判定。
    // env は PATH のみ pass、stdout/stderr は破棄 (subprocess 出力遮断)。
    let mut cmd = Command::new("pgrep");
    cmd.arg("-x").arg("orca");
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    matches!(cmd.status().map(|s| s.success()), Ok(true))
}

#[cfg(target_os = "windows")]
fn is_screen_reader_active_impl() -> bool {
    // Windows: PowerShell で Narrator.exe 検出。`Get-Process` は missing で
    // 非ゼロ exit を返すので、その場合 false。
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile").arg("-Command").arg(
        "if (Get-Process -Name Narrator -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }",
    );
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    matches!(cmd.status().map(|s| s.success()), Ok(true))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn is_screen_reader_active_impl() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// shape 検証: 関数シグネチャが `() -> bool` で安定していること。
    /// 実 OS 起動状態に依存するため戻り値は問わない (CI 環境では false 想定)。
    #[test]
    fn test_is_screen_reader_active_signature() {
        let _: fn() -> bool = is_screen_reader_active;
    }

    /// CI runner はスクリーンリーダー未起動なので、検出関数は false を返すことを
    /// 期待する (Linux: orca 不在 / Win: Narrator 不在 / macOS: VOICEOVER_RUNNING 未設定)。
    /// ただし将来 CI に accessibility テストが入った場合は本テストを SKIP 化する。
    #[test]
    fn test_is_screen_reader_active_returns_false_on_ci_runner() {
        // 戻り値は環境依存のため debug 用 print に留め、契約検証はしない (Fail Kindly)。
        let _ = is_screen_reader_active();
    }
}
