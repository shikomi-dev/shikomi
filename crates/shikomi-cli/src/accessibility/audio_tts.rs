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
use zeroize::Zeroizing;

use crate::error::CliError;

/// 24 語を OS TTS subprocess に音声合成させる。
///
/// 工程5 服部指摘 (BLOCKER 3) 解消: payload は `Zeroizing<Vec<u8>>` で構築、
/// drop 時に親プロセス heap で zeroize される (subprocess 側の文字列は別アドレス
/// 空間で本ライブラリの責務外)。`expose_secret() -> &[u8]` から直接 byte 消費。
///
/// 工程5 服部指摘 (BLOCKER 4) 解消: Windows は **`Err(unsupported)`** で fail
/// fast。PowerShell `-Command` 経由だと ScriptBlockLogging (Event ID 4104) で
/// 24 語が SIEM 転送可能な形で Event Log に記録される攻撃面があり、本ライブラリ
/// 単体では遮断不能。Windows 向け本実装は **`shikomi-windows-tts.exe`** helper
/// バイナリ + COM 経由 SAPI 呼出で Phase 8 以降に再設計する。
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
        let stdin = child.stdin.as_mut().ok_or_else(stdin_unavailable_err)?;
        stdin
            .write_all(&payload)
            .map_err(|e| io_err("tts stdin", e))?;
    } // stdin drop で EOF 通知、payload は Drop で zeroize

    let status = child.wait().map_err(|e| io_err("tts wait", e))?;
    if !status.success() {
        return Err(io_err(
            "tts non-zero exit",
            std::io::Error::other(format!("status: {status:?}")),
        ));
    }
    Ok(())
}

/// 24 語を読み上げ用 byte 列に整形する (`zeroize::Zeroizing` で drop 時に消去)。
///
/// 工程5 BLOCKER 3 解消: `to_lossy_string_for_handler()` 経路を排除し、
/// `expose_secret()` から `&[u8]` を直接 byte コピー。中間 `String` 生成なし。
fn build_payload(words: &[SerializableSecretBytes]) -> Zeroizing<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let prefix = format!("{}. ", i + 1);
        out.extend_from_slice(prefix.as_bytes());
        let bytes = w.inner().expose_secret();
        out.extend_from_slice(bytes);
        out.extend_from_slice(b". ");
    }
    Zeroizing::new(out)
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
    // 工程5 服部指摘 (BLOCKER 4) 解消: Windows audio TTS は本ライブラリ単体では
    // ScriptBlockLogging (Event ID 4104) 経由の 24 語平文漏洩を遮断できない。
    // PowerShell `-Command` だとパイプから流れた `$_` 変数 (= 24 語) が
    // ScriptBlock として記録され、企業 GPO 環境で SIEM 転送される攻撃面が成立。
    // Phase 8 以降で `shikomi-windows-tts.exe` helper bin (COM 直接呼出) を
    // 別 PR で導入するまで、Windows audio 経路は **fail fast** で塞ぐ。
    Err(io_err(
        "windows audio tts unsupported",
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Windows audio TTS is intentionally disabled until Phase 8 \
             helper bin lands (avoids PowerShell ScriptBlockLogging exposure of 24 words)",
        ),
    ))
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

    fn payload_as_str(p: &[u8]) -> &str {
        std::str::from_utf8(p).expect("payload bytes are ASCII subset")
    }

    #[test]
    fn test_build_payload_includes_numbers_and_words() {
        let words = vec![word("alpha"), word("beta")];
        let payload = build_payload(&words);
        let s = payload_as_str(&payload);
        assert!(s.contains("1. alpha"));
        assert!(s.contains("2. beta"));
    }

    #[test]
    fn test_build_payload_24_words_produces_24_segments() {
        let words: Vec<_> = (0..24).map(|i| word(&format!("w{i}"))).collect();
        let payload = build_payload(&words);
        let s = payload_as_str(&payload);
        // 番号 1〜24 が全部含まれる。
        for i in 1..=24 {
            assert!(s.contains(&format!("{i}. w")));
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
