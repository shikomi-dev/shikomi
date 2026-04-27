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

    /// TC-F-U13 (C-38): `input::password::prompt` の **C-38 stdin パイプ拒否経路** を
    /// 機械検証する。設計書 §15.5 #13 の 3 パターン (a)/(b)/(c) のうち、unit テスト
    /// で決定的に検証可能な (b) 非 TTY 経路に集中する。
    ///
    /// (a) PTY 経由 (TTY → `Ok(SecretString)`) は **`expectrl` PTY 設定が CI runner
    ///     に依存**するため結合 TC-F-I12 で実機検証 (issue-76-verification.md §15.17.3.3
    ///     自己批判: PTY 利用 TC は `#[ignore]` フォールバック articulate)。
    /// (b) **本 unit test のスコープ** = stdin 非 TTY (CI runner デフォルト) で
    ///     `Err(CliError::NonInteractivePassword)` を返す。
    /// (c) `/dev/tty` open 失敗は OS 側の挙動依存で unit テストでは決定的に再現でき
    ///     ない (b) で C-38 経路の入口は守れているため OS 別 manual smoke で担保。
    ///
    /// **mnemonic 側 TC-F-U13 (b)** は同 Issue で `input/mnemonic.rs::tests` 配下に
    /// 同名関数 `tc_f_u13_mnemonic_prompt_returns_non_interactive_password_when_stdin_not_tty`
    /// として配置 (両 prompt の C-38 ガードを並列に保証する SSoT)。
    ///
    /// 配置先: `crates/shikomi-cli/src/input/password.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置と一致)。
    #[test]
    fn tc_f_u13_password_prompt_returns_non_interactive_password_when_stdin_not_tty() {
        // CI runner は通常 stdin 非 TTY。本 TC は (b) 非 TTY 経路の決定的検証。
        // 万一 dev box 等で TTY が接続されている場合、本 TC は契約違反ではないので
        // skip 相当でフォローする (`tests/e2e_*` で対応経路を別途検証)。
        if std::io::stdin().is_terminal() {
            // TTY 接続環境では C-38 経路が unit で決定的に再現できないため早期 return。
            // CI runner では非 TTY が前提のため、ここに到達しない。
            return;
        }
        let result = prompt("test password: ");
        assert!(
            matches!(result, Err(CliError::NonInteractivePassword)),
            "C-38: stdin 非 TTY 時は NonInteractivePassword で fail-fast すべき, got: {result:?}"
        );

        // (a) シグネチャ型一致: `&str → Result<SecretString, CliError>` を compile-time に
        //     固定。Phase 5 で C-38 経路を変更しても呼出側を壊さない構造を強制。
        let _: fn(&str) -> Result<SecretString, CliError> = prompt;
    }
}
