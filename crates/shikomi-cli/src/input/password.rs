//! パスワード入力プロンプト（C-38: stdin パイプ拒否 / TTY 強制）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §セキュリティ設計 §shell history / 履歴漏洩防衛
//!   §不変条件・契約 C-38（パスワード / 24 語入力は `/dev/tty` 経由のみ、stdin パイプ拒否）
//! - docs/features/vault-encryption/basic-design/security.md §脅威 L1 §shell history
//!
//! 不変条件:
//! - stdin が非 TTY のとき `CliError::NonInteractivePassword` で fail fast。
//!   `echo pw | shikomi vault unlock` のような history / TTY scrollback / 環境変数
//!   経由のパスワード渡しを構造的に拒否する。
//! - TTY 判定後は `rpassword::prompt_password` が `/dev/tty` を優先 open する
//!   実装に依存（Unix `/dev/tty` / Windows `CONIN$`）。`is-terminal` で先に
//!   stdin TTY をガードしておくことで、rpassword の TTY フォールバック挙動による
//!   stdin 経路漏洩を二重に塞ぐ。
//! - 返値は即座に `SecretString` で包み、平文 `String` を呼出側に露出させない。

use is_terminal::IsTerminal;
use shikomi_core::SecretString;

use crate::error::CliError;

/// プロンプトを表示し、TTY から非エコーでパスワードを 1 行読み取る。
///
/// # Errors
/// - `CliError::NonInteractivePassword`: stdin が TTY でない (C-38)。
/// - `CliError::Persistence`: TTY からの読取で IO エラー。
pub fn prompt(label: &str) -> Result<SecretString, CliError> {
    if !std::io::stdin().is_terminal() {
        return Err(CliError::NonInteractivePassword);
    }
    let raw = rpassword::prompt_password(label).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<tty>"),
            source: e,
        })
    })?;
    Ok(SecretString::from_string(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `prompt` の関数シグネチャが C-38 契約を維持していることの形式確認。
    ///
    /// 実 TTY での読取は CI 環境で再現困難 (テスト runner は通常 TTY なし) のため、
    /// integration test (TC-F-I12: `echo pw | shikomi vault unlock` → exit 1) で
    /// 振る舞いを担保する。本 unit test は API shape のみを固定する。
    #[test]
    fn test_prompt_signature_returns_cli_error() {
        // 関数ポインタ取得で型一致を強制（compile-time guard）。
        let _: fn(&str) -> Result<SecretString, CliError> = prompt;
    }

    /// stdin が非 TTY (= cargo test 実行環境の一般的な状態) で呼び出すと
    /// `NonInteractivePassword` が返ることを確認する。CI runner は TTY を持たない
    /// 前提のため、この経路は CI で実観測される (TC-F-U11)。
    ///
    /// 注: 万一テスト runner が TTY 接続だった場合、本テストは早期 SKIP 相当の
    /// `Ok(_)` 経路に入る可能性があるため `is_terminal` を先にチェックする。
    #[test]
    fn test_prompt_returns_non_interactive_password_when_stdin_is_not_tty() {
        if std::io::stdin().is_terminal() {
            // 実 TTY 接続の dev box でこのテストが走った場合、契約は別経路で
            // 担保されるため早期成功扱い (false negative ではない)。
            return;
        }
        let result = prompt("test password: ");
        assert!(
            matches!(result, Err(CliError::NonInteractivePassword)),
            "expected NonInteractivePassword but got: {result:?}"
        );
    }
}
