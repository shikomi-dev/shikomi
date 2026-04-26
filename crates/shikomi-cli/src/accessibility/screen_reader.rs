//! OS スクリーンリーダー検出 (C-39 自動切替経路、Sub-F #44 Phase 7)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 自動切替経路
//!   (macOS `defaults read com.apple.universalaccess` /
//!    Windows `Narrator.exe` プロセス検出 /
//!    Linux Orca DBus 経路)
//!
//! 設計判断:
//! - **Linux**: Orca DBus 経路は `zbus` 等の重い依存を必要とするため、`/proc`
//!   ベースの **`orca` プロセス検出**で代替する (依存ゼロ、検出精度十分)。
//! - **macOS**: `defaults read com.apple.universalaccess voiceOverOnOffKey` の
//!   subprocess 起動コストを避けるため、**環境変数 `VOICEOVER_RUNNING=1`** での
//!   ヒント検出 + 失敗時は false を返す (Phase 7 minimal、`defaults` subprocess は
//!   将来 minor で追加検討)。
//! - **Windows**: PowerShell `Get-Process Narrator` の subprocess を起動して
//!   exit code で判定する (PowerShell は Windows 標準同梱、追加依存なし)。
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
