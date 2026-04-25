//! `CliError` を `MSG-CLI-100〜109` 仕様に則って stderr 用文字列に整形する。
//!
//! Presenter は pure。出力（stderr への書き出し）は `run()` の責務。

use shikomi_infra::persistence::PersistenceError;

use crate::error::CliError;

use super::Locale;

/// `CliError` を 2 行（English）または 4 行（JapaneseEn）形式で整形する。
///
/// 例外: MSG-CLI-110（DaemonNotRunning）は 3 OS 並記の hint で複数行 / MSG-CLI-111
/// （ProtocolVersionMismatch）は 1 hint 行で構成し、それぞれ専用 helper を呼ぶ。
#[must_use]
pub fn render_error(err: &CliError, locale: Locale) -> String {
    match err {
        CliError::DaemonNotRunning(path) => render_daemon_not_running(path, locale),
        CliError::ProtocolVersionMismatch { server, client } => {
            render_protocol_version_mismatch(server, client, locale)
        }
        _ => render_default(err, locale),
    }
}

fn render_default(err: &CliError, locale: Locale) -> String {
    // `lines_for` の戻り値は `(error 英, error 日, hint 英, hint 日)` 順。
    // 変数束縛もこの順に揃える（以前は `(error_en, hint_en, error_ja, hint_ja)` と
    // 入れ替えてしまい、LANG=C 環境の hint 行に日本語が漏れていた — BUG-002）。
    let (error_en, error_ja, hint_en, hint_ja) = lines_for(err);
    let mut out = format!("error: {error_en}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("error: {error_ja}\n"));
    }
    out.push_str(&format!("hint: {hint_en}\n"));
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("hint: {hint_ja}\n"));
    }
    out
}

/// MSG-CLI-110 確定文面（`basic-design/error.md §MSG-CLI-110 確定文面`）。
fn render_daemon_not_running(path: &std::path::Path, locale: Locale) -> String {
    let path_disp = path.display();
    let mut out =
        format!("error: shikomi-daemon is not running (socket {path_disp} unreachable)\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!(
            "error: shikomi-daemon が起動していません（ソケット {path_disp} に接続できません）\n"
        ));
    }
    out.push_str("hint: start the daemon in a separate terminal by running one of:\n");
    out.push_str("hint:   Linux/macOS:            'shikomi-daemon &'\n");
    out.push_str("hint:   Linux (systemd user):   'systemctl --user start shikomi-daemon'\n");
    out.push_str(
        "hint:   macOS (launchd user):   'launchctl kickstart gui/$(id -u)/dev.shikomi.daemon'\n",
    );
    out.push_str("hint:   Windows (PowerShell):   'Start-Process -NoNewWindow shikomi-daemon'\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("hint: 別のターミナルで以下のいずれかで daemon を起動してください:\n");
        out.push_str("hint:   Linux/macOS:            'shikomi-daemon &'\n");
        out.push_str("hint:   Linux (systemd user):   'systemctl --user start shikomi-daemon'\n");
        out.push_str(
            "hint:   macOS (launchd user):   'launchctl kickstart gui/$(id -u)/dev.shikomi.daemon'\n",
        );
        out.push_str(
            "hint:   Windows (PowerShell):   'Start-Process -NoNewWindow shikomi-daemon'\n",
        );
    }
    out
}

/// MSG-CLI-111 確定文面（`basic-design/error.md §MSG-CLI-111 確定文面`）。
fn render_protocol_version_mismatch(
    server: &shikomi_core::ipc::IpcProtocolVersion,
    client: &shikomi_core::ipc::IpcProtocolVersion,
    locale: Locale,
) -> String {
    let mut out = format!("error: protocol version mismatch (server={server}, client={client})\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!(
            "error: プロトコルバージョン不一致（server={server}, client={client}）\n"
        ));
    }
    out.push_str("hint: rebuild shikomi-cli and shikomi-daemon to the same version\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "hint: shikomi-cli と shikomi-daemon を同一バージョンにビルドし直してください\n",
        );
    }
    out
}

/// 4 段（error 英 / error 日 / hint 英 / hint 日）を返す。
///
/// `DaemonNotRunning` / `ProtocolVersionMismatch` は `render_error` 側の専用 helper で
/// 直接処理するため、ここでは到達しない（防御的に default 文言を返す）。
fn lines_for(err: &CliError) -> (String, String, String, String) {
    match err {
        CliError::UsageError(msg) => (
            msg.clone(),
            usage_error_ja(msg),
            "choose one, or see --help".to_owned(),
            "どちらか一方を指定するか --help を参照してください".to_owned(),
        ),
        CliError::InvalidLabel(domain) => (
            format!("invalid label: {domain}"),
            format!("不正なラベル: {domain}"),
            "labels must be non-empty and at most 255 graphemes; control chars except \\t\\n\\r are not allowed"
                .to_owned(),
            "ラベルは 1 文字以上 255 grapheme 以下で、\\t\\n\\r 以外の制御文字は禁止です".to_owned(),
        ),
        CliError::InvalidId(domain) => (
            format!("invalid record id: {domain}"),
            format!("不正なレコード ID: {domain}"),
            "use the uuid shown by \"shikomi list\"".to_owned(),
            "\"shikomi list\" で表示された UUID を指定してください".to_owned(),
        ),
        CliError::RecordNotFound(id) => (
            format!("record not found: {id}"),
            format!("レコードが見つかりません: {id}"),
            "check with \"shikomi list\"".to_owned(),
            "\"shikomi list\" で確認してください".to_owned(),
        ),
        CliError::VaultNotInitialized(path) => (
            format!("vault not initialized at {}", path.display()),
            format!("vault が初期化されていません: {}", path.display()),
            "run \"shikomi add\" to create a plaintext vault".to_owned(),
            "\"shikomi add\" で平文 vault を初期化できます".to_owned(),
        ),
        CliError::NonInteractiveRemove => (
            "refusing to delete without --yes in non-interactive mode".to_owned(),
            "非対話モードでは --yes なしの削除を拒否します".to_owned(),
            "re-run with --yes to confirm deletion".to_owned(),
            "削除を確認するには --yes を付けて再実行してください".to_owned(),
        ),
        CliError::Persistence(pe) => render_persistence_lines(pe),
        CliError::Domain(domain) => (
            format!("internal bug: {domain}"),
            format!("内部バグ: {domain}"),
            "please report this issue to https://github.com/shikomi-dev/shikomi/issues".to_owned(),
            "https://github.com/shikomi-dev/shikomi/issues に報告してください".to_owned(),
        ),
        CliError::EncryptionUnsupported => (
            "this vault is encrypted; encryption is not yet supported in this CLI version"
                .to_owned(),
            "この vault は暗号化されています。本バージョンの CLI は暗号化モード未対応です"
                .to_owned(),
            "future \"shikomi vault decrypt\" will convert it; for now, use a plaintext vault"
                .to_owned(),
            "将来の \"shikomi vault decrypt\" で変換可能になります。暫定的には平文 vault をご利用ください"
                .to_owned(),
        ),
        // `DaemonNotRunning` / `ProtocolVersionMismatch` は `render_error` 側で先に分岐済み。
        // `#[non_exhaustive]` 防御的 wildcard も含めて到達しないが、フォールバック文言を返す。
        _ => (
            "internal: unhandled error variant".to_owned(),
            "内部: 未処理のエラーバリアント".to_owned(),
            "please report this issue to https://github.com/shikomi-dev/shikomi/issues".to_owned(),
            "https://github.com/shikomi-dev/shikomi/issues に報告してください".to_owned(),
        ),
    }
}

/// usage error の日本語文は機械訳ではなく代表的な英語メッセージをカタログ引きする。
/// カタログに無い場合は英文をそのまま返す（secret を含まない前提）。
fn usage_error_ja(msg: &str) -> String {
    match msg {
        "--value and --stdin cannot be used together" => {
            "--value と --stdin は同時に使えません".to_owned()
        }
        "either --value or --stdin is required" => {
            "--value または --stdin のどちらかが必要です".to_owned()
        }
        "at least one of --label/--value/--stdin is required" => {
            "--label / --value / --stdin のいずれかを指定してください".to_owned()
        }
        other => other.to_owned(),
    }
}

fn render_persistence_lines(pe: &PersistenceError) -> (String, String, String, String) {
    match pe {
        PersistenceError::Corrupted { .. } => (
            format!("vault is corrupted: {pe}"),
            format!("vault が破損しています: {pe}"),
            "restore from backup or start a new vault".to_owned(),
            "バックアップから復元するか、新規 vault を作成してください".to_owned(),
        ),
        _ => (
            format!("failed to access vault: {pe}"),
            format!("vault へのアクセスに失敗しました: {pe}"),
            "check permissions and re-run".to_owned(),
            "パーミッションを確認して再実行してください".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::error::InvalidRecordLabelReason;
    use shikomi_core::DomainError;

    #[test]
    fn test_render_error_english_has_two_lines_for_usage_error() {
        let err = CliError::UsageError("--value and --stdin cannot be used together".to_owned());
        let out = render_error(&err, Locale::English);
        let count = out.matches('\n').count();
        assert_eq!(
            count, 2,
            "English render_error should be 2 lines, got: {out:?}"
        );
    }

    #[test]
    fn test_render_error_japanese_en_has_four_lines() {
        let err = CliError::UsageError("--value and --stdin cannot be used together".to_owned());
        let out = render_error(&err, Locale::JapaneseEn);
        let count = out.matches('\n').count();
        assert_eq!(
            count, 4,
            "JapaneseEn render_error should be 4 lines, got: {out:?}"
        );
    }

    #[test]
    fn test_render_error_invalid_label_contains_label_keyword() {
        let err = CliError::InvalidLabel(DomainError::InvalidRecordLabel(
            InvalidRecordLabelReason::Empty,
        ));
        let out = render_error(&err, Locale::English);
        assert!(out.contains("invalid label"));
    }

    #[test]
    fn test_render_error_encryption_unsupported_mentions_encryption() {
        let out = render_error(&CliError::EncryptionUnsupported, Locale::English);
        assert!(out.contains("encrypted"));
    }

    #[test]
    fn test_render_error_non_interactive_remove_mentions_yes() {
        let out = render_error(&CliError::NonInteractiveRemove, Locale::English);
        assert!(out.contains("--yes"));
    }

    /// BUG-002 回帰: English モードの出力には日本語文字を一切含まないこと。
    /// 以前は `lines_for` の戻り値と受取側変数の順序がずれており hint 行に
    /// 日本語カタログが漏出していた。
    #[test]
    fn test_render_error_english_mode_never_contains_japanese() {
        let err = CliError::InvalidLabel(DomainError::InvalidRecordLabel(
            InvalidRecordLabelReason::Empty,
        ));
        let out = render_error(&err, Locale::English);
        assert!(
            out.is_ascii() || out.chars().all(|c| c.is_ascii() || c == '…'),
            "English render_error should be ASCII-only, got: {out:?}"
        );
    }
}
