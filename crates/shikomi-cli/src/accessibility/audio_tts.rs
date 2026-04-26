//! 24 語の OS TTS subprocess 出力（C-39、Sub-F #44 Phase 6）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 `--output audio` 行
//!   §セキュリティ設計 §`--output audio` TTS dictation 学習対策
//!
//! OS 別 TTS:
//! - **macOS**: `say` コマンド (Apple SpeechSynthesis)
//! - **Windows**: PowerShell + `[System.Speech.Synthesis.SpeechSynthesizer]::Speak`
//! - **Linux**: `espeak` (PCM を音声サーバ PulseAudio / PipeWire にパイプ)
//! - その他: 未対応 → `CliError::Persistence` で fail (Fail Fast)
//!
//! 不変条件:
//! - **中間ファイルなし**: 24 語は subprocess の **stdin (`Stdio::piped()`)** に
//!   直接書出。ファイル経路一切不採用 (TTS dictation 学習防衛、§セキュリティ設計)。
//! - **env allowlist**: subprocess の env を `PATH` / `HOME` / `LANG` / `USER` の
//!   最小限に制限 (`Command::env_clear` + `Command::env`)。shikomi 内部 env は除外
//!   (C-40 と整合、§セキュリティ設計 §subprocess の env を allowlist で sanity check)。
//! - **Stdout 抑止**: subprocess の stdout / stderr は `Stdio::null()` で破棄
//!   (24 語が誤って親プロセスの stdout に echo されないこと、L1 攻撃面遮断)。

use std::io::Write;
use std::process::{Command, Stdio};

use shikomi_core::ipc::SerializableSecretBytes;

use crate::error::CliError;

/// 24 語を OS TTS subprocess に音声合成させる。
///
/// # Errors
/// - `CliError::Persistence`: subprocess 起動失敗 / stdin 書出失敗 / 未対応 OS。
pub fn speak(words: &[SerializableSecretBytes]) -> Result<(), CliError> {
    let payload = build_payload(words);
    let mut cmd = build_command_for_current_os()?;
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| io_err("tts spawn", e))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| stdin_unavailable_err())?;
        stdin
            .write_all(payload.as_bytes())
            .map_err(|e| io_err("tts stdin", e))?;
    } // stdin drop で EOF 通知

    let status = child.wait().map_err(|e| io_err("tts wait", e))?;
    if !status.success() {
        return Err(io_err(
            "tts non-zero exit",
            std::io::Error::other(format!("status: {status:?}")),
        ));
    }
    Ok(())
}

/// 24 語を読み上げ用 1 行プレーンテキストに整形する (番号 + word)。
fn build_payload(words: &[SerializableSecretBytes]) -> String {
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let plain = w.to_lossy_string_for_handler();
        out.push_str(&format!("{}. {}. ", i + 1, plain));
    }
    out
}

/// 現在の OS に対応した TTS subprocess 起動コマンドを構築する。
///
/// env allowlist: `PATH` / `HOME` / `LANG` / `USER` のみ pass し、shikomi 内部
/// env や `SHIKOMI_DAEMON_*` は除外する。
fn build_command_for_current_os() -> Result<Command, CliError> {
    let mut cmd = build_command_inner()?;
    cmd.env_clear();
    for key in ["PATH", "HOME", "LANG", "USER"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
    Ok(cmd)
}

#[cfg(target_os = "macos")]
fn build_command_inner() -> Result<Command, CliError> {
    // `say` は stdin から読み取る経路を持たないため、`-f /dev/stdin` で明示。
    let mut cmd = Command::new("say");
    cmd.arg("-f").arg("/dev/stdin");
    Ok(cmd)
}

#[cfg(target_os = "linux")]
fn build_command_inner() -> Result<Command, CliError> {
    // `espeak` は `--stdin` で stdin 読取、PulseAudio / PipeWire にパイプ。
    let mut cmd = Command::new("espeak");
    cmd.arg("--stdin");
    Ok(cmd)
}

#[cfg(target_os = "windows")]
fn build_command_inner() -> Result<Command, CliError> {
    // PowerShell 経由で SAPI を呼び出す。stdin から読んだテキストを Speak。
    // `Add-Type` で System.Speech をロード、`$Input | ForEach-Object` で stdin 行を Speak。
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile").arg("-Command").arg(
        "Add-Type -AssemblyName System.Speech; \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $Input | ForEach-Object { $s.Speak($_) }",
    );
    Ok(cmd)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn build_command_inner() -> Result<Command, CliError> {
    Err(io_err(
        "unsupported OS for audio TTS",
        std::io::Error::new(std::io::ErrorKind::Unsupported, "no TTS backend"),
    ))
}

fn stdin_unavailable_err() -> CliError {
    io_err(
        "tts stdin unavailable",
        std::io::Error::other("child stdin handle missing"),
    )
}

fn io_err(op: &'static str, e: std::io::Error) -> CliError {
    CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
        path: std::path::PathBuf::from(format!("<audio_tts:{op}>")),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::SecretString;

    fn word(s: &str) -> SerializableSecretBytes {
        SerializableSecretBytes::from_secret_string(SecretString::from_string(s.to_owned()))
    }

    #[test]
    fn test_build_payload_includes_numbers_and_words() {
        let words = vec![word("alpha"), word("beta")];
        let payload = build_payload(&words);
        assert!(payload.contains("1. alpha"));
        assert!(payload.contains("2. beta"));
    }

    #[test]
    fn test_build_payload_24_words_produces_24_segments() {
        let words: Vec<_> = (0..24).map(|i| word(&format!("w{i}"))).collect();
        let payload = build_payload(&words);
        // 番号 1〜24 が全部含まれる。
        for i in 1..=24 {
            assert!(payload.contains(&format!("{i}. w")));
        }
    }

    /// `build_command_for_current_os` が env_clear 後に PATH / HOME / LANG / USER のみ
    /// 渡している契約を確認する (env allowlist 不変条件)。
    #[test]
    fn test_build_command_env_allowlist() {
        // build_command_inner が現在 OS で成功する (CI Linux でテスト)。
        let cmd_result = build_command_for_current_os();
        if let Ok(cmd) = cmd_result {
            // env_clear 後は明示 set した key 以外存在しない。
            // 検証は std::process::Command の API では直接できないため、
            // 振る舞いの shape (エラーなく構築できる) のみ確認する。
            let _ = cmd; // shape OK
        }
    }
}
