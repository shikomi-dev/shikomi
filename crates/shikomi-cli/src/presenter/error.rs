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
/// Issue #141: MSG-CLI-144（ImportValidationFailed::RedactedPayload）も専用 helper を呼ぶ。
#[must_use]
pub fn render_error(err: &CliError, locale: Locale) -> String {
    match err {
        CliError::DaemonNotRunning(path) => render_daemon_not_running(path, locale),
        CliError::ProtocolVersionMismatch { server, client } => {
            render_protocol_version_mismatch(server, client, locale)
        }
        // Issue #141: MSG-CLI-144 — RedactedPayload は専用 helper で描画する。
        // UnknownFormatVersion / DuplicateIdInFile は lines_for の fallback（MSG-CLI-143）で処理する。
        CliError::ImportValidationFailed(
            shikomi_core::portability::ImportValidationError::RedactedPayload { id },
        ) => render_import_validation_redacted(id, locale),
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
    // Issue #134 (MSG-CLI-110): Sub-B 完了後の autostart hint 追加
    // (basic-design.md §Sub-B完了後に更新するメッセージ)
    out.push_str("hint: or enable autostart: shikomi daemon install\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str("hint: または自動起動を有効にする場合: shikomi daemon install\n");
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
        // Issue #89: ホットキー操作エラー文言（PG-①② 対応）。exit 1（UserError）。
        CliError::HotkeyConflict { reason } => (
            format!("hotkey conflict: {reason}"),
            format!("ホットキーが競合しています: {reason}"),
            // PG-②: ユーザへの行動案内を明示する
            "specify a different hotkey combo".to_owned(),
            "別のホットキーコンボを指定してください".to_owned(),
        ),
        CliError::HotkeyParseError { reason } => (
            format!("invalid hotkey: {reason}"),
            format!("不正なホットキー: {reason}"),
            "use format like ctrl+alt+1 or shift+f1".to_owned(),
            "ctrl+alt+1 や shift+f1 のような形式で指定してください".to_owned(),
        ),
        CliError::GuiLaunchFailed(msg) => (
            format!("failed to launch GUI: {msg}"),
            format!("GUI の起動に失敗しました: {msg}"),
            "is shikomi-gui installed? try reinstalling shikomi".to_owned(),
            "shikomi-gui がインストールされているか確認してください".to_owned(),
        ),
        // Issue #141: data-portability export / import エラー文言（MSG-CLI-140〜143）
        CliError::ExportImportVaultLocked => lit(
            "vault is locked; unlock the vault before running export or import",
            "vault がロックされています。export / import の前に vault のロックを解除してください",
            "run `shikomi vault unlock` first",
            "先に `shikomi vault unlock` を実行してください",
        ),
        CliError::ExportOutputFileExists { path } => (
            format!("export output file already exists: {}", path.display()),
            format!("export 先ファイルが既に存在します: {}", path.display()),
            "pass --force to overwrite, or choose a different --output path".to_owned(),
            "上書きする場合は --force を指定するか、別の --output パスを指定してください".to_owned(),
        ),
        CliError::ImportConflict { ids } => {
            let display = format_conflict_ids(ids);
            (
                format!("import conflict: {} record(s) already exist in vault (ids: {display})", ids.len()),
                format!("import 衝突: {} 件のレコードが vault に既に存在します（ID: {display}）", ids.len()),
                "use --on-conflict skip to skip conflicting records, or --on-conflict overwrite to replace them".to_owned(),
                "--on-conflict skip で衝突レコードをスキップするか、--on-conflict overwrite で上書きしてください".to_owned(),
            )
        }
        CliError::ImportDeserializationFailed { reason } => (
            format!("failed to parse import file: {reason}"),
            format!("import ファイルの解析に失敗しました: {reason}"),
            "verify the file is a valid shikomi export (format_version must be 1)".to_owned(),
            "ファイルが有効な shikomi export ファイルであることを確認してください（format_version は 1 である必要があります）".to_owned(),
        ),
        // Issue #146: MSG-CLI-146（ImportVaultBusy）— SQLITE_BUSY 超過後の daemon ロック競合
        // 文面の SSoT: docs/features/data-portability/cli/detailed-design/presenter.md §ImportVaultBusy
        CliError::ImportVaultBusy => lit(
            "vault is in use by shikomi-daemon; import aborted after 2 seconds",
            "vault が shikomi-daemon に使用されています。2 秒待機後に import を中断しました",
            "stop shikomi-daemon, then retry (to disable autostart: shikomi daemon uninstall)",
            "shikomi-daemon を停止してから再実行してください（自動起動の無効化: shikomi daemon uninstall）",
        ),
        // `RedactedPayload` は `render_error` 側の専用 helper で処理済のため、
        // `lines_for` には到達しない契約。万一到達した場合のコンパイル時網羅性のため記述する。
        CliError::ImportValidationFailed(err) => {
            debug_assert!(
                false,
                "ImportValidationFailed(RedactedPayload) should be rendered by render_import_validation_redacted; \
                 other variants fall through to lines_for"
            );
            (
                format!("failed to parse import file: {err}"),
                format!("import ファイルの解析に失敗しました: {err}"),
                "verify the file is a valid shikomi export (format_version must be 1)".to_owned(),
                "ファイルが有効な shikomi export ファイルであることを確認してください（format_version は 1 である必要があります）".to_owned(),
            )
        }
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

// -------------------------------------------------------------------
// Issue #141: data-portability private helpers
// -------------------------------------------------------------------

/// 衝突 ID 一覧を最大 4 件 + 省略で整形する。
///
/// 4 件以下: `ids.join(", ")`
/// 5 件以上: `"id1, id2, id3, id4, ... (N more)"` 形式（terminal 溢れ防止）
fn format_conflict_ids(ids: &[String]) -> String {
    if ids.len() <= 4 {
        ids.join(", ")
    } else {
        let head = ids[..4].join(", ");
        format!("{head}, ... ({} more)", ids.len() - 4)
    }
}

/// MSG-CLI-144: `ImportValidationFailed(RedactedPayload)` の専用 render helper。
///
/// `render_error` から dispatch される。リダクトペイロードを含むレコードの
/// import 試行に対し、`--export-secrets` 付き再 export を案内する。
fn render_import_validation_redacted(id: &str, locale: Locale) -> String {
    let mut out = format!("error: cannot import record {id}: payload is redacted\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!(
            "error: レコード {id} を import できません: ペイロードがリダクトされています\n"
        ));
    }
    out.push_str(
        "hint: re-export the source vault with --export-secrets, then import the new file\n",
    );
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(
            "hint: ソース vault を --export-secrets 付きで再 export し、新しいファイルを import してください\n",
        );
    }
    out
}

// -------------------------------------------------------------------
// Sub-B (#127): autostart エラーメッセージ（MSG-CLI-120 / MSG-CLI-121）
//
// 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/presenter.md
// §MSG-CLI-120 / MSG-CLI-121 追加
// -------------------------------------------------------------------

/// MSG-CLI-120: `shikomi daemon install` 失敗文言。
#[must_use]
pub fn render_autostart_install_error(
    err: &crate::autostart::AutostartError,
    locale: Locale,
) -> String {
    let mut out = format!("error: failed to enable autostart: {err}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("エラー: 自動起動の有効化に失敗しました: {err}\n"));
    }
    out
}

/// MSG-CLI-121: `shikomi daemon uninstall` 失敗文言。
#[must_use]
pub fn render_autostart_uninstall_error(
    err: &crate::autostart::AutostartError,
    locale: Locale,
) -> String {
    let mut out = format!("error: failed to disable autostart: {err}\n");
    if matches!(locale, Locale::JapaneseEn) {
        out.push_str(&format!("エラー: 自動起動の無効化に失敗しました: {err}\n"));
    }
    out
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

    /// TC-UT-158 (REQ-DDM-004 / AC-DDM-03): MSG-CLI-110 の hint 文面に `--ipc` が含まれない。
    /// Phase 2 で `--ipc` フラグが廃止されたため、hint で案内しない。
    #[test]
    fn tc_ut_158_daemon_not_running_hint_does_not_contain_ipc_flag() {
        use std::path::PathBuf;

        for locale in [Locale::English, Locale::JapaneseEn] {
            let err = CliError::DaemonNotRunning(PathBuf::from("/tmp/test.sock"));
            let rendered = render_error(&err, locale);
            assert!(
                !rendered.contains("--ipc"),
                "MSG-CLI-110 hint must not mention '--ipc' (廃止済み) for locale {locale:?}: {rendered:?}"
            );
            // hint 行に daemon 起動コマンドが含まれる
            assert!(
                rendered.contains("shikomi-daemon"),
                "MSG-CLI-110 hint must mention 'shikomi-daemon' for locale {locale:?}: {rendered:?}"
            );
            // エラー行にソケットパスが含まれる
            assert!(
                rendered.contains("not running"),
                "MSG-CLI-110 must contain 'not running' for locale {locale:?}: {rendered:?}"
            );
        }
    }

    // -------------------------------------------------------------------
    // Issue #141: data-portability Presenter / ExitCode UT — TC-UT-205〜208
    // 設計根拠: docs/features/data-portability/cli/test-design.md §5.3
    // -------------------------------------------------------------------

    /// TC-UT-205 (REQ-DP-010): data-portability 新バリアント 5 種が全て
    /// `ExitCode::UserError`（exit 1）に写像される（SSoT 拡張の局所検証）。
    ///
    /// `tc_f_u15` の全体 SSoT 網羅マトリクスと直交し、本 TC は Issue #141 由来の
    /// 5 バリアントだけに絞った焦点検証として機能する。
    #[test]
    fn tc_ut_205_data_portability_error_variants_all_map_to_exit_1() {
        use crate::error::ExitCode;

        let cases: Vec<(&str, CliError)> = vec![
            ("ExportImportVaultLocked", CliError::ExportImportVaultLocked),
            (
                "ExportOutputFileExists",
                CliError::ExportOutputFileExists {
                    path: std::path::PathBuf::from("/tmp/out.json"),
                },
            ),
            (
                "ImportConflict(empty)",
                CliError::ImportConflict { ids: vec![] },
            ),
            (
                "ImportDeserializationFailed",
                CliError::ImportDeserializationFailed {
                    reason: "reason".to_owned(),
                },
            ),
            (
                "ImportValidationFailed(UnknownFormatVersion)",
                CliError::ImportValidationFailed(
                    shikomi_core::portability::ImportValidationError::UnknownFormatVersion {
                        found: 999,
                    },
                ),
            ),
        ];
        for (name, err) in cases {
            assert_eq!(
                ExitCode::from(&err),
                ExitCode::UserError,
                "TC-UT-205: {name} should map to ExitCode::UserError (exit 1)"
            );
        }
    }

    /// TC-UT-206 (REQ-DP-011 / AC-DP-10): `format_conflict_ids` — 4 件以下は
    /// 全 ID をそのままカンマ区切りで返す（省略なし境界値）。
    #[test]
    fn tc_ut_206_format_conflict_ids_four_or_fewer_returns_all_ids() {
        let ids: Vec<String> = vec![
            "id-1".to_owned(),
            "id-2".to_owned(),
            "id-3".to_owned(),
            "id-4".to_owned(),
        ];
        let result = format_conflict_ids(&ids);
        assert_eq!(
            result, "id-1, id-2, id-3, id-4",
            "4 IDs must be joined without ellipsis"
        );
    }

    /// TC-UT-207 (REQ-DP-011 / AC-DP-10): `format_conflict_ids` — 5 件以上は
    /// 先頭 4 件 + `... (N more)` 形式で省略される（terminal 溢れ防止境界値）。
    #[test]
    fn tc_ut_207_format_conflict_ids_five_or_more_shows_ellipsis() {
        let ids: Vec<String> = vec![
            "a".to_owned(),
            "b".to_owned(),
            "c".to_owned(),
            "d".to_owned(),
            "e".to_owned(),
            "f".to_owned(),
        ];
        let result = format_conflict_ids(&ids);
        assert!(
            result.contains("a, b, c, d"),
            "must contain first 4 IDs 'a, b, c, d', got: {result:?}"
        );
        assert!(
            result.contains("... (2 more)"),
            "must contain '... (2 more)' for 6 total IDs, got: {result:?}"
        );
    }

    /// TC-UT-208 (REQ-DP-010/011 / AC-DP-08): `render_error` —
    /// `ImportValidationFailed(RedactedPayload)` → MSG-CLI-144 文面が出力される。
    ///
    /// `render_error` → `render_import_validation_redacted` dispatch の検証。
    #[test]
    fn tc_ut_208_render_error_import_validation_redacted_returns_msg_cli_144() {
        let err = CliError::ImportValidationFailed(
            shikomi_core::portability::ImportValidationError::RedactedPayload {
                id: "test-id-xyz".to_owned(),
            },
        );
        let out = render_error(&err, Locale::English);
        assert!(
            out.contains("cannot import record test-id-xyz"),
            "must contain 'cannot import record test-id-xyz', got: {out:?}"
        );
        assert!(
            out.contains("payload is redacted"),
            "must contain 'payload is redacted', got: {out:?}"
        );
        assert!(
            out.contains("re-export"),
            "must contain 're-export' hint, got: {out:?}"
        );
    }

    /// TC-UT-158b (Issue #134 / MSG-CLI-110): `render_daemon_not_running()` の出力に
    /// `"shikomi daemon install"` autostart hint が含まれること。
    ///
    /// 英語・JapaneseEn 両ロケールで検証する。
    /// 設計根拠: basic-design.md §Sub-B完了後に更新するメッセージ / §テスト戦略 TC-UT-158 拡張
    #[test]
    fn tc_ut_158b_daemon_not_running_contains_autostart_hint_for_english_locale() {
        use std::path::PathBuf;
        let err = CliError::DaemonNotRunning(PathBuf::from("/tmp/test.sock"));
        let rendered = render_error(&err, Locale::English);
        assert!(
            rendered.contains("shikomi daemon install"),
            "MSG-CLI-110 English must contain autostart hint 'shikomi daemon install': {rendered:?}"
        );
    }

    /// TC-UT-158c (Issue #134 / MSG-CLI-110): JapaneseEn ロケールで
    /// `"shikomi daemon install"` と日本語 hint 行の両方が含まれること。
    ///
    /// 設計根拠: basic-design.md §Sub-B完了後に更新するメッセージ / §テスト戦略 TC-UT-158 拡張
    #[test]
    fn tc_ut_158c_daemon_not_running_contains_autostart_hint_for_japanese_en_locale() {
        use std::path::PathBuf;
        let err = CliError::DaemonNotRunning(PathBuf::from("/tmp/test.sock"));
        let rendered = render_error(&err, Locale::JapaneseEn);
        assert!(
            rendered.contains("shikomi daemon install"),
            "MSG-CLI-110 JapaneseEn must contain 'shikomi daemon install': {rendered:?}"
        );
        assert!(
            rendered.contains("または自動起動を有効にする場合"),
            "MSG-CLI-110 JapaneseEn must contain Japanese autostart hint: {rendered:?}"
        );
    }
}
