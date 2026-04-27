//! `CliError` を `MSG-CLI-100〜109` 仕様に則って stderr 用文字列に整形する。
//!
//! Presenter は pure。出力（stderr への書き出し）は `run()` の責務。

use std::fmt::Write as _;

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
        let _ = writeln!(out, "error: {error_ja}");
    }
    let _ = writeln!(out, "hint: {hint_en}");
    if matches!(locale, Locale::JapaneseEn) {
        let _ = writeln!(out, "hint: {hint_ja}");
    }
    out
}

/// MSG-CLI-110 確定文面（`basic-design/error.md §MSG-CLI-110 確定文面`）。
fn render_daemon_not_running(path: &std::path::Path, locale: Locale) -> String {
    let path_disp = path.display();
    let mut out =
        format!("error: shikomi-daemon is not running (socket {path_disp} unreachable)\n");
    if matches!(locale, Locale::JapaneseEn) {
        let _ = writeln!(
            out,
            "error: shikomi-daemon が起動していません（ソケット {path_disp} に接続できません）"
        );
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
    // Issue #75 Bug-F-007 解消: MSG-S09(b) 拡張文言として `--vault-dir <DIR>` 案内を追加
    // (`cli-subcommands.md` §Bug-F-007 解消 §エラー文言 SSoT、ユーザ認知モデル
    // 「`<DIR>` = vault.db の所在ディレクトリ」と一致)。`SHIKOMI_VAULT_DIR` env 直接案内は
    // `--vault-dir` フラグ経路の方が明示的なため出さない (Phase 2 規定 = CLI は IPC 経由のみ、
    // vault.db 直接操作禁止 と整合)。
    out.push_str(
        "hint: or pass --vault-dir <DIR> to point at the vault.db directory whose shikomi.sock you want to use\n",
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "hint: または --vault-dir <DIR> で vault.db の所在ディレクトリを指定してください（同ディレクトリの shikomi.sock が daemon socket として使われます）\n",
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
        let _ = writeln!(
            out,
            "error: プロトコルバージョン不一致（server={server}, client={client}）"
        );
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
/// `CliError` は **同一 crate 定義**のため、`#[non_exhaustive]` 属性があっても
/// 内部からは exhaustive match が可能。新バリアント追加時にコンパイル時で
/// 網羅漏れを検出するため、wildcard fallback (`_ =>`) は使わない。
///
/// `DaemonNotRunning` / `ProtocolVersionMismatch` は `render_error` 側の専用 helper で
/// 描画される（本関数には到達しない契約）。万一到達した場合に備え固定の sentinel
/// 文言を返し、`debug_assertions` ビルドではパニックさせて開発時に検出可能化する。
fn lines_for(err: &CliError) -> (String, String, String, String) {
    let lit = |error_en: &str, error_ja: &str, hint_en: &str, hint_ja: &str| {
        (
            error_en.to_owned(),
            error_ja.to_owned(),
            hint_en.to_owned(),
            hint_ja.to_owned(),
        )
    };
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
        CliError::NonInteractiveRemove => lit(
            "refusing to delete without --yes in non-interactive mode",
            "非対話モードでは --yes なしの削除を拒否します",
            "re-run with --yes to confirm deletion",
            "削除を確認するには --yes を付けて再実行してください",
        ),
        // Sub-F (#44) Phase 5 / C-38: stdin パイプ経由のパスワード入力を構造的に拒否。
        CliError::NonInteractivePassword => lit(
            "refusing to read password from non-tty stdin",
            "非対話モードではパスワード入力を拒否します",
            "run from a terminal (TTY); piping passwords via stdin is not supported (C-38)",
            "ターミナル (TTY) から実行してください。stdin パイプ経由のパスワード入力は未対応です (C-38)",
        ),
        CliError::Persistence(pe) => render_persistence_lines(pe),
        CliError::Domain(domain) => (
            format!("internal bug: {domain}"),
            format!("内部バグ: {domain}"),
            "please report this issue to https://github.com/shikomi-dev/shikomi/issues".to_owned(),
            "https://github.com/shikomi-dev/shikomi/issues に報告してください".to_owned(),
        ),
        CliError::EncryptionUnsupported => lit(
            "this vault is encrypted; encryption is not yet supported in this CLI version",
            "この vault は暗号化されています。本バージョンの CLI は暗号化モード未対応です",
            "future \"shikomi vault decrypt\" will convert it; for now, use a plaintext vault",
            "将来の \"shikomi vault decrypt\" で変換可能になります。暫定的には平文 vault をご利用ください",
        ),
        CliError::DaemonNotRunning(_) | CliError::ProtocolVersionMismatch { .. } => {
            debug_assert!(
                false,
                "lines_for should not be reached for DaemonNotRunning / ProtocolVersionMismatch; \
                 they are dispatched by render_error to dedicated helpers"
            );
            lit(
                "internal: this variant is rendered by a dedicated helper",
                "内部: このバリアントは専用のヘルパで描画されます",
                "please report this issue to https://github.com/shikomi-dev/shikomi/issues",
                "https://github.com/shikomi-dev/shikomi/issues に報告してください",
            )
        }
        // Sub-F (#44) Phase 2: vault サブコマンド経路の MSG-S 系文言。
        // i18n 辞書 `messages.toml` 移行は Phase 6/7 で `Localizer` に集約予定。
        CliError::VaultLocked => lit(
            "vault is locked",
            "vault がロックされています",
            "run `shikomi vault unlock` to unlock the vault",
            "`shikomi vault unlock` でロックを解除してください",
        ),
        CliError::WrongPassword => lit(
            "wrong password",
            "パスワードが違います",
            "retry, or use `shikomi vault unlock --recovery` if you have the 24 recovery words",
            "再入力してください。リカバリ用 24 語があれば `shikomi vault unlock --recovery` も使えます",
        ),
        CliError::BackoffActive { wait_secs } => (
            format!("unlock blocked by backoff for {wait_secs}s"),
            format!("連続失敗のため {wait_secs} 秒待機してください"),
            "wait until the backoff window ends, then retry".to_owned(),
            "バックオフ期間の経過後に再試行してください".to_owned(),
        ),
        CliError::RecoveryRequired => lit(
            "recovery path required",
            "リカバリ経路での解除が必要です",
            "retry with `shikomi vault unlock --recovery` and the 24 recovery words",
            "リカバリ用 24 語を使い `shikomi vault unlock --recovery` で再試行してください",
        ),
        CliError::ProtocolDowngrade => lit(
            "ipc protocol downgrade detected",
            "IPC プロトコルの降格が検出されました",
            "rebuild shikomi-cli and shikomi-daemon to the same version",
            "shikomi-cli と shikomi-daemon を同一バージョンにビルドし直してください",
        ),
        CliError::Crypto { reason } => (
            format!("crypto error: {reason}"),
            format!("暗号エラー: {reason}"),
            "see the documentation for `shikomi vault {encrypt,unlock,decrypt}` failure modes"
                .to_owned(),
            "`shikomi vault {encrypt,unlock,decrypt}` の失敗事由をドキュメントで確認してください"
                .to_owned(),
        ),
        CliError::UnexpectedIpcResponse { request_kind } => (
            format!("unexpected ipc response for {request_kind}"),
            format!("{request_kind} に対する想定外の IPC 応答"),
            "rebuild shikomi-cli and shikomi-daemon to the same version".to_owned(),
            "shikomi-cli と shikomi-daemon を同一バージョンにビルドし直してください".to_owned(),
        ),
        // Sub-F (#44) Phase 3 / REQ-S16 Fail-Secure: 保護モード判定不能。
        // CLI は exit 3 で fail-fast し、レコード一覧を一切表示しない。
        CliError::ProtectionModeUnknown => lit(
            "vault protection mode is unknown",
            "vault の保護モードが不明です",
            "the vault header may be corrupted; restore from backup or contact support",
            "vault ヘッダが破損している可能性があります。バックアップから復元するか、サポートに連絡してください",
        ),
        // Issue #75 Bug-F-001 §排他違反検知 (defensive): MSG-S21 文言固定、exit 64 (`EX_USAGE`)。
        // i18n 辞書 (`messages.toml`) 移行は Phase 7 で `Localizer` に集約予定。本 PR では
        // 既存の他 MSG-S* と同パターンで `lit()` 経由インライン化する。
        CliError::IncompatibleAuthFlags { hint } => lit(
            &format!("conflicting authentication flags ({hint})"),
            &format!("複数の認証経路が同時に指定されています（{hint}）"),
            "`--recovery` and password input cannot be combined; choose one",
            "`--recovery` と password 入力は併用できません。どちらか一方を指定してください",
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
