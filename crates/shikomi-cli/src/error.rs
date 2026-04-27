//! CLI 層のエラー型と終了コード。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/data-structures.md
//! §`CliError` バリアント詳細、§`CliError` の `From` 実装

use std::path::PathBuf;

use shikomi_core::ipc::{IpcErrorCode, IpcProtocolVersion};
use shikomi_core::{DomainError, RecordId};
use shikomi_infra::persistence::PersistenceError;
use thiserror::Error;

// -------------------------------------------------------------------
// CliError
// -------------------------------------------------------------------

/// CLI 全体が返すエラー型。
///
/// i18n 併記は `presenter::error::render_error` の責務。`Display` は英語原文のみ。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CliError {
    /// clap usage error / フラグ併用違反（人間可読の英文を保持）
    #[error("{0}")]
    UsageError(String),

    /// `RecordLabel::try_new` 失敗
    #[error("invalid label: {0}")]
    InvalidLabel(DomainError),

    /// `RecordId::try_from_str` 失敗
    #[error("invalid record id: {0}")]
    InvalidId(DomainError),

    /// 対象レコードが vault に存在しない
    #[error("record not found: {0}")]
    RecordNotFound(RecordId),

    /// vault 未初期化（`list` / `edit` / `remove` のみ。`add` は自動作成）
    #[error("vault not initialized at {0}")]
    VaultNotInitialized(PathBuf),

    /// 非 TTY で `remove --yes` 未指定
    #[error("refusing to delete without --yes in non-interactive mode")]
    NonInteractiveRemove,

    /// 永続化層のエラー
    #[error("persistence error: {0}")]
    Persistence(PersistenceError),

    /// 予期しないドメインエラー（集約整合性等）
    #[error("domain error: {0}")]
    Domain(DomainError),

    /// 暗号化 vault 検出（Phase 1 未対応）
    #[error("this vault is encrypted; encryption is not yet supported in this CLI version")]
    EncryptionUnsupported,

    /// `--ipc` 指定で daemon に接続できない（daemon 未起動）
    #[error("shikomi-daemon is not running (socket {0} unreachable)")]
    DaemonNotRunning(PathBuf),

    /// IPC ハンドシェイクで daemon と CLI のプロトコルバージョンが不一致
    #[error("protocol version mismatch (server={server}, client={client})")]
    ProtocolVersionMismatch {
        /// daemon 側バージョン。
        server: IpcProtocolVersion,
        /// クライアント側バージョン。
        client: IpcProtocolVersion,
    },

    // ---------------- Sub-F (#44) Phase 2: vault サブコマンド経路 ----------------
    /// vault が Locked 状態で V2 read/write IPC を受信した
    /// （MSG-S09(c)、設計書 §終了コード SSoT exit 3）。
    #[error("vault is locked, run `shikomi vault unlock` first")]
    VaultLocked,
    /// パスワード違い (MSG-S09(a) 経路、`Crypto { reason: "wrong-password" }`)。
    /// 設計書 §終了コード SSoT exit 2。
    #[error("wrong password")]
    WrongPassword,
    /// 連続 unlock 失敗 5 回後の指数バックオフ中
    /// （MSG-S09(a) 待機表示、設計書 §終了コード SSoT exit 2）。
    #[error("unlock blocked by backoff for {wait_secs}s")]
    BackoffActive {
        /// 待機秒数（ユーザ表示用）。
        wait_secs: u32,
    },
    /// recovery 経路への移行案内
    /// （MSG-S09(a) 派生、`MigrationError::RecoveryRequired` 透過、exit 5）。
    #[error("recovery path required; retry with `shikomi vault unlock --recovery`")]
    RecoveryRequired,
    /// V1 client が V2 専用 variant を送出した（C-28 違反、MSG-S15、exit 4）。
    #[error("ipc protocol downgrade detected (V1 client cannot use V2-only request)")]
    ProtocolDowngrade,
    /// 暗号エラー透過 (MSG-S08〜S12、exit 1)。`reason` は kebab-case 固定文言のみ。
    #[error("crypto error: {reason}")]
    Crypto {
        /// 固定文言（"weak-password" / "aead-tag-mismatch" / "nonce-limit-exceeded" 等）。
        reason: String,
    },
    /// daemon から想定外 IPC variant 受信（プロトコル違反、exit 2）。
    #[error("unexpected ipc response for {request_kind}")]
    UnexpectedIpcResponse {
        /// 直前に送出したリクエスト種別（static 文字列）。
        request_kind: &'static str,
    },

    // ---------------- Sub-F (#44) Phase 3: 保護モードバナー ----------------
    /// daemon が `protection_mode = Unknown` を返した
    /// (vault.db ヘッダ破損等、REQ-S16 Fail-Secure、`cli-subcommands.md`
    /// §終了コード SSoT exit 3)。`shikomi list` は実行せず即座に fail-fast。
    #[error("vault protection mode is unknown; refusing to list records (fail-secure)")]
    ProtectionModeUnknown,

    // ---------------- Sub-F (#44) Phase 5: stdin 拒否 / TTY 強制 (C-38) ----------------
    /// stdin が非 TTY の状態でパスワード / 24 語入力が要求された
    /// (C-38、`echo pw | shikomi vault unlock` の history / scrollback 漏洩防止)。
    /// 設計書 §終了コード SSoT exit 1 (UserError)。
    #[error("refusing to read password from non-tty stdin (run from a terminal)")]
    NonInteractivePassword,

    // ---------------- Issue #75 / Bug-F-001: 認証フラグ排他違反 (defensive) ----------------
    /// 複数の認証経路（`--recovery` と password 入力等）が同時指定された動的 defensive 経路。
    /// clap の `conflicts_with` で静的に弾けない経路（将来の代替認証フラグ追加 / `--recovery`
    /// 経路への動的遷移時の状態遷移エラー等）に備える防衛線。MSG-S21 で文言化、`EX_USAGE`
    /// (exit 64) として `§終了コード SSoT` に整合（`cli-subcommands.md` §Bug-F-001 解消
    /// §排他違反検知 (defensive) 行）。
    #[error("conflicting authentication flags: {hint}")]
    IncompatibleAuthFlags {
        /// 衝突した経路の固定文言ヒント（`&'static str`、動的データ非含有）。
        hint: &'static str,
    },
}

/// daemon から返る `IpcErrorCode` を CLI 層エラーに写像する（Sub-F #44 Phase 2）。
///
/// 設計書 §終了コード SSoT に基づき、各 V2 variant を専用 `CliError` 変種に
/// 直接寄せて exit code 写像を再分散しないようにする（ドリフト防止）。
/// `EncryptionUnsupported` / `NotFound` 等の V1 variant は従来通り
/// `PersistenceError` 経路に流して既存写像を維持する。
impl From<IpcErrorCode> for CliError {
    fn from(code: IpcErrorCode) -> Self {
        match code {
            IpcErrorCode::VaultLocked => Self::VaultLocked,
            IpcErrorCode::BackoffActive { wait_secs } => Self::BackoffActive { wait_secs },
            IpcErrorCode::RecoveryRequired => Self::RecoveryRequired,
            IpcErrorCode::ProtocolDowngrade => Self::ProtocolDowngrade,
            IpcErrorCode::Crypto { reason } => match reason.as_str() {
                "wrong-password" => Self::WrongPassword,
                _ => Self::Crypto { reason },
            },
            // V1 系および未知 variant は従来通り PersistenceError 経路を経由する。
            // `From<PersistenceError> for CliError` が `EncryptionUnsupported` /
            // `RecordNotFound` 等を専用 variant に再写像するので、ここで握り潰さず
            // 既存写像表に丸ごと委譲する（DRY、`#[non_exhaustive]` 防衛も継承）。
            other => Self::from(PersistenceError::from(other)),
        }
    }
}

impl From<PersistenceError> for CliError {
    fn from(e: PersistenceError) -> Self {
        // 暗号化 vault 検出は `PersistenceError::UnsupportedYet { feature: "encrypted vault persistence", .. }`
        // の形で infra 層から返る（`shikomi-infra::persistence::repository.rs` の
        // `load_inner` / `save_inner` が Encrypted モードで Fail Fast）。
        // CLI 層はこれを専用の `EncryptionUnsupported` バリアントへ写像し、
        // `ExitCode::EncryptionUnsupported (3)` に繋げる（REQ-CLI-009 / 受入基準 8）。
        match e {
            PersistenceError::UnsupportedYet {
                feature: "encrypted vault persistence",
                ..
            } => Self::EncryptionUnsupported,
            // IPC 由来の特定バリアントは CLI 専用バリアントへ写像（MSG-CLI-110/111）
            PersistenceError::DaemonNotRunning(path) => Self::DaemonNotRunning(path),
            PersistenceError::ProtocolVersionMismatch { server, client } => {
                Self::ProtocolVersionMismatch { server, client }
            }
            // Phase 1.5（Issue #30）: daemon 側 `IpcErrorCode::NotFound { id }` 由来の
            // `PersistenceError::RecordNotFound(id)` は CLI 既存の同名バリアントへ写像し、
            // SQLite 経路と同じ presenter 経路（MSG-CLI-103）に着地させる（DRY、UX 同一）。
            PersistenceError::RecordNotFound(id) => Self::RecordNotFound(id),
            other => Self::Persistence(other),
        }
    }
}

// -------------------------------------------------------------------
// ExitCode
// -------------------------------------------------------------------

/// CLI の終了コード。`std::process::Termination` を実装し、`main() -> ExitCode` から返せる。
///
/// Sub-F (#44) で SSoT 化（cli-subcommands.md §終了コード SSoT、ペガサス致命指摘 ② 解消）。
/// 個別箇所での再定義禁止、本 enum のみが**唯一の真実源**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// 成功 (`EX_OK`、sysexits.h)。
    Success = 0,
    /// ユーザ入力 / 一般エラー (MSG-S08 弱パスワード / MSG-S12 mnemonic 認識失敗 /
    /// MSG-S13 マイグレーション失敗 / MSG-S10 AEAD 改竄 / paste-suspected /
    /// recovery-already-disclosed 等、F-F1〜F-F7 共通)。Phase 1 既存 `UserError` を継承。
    UserError = 1,
    /// パスワード違い / 連続失敗バックオフ
    /// (MSG-S09(a)、Sub-F SSoT exit 2)。Phase 1 既存 `SystemError` 命名は維持しつつ意味を拡張。
    /// 旧 `SystemError`（I/O エラー等）も exit 2 に集約して整合（Sub-F 設計書 §終了コード SSoT）。
    SystemError = 2,
    /// vault Locked / 保護モード Unknown 検出
    /// (MSG-S09(c) / REQ-S16 Fail-Secure、Sub-F SSoT exit 3)。
    /// 旧 `EncryptionUnsupported`（Phase 1 plaintext-only 互換）も意味的に同 exit 3 に
    /// 寄せる（vault が暗号化されていて未対応 → Locked と同じく「現状読めない」状態）。
    EncryptionUnsupported = 3,
    /// プロトコル非互換 (MSG-S15、handshake 段で fail fast、Sub-F SSoT exit 4)。
    ProtocolDowngrade = 4,
    /// recovery 経路必須 (MSG-S09(a) 派生、Sub-F SSoT exit 5)。
    RecoveryRequired = 5,
    /// 認証フラグ排他違反 (Issue #75 / Bug-F-001 §排他違反検知 (defensive)、`EX_USAGE` /
    /// sysexits.h、`cli-subcommands.md` §終了コード SSoT 表 64 行 §脚注「`IncompatibleAuthFlags`
    /// も `EX_USAGE` 経路」)。clap `conflicts_with` で静的に弾ききれない動的経路の defensive 受け皿。
    UsageError64 = 64,
}

impl std::process::Termination for ExitCode {
    fn report(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self as u8)
    }
}

impl From<&CliError> for ExitCode {
    fn from(err: &CliError) -> Self {
        match err {
            // exit 1（UserError）: MSG-S08/S10/S12/S13 系 + 既存 V1 ユーザ入力エラー
            CliError::UsageError(_)
            | CliError::InvalidLabel(_)
            | CliError::InvalidId(_)
            | CliError::RecordNotFound(_)
            | CliError::VaultNotInitialized(_)
            | CliError::NonInteractiveRemove
            | CliError::NonInteractivePassword
            | CliError::DaemonNotRunning(_)
            | CliError::ProtocolVersionMismatch { .. }
            | CliError::Crypto { .. } => Self::UserError,
            // exit 2（SystemError / WrongPassword）: 既存 I/O 系 + Sub-F SSoT パスワード違い
            CliError::Persistence(_)
            | CliError::Domain(_)
            | CliError::WrongPassword
            | CliError::BackoffActive { .. }
            | CliError::UnexpectedIpcResponse { .. } => Self::SystemError,
            // exit 3: 暗号化未対応 / vault Locked / 保護モード Unknown（Sub-F SSoT）
            CliError::EncryptionUnsupported
            | CliError::VaultLocked
            | CliError::ProtectionModeUnknown => Self::EncryptionUnsupported,
            // exit 4: プロトコル非互換 (handshake 段 fail fast)
            CliError::ProtocolDowngrade => Self::ProtocolDowngrade,
            // exit 5: recovery 経路必須
            CliError::RecoveryRequired => Self::RecoveryRequired,
            // exit 64 (`EX_USAGE`、Issue #75 Bug-F-001 §排他違反検知 (defensive))
            CliError::IncompatibleAuthFlags { .. } => Self::UsageError64,
        }
    }
}

// -------------------------------------------------------------------
// テスト
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::error::InvalidRecordLabelReason;

    #[test]
    fn test_exit_code_usage_error_maps_to_user_error() {
        let err = CliError::UsageError("x".to_owned());
        assert_eq!(ExitCode::from(&err), ExitCode::UserError);
    }

    #[test]
    fn test_exit_code_invalid_label_maps_to_user_error() {
        let err = CliError::InvalidLabel(DomainError::InvalidRecordLabel(
            InvalidRecordLabelReason::Empty,
        ));
        assert_eq!(ExitCode::from(&err), ExitCode::UserError);
    }

    #[test]
    fn test_exit_code_non_interactive_remove_maps_to_user_error() {
        assert_eq!(
            ExitCode::from(&CliError::NonInteractiveRemove),
            ExitCode::UserError
        );
    }

    /// Sub-F (#44) Phase 5 / C-38: 非 TTY パスワード入力は exit 1 に写像される。
    #[test]
    fn test_exit_code_non_interactive_password_maps_to_user_error() {
        assert_eq!(
            ExitCode::from(&CliError::NonInteractivePassword),
            ExitCode::UserError
        );
    }

    #[test]
    fn test_exit_code_encryption_unsupported_maps_to_exit_3() {
        assert_eq!(ExitCode::from(&CliError::EncryptionUnsupported) as u8, 3);
    }

    #[test]
    fn test_display_invalid_label_does_not_contain_secret_marker() {
        // CliError::Display はラベル検証エラー文を英語で返す。secret 値は含まれない。
        let err = CliError::InvalidLabel(DomainError::InvalidRecordLabel(
            InvalidRecordLabelReason::Empty,
        ));
        let msg = err.to_string();
        assert!(msg.contains("invalid label"));
        assert!(!msg.contains("SECRET_TEST_VALUE"));
    }

    /// BUG-001 回帰: `PersistenceError::UnsupportedYet { feature: "encrypted vault persistence", .. }`
    /// は `CliError::EncryptionUnsupported`（exit 3）に写像されなければならない。
    /// 以前は無条件に `CliError::Persistence(...)` に包まれ、exit 2 になっていた。
    #[test]
    fn test_from_persistence_encrypted_vault_maps_to_encryption_unsupported() {
        let pe = PersistenceError::UnsupportedYet {
            feature: "encrypted vault persistence",
            tracking_issue: None,
        };
        let cli_err: CliError = pe.into();
        assert!(matches!(cli_err, CliError::EncryptionUnsupported));
        assert_eq!(ExitCode::from(&cli_err), ExitCode::EncryptionUnsupported);
    }

    /// 他の `UnsupportedYet`（将来の未実装機能）は `CliError::Persistence` に留める。
    #[test]
    fn test_from_persistence_other_unsupported_maps_to_persistence() {
        let pe = PersistenceError::UnsupportedYet {
            feature: "some other future feature",
            tracking_issue: None,
        };
        let cli_err: CliError = pe.into();
        assert!(matches!(cli_err, CliError::Persistence(_)));
        assert_eq!(ExitCode::from(&cli_err), ExitCode::SystemError);
    }

    /// Phase 1.5（Issue #30）: `PersistenceError::RecordNotFound(id)` は
    /// CLI 既存 `RecordNotFound(id)` に直結（SQLite 経路と同 UX）。
    #[test]
    fn test_from_persistence_record_not_found_maps_to_record_not_found() {
        let id =
            RecordId::new(uuid::Uuid::now_v7()).expect("uuid v7 must satisfy RecordId invariant");
        let pe = PersistenceError::RecordNotFound(id);
        let cli_err: CliError = pe.into();
        assert!(matches!(cli_err, CliError::RecordNotFound(_)));
        assert_eq!(ExitCode::from(&cli_err), ExitCode::UserError);
    }

    /// Phase 1.5: `PersistenceError::Internal { reason }` は `Persistence` に
    /// 寄せ、終了コードは `SystemError`（exit 2）。reason 文字列は固定文言のみ。
    #[test]
    fn test_from_persistence_internal_maps_to_persistence_system_error() {
        let pe = PersistenceError::Internal {
            reason: "persistence error".into(),
        };
        let cli_err: CliError = pe.into();
        assert!(matches!(cli_err, CliError::Persistence(_)));
        assert_eq!(ExitCode::from(&cli_err), ExitCode::SystemError);
    }

    // -----------------------------------------------------------------
    // Sub-F (#44) Phase 2: From<IpcErrorCode> + ExitCode SSoT 整合確認
    // 設計根拠: cli-subcommands.md §終了コード SSoT
    // -----------------------------------------------------------------

    #[test]
    fn test_from_ipc_error_vault_locked_maps_to_vault_locked_exit_3() {
        let cli_err: CliError = IpcErrorCode::VaultLocked.into();
        assert!(matches!(cli_err, CliError::VaultLocked));
        assert_eq!(ExitCode::from(&cli_err) as u8, 3);
    }

    #[test]
    fn test_from_ipc_error_backoff_active_maps_to_backoff_active_exit_2() {
        let cli_err: CliError = IpcErrorCode::BackoffActive { wait_secs: 30 }.into();
        match &cli_err {
            CliError::BackoffActive { wait_secs } => assert_eq!(*wait_secs, 30),
            other => panic!("expected BackoffActive, got {other:?}"),
        }
        assert_eq!(ExitCode::from(&cli_err) as u8, 2);
    }

    #[test]
    fn test_from_ipc_error_recovery_required_maps_to_recovery_required_exit_5() {
        let cli_err: CliError = IpcErrorCode::RecoveryRequired.into();
        assert!(matches!(cli_err, CliError::RecoveryRequired));
        assert_eq!(ExitCode::from(&cli_err) as u8, 5);
    }

    #[test]
    fn test_from_ipc_error_protocol_downgrade_maps_to_protocol_downgrade_exit_4() {
        let cli_err: CliError = IpcErrorCode::ProtocolDowngrade.into();
        assert!(matches!(cli_err, CliError::ProtocolDowngrade));
        assert_eq!(ExitCode::from(&cli_err) as u8, 4);
    }

    #[test]
    fn test_from_ipc_error_crypto_wrong_password_maps_to_wrong_password_exit_2() {
        let cli_err: CliError = IpcErrorCode::Crypto {
            reason: "wrong-password".to_owned(),
        }
        .into();
        assert!(matches!(cli_err, CliError::WrongPassword));
        assert_eq!(ExitCode::from(&cli_err) as u8, 2);
    }

    #[test]
    fn test_from_ipc_error_crypto_other_reason_preserves_kebab_string_exit_1() {
        let cli_err: CliError = IpcErrorCode::Crypto {
            reason: "aead-tag-mismatch".to_owned(),
        }
        .into();
        match &cli_err {
            CliError::Crypto { reason } => assert_eq!(reason, "aead-tag-mismatch"),
            other => panic!("expected Crypto, got {other:?}"),
        }
        assert_eq!(ExitCode::from(&cli_err) as u8, 1);
    }

    /// TC-F-U15 (REQ-S15 / cli-subcommands.md §終了コード SSoT、**旧 TC-F-U09 リナンバ**):
    /// `usecase::vault::*` 戻り値型 `Result<ExitCode, CliError>` 経由で `CliError` 各
    /// variant が **cli-subcommands.md §終了コード SSoT 表通り**に `ExitCode` へ写像
    /// されることの **網羅マトリクス機械検証** (Rev1 ペガサス致命指摘②解消)。
    ///
    /// 設計書 §15.5 #15 の SSoT 表:
    ///   成功 = **0** / 一般エラー = **1** / `WrongPassword`,`BackoffActive` = **2** /
    ///   `VaultLocked`,`ProtectionModeUnknown`,`EncryptionUnsupported` = **3** /
    ///   `ProtocolDowngrade` = **4** / `RecoveryRequired` = **5** /
    ///   `IncompatibleAuthFlags` = **64 (`EX_USAGE`)** / `EX_CONFIG` = 78 (現状未使用)。
    ///
    /// 既存個別 TC (`test_exit_code_*`) と直交し、本 TC は **マトリクスを 1 関数で
    /// 一括 articulate** することで、将来 `CliError` 新 variant 追加時に SSoT ドリフト
    /// を即座に検出する SSoT 1 ファイル化を担保する (Bug-G-005 同型再演防止、
    /// issue-76-verification.md §15.17.3.3 `Err パスも明示的にカバー`)。
    ///
    /// 配置先: `crates/shikomi-cli/src/error.rs::tests` (issue-76-verification.md §15.17.1
    /// 推奨配置 `usecase/vault/unlock.rs::tests` を **cli-subcommands.md §終了コード SSoT
    /// が `error::ExitCode` で SSoT 化**されている実装事実に追従。`unlock` だけでなく
    /// 7 vault サブコマンド全てが共通 SSoT を共有するため、変換マトリクス本体は
    /// `error.rs` で 1 箇所に集約)。
    #[test]
    fn tc_f_u15_exit_code_ssot_mapping_for_all_cli_error_variants_in_one_matrix() {
        use shikomi_core::error::InvalidRecordLabelReason;

        // SSoT 表に従う網羅マトリクス。新 variant 追加時はここに 1 行追加して SSoT
        // の機械検証を更新する (Open/Closed)。
        // `DomainError` は `Clone` 非実装のため、必要箇所で都度構築する。
        let make_label_err = || DomainError::InvalidRecordLabel(InvalidRecordLabelReason::Empty);
        let id =
            RecordId::new(uuid::Uuid::now_v7()).expect("uuid v7 must satisfy RecordId invariant");

        // (exit 1) UserError 経路の SSoT 一括検証。
        let user_error_cases: Vec<(&str, CliError)> = vec![
            ("UsageError", CliError::UsageError("x".to_owned())),
            ("InvalidLabel", CliError::InvalidLabel(make_label_err())),
            ("InvalidId", CliError::InvalidId(make_label_err())),
            ("RecordNotFound", CliError::RecordNotFound(id.clone())),
            (
                "VaultNotInitialized",
                CliError::VaultNotInitialized(std::path::PathBuf::from("/tmp/vault")),
            ),
            ("NonInteractiveRemove", CliError::NonInteractiveRemove),
            ("NonInteractivePassword", CliError::NonInteractivePassword),
            (
                "DaemonNotRunning",
                CliError::DaemonNotRunning(std::path::PathBuf::from("/tmp/sock")),
            ),
            (
                "ProtocolVersionMismatch",
                CliError::ProtocolVersionMismatch {
                    server: IpcProtocolVersion::V2,
                    client: IpcProtocolVersion::V1,
                },
            ),
            (
                "Crypto(other)",
                CliError::Crypto {
                    reason: "aead-tag-mismatch".to_owned(),
                },
            ),
        ];
        for (name, err) in user_error_cases {
            assert_eq!(
                ExitCode::from(&err) as u8,
                1,
                "SSoT exit 1 (UserError) expected for {name}, but mapping drifted"
            );
        }

        // (exit 2) SystemError 経路 (パスワード違い / Backoff / I/O 等)。
        let system_error_cases: Vec<(&str, CliError)> = vec![
            (
                "Persistence",
                CliError::Persistence(shikomi_infra::persistence::PersistenceError::Internal {
                    reason: "x".into(),
                }),
            ),
            ("Domain", CliError::Domain(make_label_err())),
            ("WrongPassword", CliError::WrongPassword),
            (
                "BackoffActive(30s)",
                CliError::BackoffActive { wait_secs: 30 },
            ),
            (
                "UnexpectedIpcResponse",
                CliError::UnexpectedIpcResponse {
                    request_kind: "ListRecords",
                },
            ),
        ];
        for (name, err) in system_error_cases {
            assert_eq!(
                ExitCode::from(&err) as u8,
                2,
                "SSoT exit 2 (SystemError) expected for {name}, but mapping drifted"
            );
        }

        // (exit 3) EncryptionUnsupported / VaultLocked / ProtectionModeUnknown SSoT。
        for (name, err) in [
            ("EncryptionUnsupported", CliError::EncryptionUnsupported),
            ("VaultLocked", CliError::VaultLocked),
            ("ProtectionModeUnknown", CliError::ProtectionModeUnknown),
        ] {
            assert_eq!(
                ExitCode::from(&err) as u8,
                3,
                "SSoT exit 3 expected for {name}, but mapping drifted"
            );
        }

        // (exit 4) ProtocolDowngrade SSoT (handshake 段 fail-fast)。
        assert_eq!(
            ExitCode::from(&CliError::ProtocolDowngrade) as u8,
            4,
            "SSoT exit 4 expected for ProtocolDowngrade"
        );

        // (exit 5) RecoveryRequired SSoT。
        assert_eq!(
            ExitCode::from(&CliError::RecoveryRequired) as u8,
            5,
            "SSoT exit 5 expected for RecoveryRequired"
        );

        // (exit 64) IncompatibleAuthFlags SSoT (`EX_USAGE`、Issue #75 Bug-F-001
        // §排他違反検知 (defensive))。
        assert_eq!(
            ExitCode::from(&CliError::IncompatibleAuthFlags {
                hint: "password and --recovery cannot be combined",
            }) as u8,
            64,
            "SSoT exit 64 (EX_USAGE) expected for IncompatibleAuthFlags"
        );

        // 成功経路 (`Ok`) は ExitCode::Success = 0。`usecase::vault::*` の戻り値型
        // `Result<(), CliError>` で `Ok` 時に `lib::run` が `ExitCode::Success` を返す
        // 経路 (本 TC は `ExitCode::Success as u8 == 0` の SSoT 1 行を担保)。
        assert_eq!(ExitCode::Success as u8, 0, "SSoT exit 0 (Success)");
    }
}
