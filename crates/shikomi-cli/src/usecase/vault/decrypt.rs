//! `vault decrypt` usecase（Sub-F #44 Phase 2、F-F2）。
//!
//! 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
//! §処理フロー詳細 F-F2
//!
//! Phase 2 スコープ: master password + 確認入力 `DECRYPT` 取得 → IPC `Decrypt`
//! 1 往復。`subtle::ConstantTimeEq` 比較 + paste 抑制 (C-34) は Phase 5 で
//! `input::decrypt_confirmation::prompt` に集約する。Phase 2 は単純文字列比較で wire。

use std::io::Write;

use super::read_master_password;
use crate::error::CliError;
use crate::io::ipc_vault_repository::IpcVaultRepository;
use crate::io::terminal;
use crate::presenter::{success, Locale};

/// 確認入力で要求される確定文字列（Sub-D Rev2 凍結契約 + C-34）。
const CONFIRMATION_LITERAL: &str = "DECRYPT";

/// `vault decrypt` を実行する。
///
/// # Errors
/// 確認入力不一致 / IPC 失敗 / daemon 側エラー / TTY 入力失敗時に `CliError`。
pub fn execute(repo: &IpcVaultRepository, locale: Locale, quiet: bool) -> Result<(), CliError> {
    let master_password = read_master_password("master password: ")?;
    let confirmation = read_confirmation_literal(locale)?;
    let confirmed = is_decrypt_confirmation_literal(&confirmation);
    if !confirmed {
        return Err(CliError::UsageError(
            "decrypt confirmation mismatch (expected literal `DECRYPT`)".to_owned(),
        ));
    }
    repo.decrypt(master_password, confirmed)?;
    if !quiet {
        let rendered = success::render_decrypted(locale);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(rendered.as_bytes());
    }
    Ok(())
}

/// `DECRYPT` 確認入力を 1 行読み込む（Phase 2 単純実装、Phase 5 で paste 抑制 + ConstantTimeEq）。
fn read_confirmation_literal(_locale: Locale) -> Result<String, CliError> {
    let prompt = "type `DECRYPT` to confirm: ";
    terminal::read_line(prompt).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<tty>"),
            source: e,
        })
    })
}

/// 確認文字列が `CONFIRMATION_LITERAL` (= `"DECRYPT"`) と完全一致するか判定する pure 関数。
///
/// `execute` 本体の判定経路と同じ等値比較を**単独に切り出した**テスト容易性向上ヘルパ。
/// `subtle::ConstantTimeEq` への移行は C-34 paste 抑制と同じ Phase 5 タスクで行う計画
/// (`cli-subcommands.md` §パスワード入力 §C-34)。本 helper は判定ロジックそのものを
/// 単独関数として SSoT 化し、Phase 5 移行時に呼出側を変えずに内部だけ ConstantTimeEq
/// に差し替えられる構造を作る (Open/Closed)。
#[must_use]
pub(crate) fn is_decrypt_confirmation_literal(input: &str) -> bool {
    input == CONFIRMATION_LITERAL
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-F-U03 (C-34): `vault decrypt` 確認文字列の **大文字 `DECRYPT` 完全一致**契約の
    /// 機械検証。
    ///
    /// 設計書 §15.5 #3 は `decrypt_confirmation::prompt` で paste 模擬の時刻差検証
    /// (`< 30ms = Err(PasteSuspected)` / `>= 30ms = Ok`) を要求するが、現状実装は
    /// `usecase::vault::decrypt::read_confirmation_literal` が **Phase 2 単純文字列比較**
    /// で wire されており、`subtle::ConstantTimeEq` + paste 抑制への昇格は **Phase 5 タスク**
    /// として `input::decrypt_confirmation::prompt` モジュール導入時に集約される計画
    /// (本ファイル冒頭 doc コメント「Phase 5 で paste 抑制 + ConstantTimeEq」)。
    ///
    /// **§15.17.2 §A 実装事実への追従**: `input::decrypt_confirmation` モジュールは未導入のため、
    /// 本 TC は現実装の判定経路 (`is_decrypt_confirmation_literal` + `CONFIRMATION_LITERAL`)
    /// を SSoT として:
    /// (a) `"DECRYPT"` のみが `true` を返す、
    /// (b) `"decrypt"` (lower-case) / `"DECRYPT "` (trailing space) / `""` (空) は
    ///     全て `false` を返す、
    /// ことを機械検証する。Phase 5 移行時に paste 抑制 4 段時刻差検証へ差し替える Boy
    /// Scout が必要 (`is_decrypt_confirmation_literal` を `decrypt_confirmation::prompt`
    /// 内部呼出 + `Instant` fake provider 経由 4 段検証に拡張)。
    ///
    /// 配置先: `crates/shikomi-cli/src/usecase/vault/decrypt.rs::tests` (issue-76-verification.md
    /// §15.17.1 推奨配置 `input/decrypt_confirmation.rs::tests` を未導入実装事実に追従)。
    #[test]
    fn tc_f_u03_decrypt_confirmation_literal_compare_only_accepts_uppercase_decrypt() {
        // (a) 正規入力。
        assert!(
            is_decrypt_confirmation_literal("DECRYPT"),
            "DECRYPT は確認文字列として受理されるべき"
        );

        // (b) 異常系列挙: case mismatch / 余分な whitespace / 空 / 似て非なる文字列。
        let rejects = [
            "decrypt",          // 小文字
            "Decrypt",          // 先頭のみ大文字
            "DECRYPT ",         // 末尾空白
            " DECRYPT",         // 先頭空白
            "DECRYPTING",       // 部分一致
            "",                 // 空
            "decrypt me",       // 多語
            "DECRYPT\n",        // 改行混入
            "decrypt\tDECRYPT", // tab 混入の連結
        ];
        for r in rejects {
            assert!(
                !is_decrypt_confirmation_literal(r),
                "{r:?} は確認文字列として拒否されるべき (Phase 5 で ConstantTimeEq + paste 抑制に昇格予定、Phase 2 段階での SSoT)"
            );
        }
    }
}
