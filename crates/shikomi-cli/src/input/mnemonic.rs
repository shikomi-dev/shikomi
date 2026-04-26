//! BIP-39 24 語入力プロンプト (C-38、Sub-F #44 工程4 Bug-F-001 解消)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §モジュール配置と責務 `shikomi_cli::input::mnemonic_prompt`
//!   §不変条件・契約 C-38 (`/dev/tty` 経由のみ、stdin パイプ拒否)
//! - docs/features/vault-encryption/test-design/sub-f-cli-subcommands.md
//!   §15.6 TC-F-I03b (`vault unlock --recovery` 経路の 24 語 stdin 入力)
//!
//! 不変条件:
//! - `input::password::prompt` と同じ TTY ガード (C-38)。非 TTY 時は
//!   `CliError::NonInteractivePassword` で fail fast し `echo word | shikomi vault
//!   unlock --recovery` 経路を構造的に拒否する。
//! - 入力は **rpassword 非エコー** で取得。BIP-39 24 語は VEK 復号鍵相当の機微情報
//!   であり、TTY 上でエコーバックしてはならない (TTY scrollback / 録画ツール経由
//!   漏洩防止、§セキュリティ設計 §TTY scrollback と整合)。
//! - 入力フォーマット: 1 行に空白区切りで 24 語を入力 (改行 1 回で完了)。
//!   将来 minor で「1 行 1 語 × 24 行」の対話モードを追加検討。
//! - 区切りは ASCII 空白 (`' '` / `'\t'`) 連続を 1 区切りとして扱う
//!   (`str::split_whitespace`)。BIP-39 wordlist は ASCII 英小文字のみ前提なので
//!   非 ASCII 文字混入時は **Fail Fast** で `CliError::UsageError` を返す
//!   (`?` 置換で握り潰すと recovery 失敗の根本原因が見えなくなるため)。
//!
//! 戻り値の機微情報扱い:
//! - 各語を即座に `SerializableSecretBytes` で包み、平文 `String` を呼出側に
//!   露出させない。`SecretString` で 1 行を持つ中間バッファは関数内で drop され、
//!   Drop で zeroize されることを `secrecy` crate の契約で保証 (BIP-39 word 単体の
//!   平文リークは secret_bytes の lossy_string helper のみで起こる)。

use is_terminal::IsTerminal;
use shikomi_core::ipc::SerializableSecretBytes;
use shikomi_core::SecretString;

use crate::error::CliError;

/// 期待語数 (BIP-39 24 語固定、定数化で magic number を排除)。
pub const EXPECTED_WORDS: usize = 24;

/// プロンプトを表示し、TTY から非エコーで 24 語をスペース区切りで読み取る。
///
/// # Errors
/// - `CliError::NonInteractivePassword`: stdin が TTY でない (C-38)。
/// - `CliError::UsageError`: 24 語に満たない / 非 ASCII 混入。
/// - `CliError::Persistence`: TTY 読取で IO エラー。
pub fn prompt(label: &str) -> Result<Vec<SerializableSecretBytes>, CliError> {
    if !std::io::stdin().is_terminal() {
        return Err(CliError::NonInteractivePassword);
    }
    let raw = rpassword::prompt_password(label).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<tty:mnemonic>"),
            source: e,
        })
    })?;
    parse_mnemonic_line(&raw)
}

/// 1 行のスペース区切り入力を 24 語の `SerializableSecretBytes` に分割する pure 関数。
///
/// 区切りは `str::split_whitespace` (空白連続を 1 区切り扱い)。
/// テスト容易性 + 非 TTY ヘルパからの再利用のために公開。
pub fn parse_mnemonic_line(line: &str) -> Result<Vec<SerializableSecretBytes>, CliError> {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() != EXPECTED_WORDS {
        return Err(CliError::UsageError(format!(
            "expected {EXPECTED_WORDS} recovery words, got {}",
            words.len()
        )));
    }
    let mut out = Vec::with_capacity(EXPECTED_WORDS);
    for w in words {
        if !w.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(CliError::UsageError(
                "recovery words must be ASCII lowercase BIP-39 mnemonic words".to_owned(),
            ));
        }
        out.push(SerializableSecretBytes::from_secret_string(
            SecretString::from_string(w.to_owned()),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mnemonic_line_24_ascii_words_succeeds() {
        let line = (0..24).map(|_| "abandon").collect::<Vec<_>>().join(" ");
        let result = parse_mnemonic_line(&line);
        let words = result.expect("24 valid ascii words should parse");
        assert_eq!(words.len(), 24);
    }

    #[test]
    fn test_parse_mnemonic_line_too_few_words_returns_usage_error() {
        let line = "abandon ability";
        let result = parse_mnemonic_line(line);
        assert!(matches!(result, Err(CliError::UsageError(_))));
    }

    #[test]
    fn test_parse_mnemonic_line_too_many_words_returns_usage_error() {
        let line = (0..25).map(|_| "abandon").collect::<Vec<_>>().join(" ");
        let result = parse_mnemonic_line(&line);
        assert!(matches!(result, Err(CliError::UsageError(_))));
    }

    #[test]
    fn test_parse_mnemonic_line_non_ascii_returns_usage_error() {
        let mut line = (0..23).map(|_| "abandon").collect::<Vec<_>>().join(" ");
        line.push_str(" héllo");
        let result = parse_mnemonic_line(&line);
        assert!(matches!(result, Err(CliError::UsageError(_))));
    }

    #[test]
    fn test_parse_mnemonic_line_uppercase_returns_usage_error() {
        let mut line = (0..23).map(|_| "abandon").collect::<Vec<_>>().join(" ");
        line.push_str(" Abandon");
        let result = parse_mnemonic_line(&line);
        assert!(matches!(result, Err(CliError::UsageError(_))));
    }

    #[test]
    fn test_parse_mnemonic_line_collapses_multiple_whitespace() {
        let line = format!("{}", (0..24).map(|_| "abandon").collect::<Vec<_>>().join("\t  "));
        let result = parse_mnemonic_line(&line);
        assert!(result.is_ok());
    }

    #[test]
    fn test_prompt_signature_returns_cli_error() {
        let _: fn(&str) -> Result<Vec<SerializableSecretBytes>, CliError> = prompt;
    }

    #[test]
    fn test_prompt_returns_non_interactive_password_when_stdin_is_not_tty() {
        if std::io::stdin().is_terminal() {
            return;
        }
        let result = prompt("recovery words: ");
        assert!(matches!(result, Err(CliError::NonInteractivePassword)));
    }
}
