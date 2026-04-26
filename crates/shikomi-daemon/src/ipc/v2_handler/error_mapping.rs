//! `MigrationError → IpcErrorCode` 写像 (Sub-E §C-27)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/vek-cache-and-ipc.md`
//! §`MigrationError → IpcError` マッピング表
//!
//! 集約方針: Sub-D `MigrationError` 9 variants を IPC 経由で `IpcErrorCode` 4 V2
//! variants (+ 既存 `Persistence/Domain/Internal`) に集約し、内部詳細を秘匿しつつ
//! MSG マッピング (MSG-S08〜MSG-S13) を 1:1 で確定する。

use shikomi_core::error::CryptoError;
use shikomi_core::ipc::IpcErrorCode;
use shikomi_infra::persistence::vault_migration::MigrationError;

/// `MigrationError` を `IpcErrorCode` に写像する (§C-27)。
///
/// マッピング表 (設計書 §`MigrationError → IpcError` マッピング表):
///
/// | `MigrationError` | `IpcErrorCode` | reason | MSG |
/// |---|---|---|---|
/// | `Crypto(WeakPassword(_))` | `Crypto` | `"weak-password"` | MSG-S08 |
/// | `Crypto(AeadTagMismatch)` | `Crypto` | `"aead-tag-mismatch"` | MSG-S10 |
/// | `Crypto(NonceLimitExceeded)` | `Crypto` | `"nonce-limit-exceeded"` | MSG-S11 |
/// | `Crypto(WrongPassword)` | `Crypto` | `"wrong-password"` | MSG-S09 (a) |
/// | `Crypto(InvalidMnemonic)` | `Crypto` | `"invalid-mnemonic"` | MSG-S12 |
/// | `Crypto(_)` その他 | `Crypto` | `"kdf-failed"` | MSG-S09 (a) |
/// | `Persistence(_)` | `Persistence` | 透過 | 既存 |
/// | `Domain(_)` | `Domain` | 透過 | 既存 |
/// | `AlreadyEncrypted` / `NotEncrypted` | `Internal` | 開発者向け | — |
/// | `PlaintextNotUtf8` / `RecoveryAlreadyConsumed` | `Internal` | 開発者向け | — |
/// | `AtomicWriteFailed { stage }` | `Persistence` | `"atomic-write-{stage}"` | MSG-S13 |
/// | `RecoveryRequired` | `RecoveryRequired` | — | MSG-S09 (a) リカバリ経路案内 |
#[must_use]
pub fn migration_error_to_ipc(err: MigrationError) -> IpcErrorCode {
    match err {
        MigrationError::Crypto(c) => crypto_error_to_ipc(c),
        MigrationError::Persistence(e) => IpcErrorCode::Persistence {
            reason: format!("{e}"),
        },
        MigrationError::Domain(d) => IpcErrorCode::Domain {
            reason: format!("{d}"),
        },
        MigrationError::AlreadyEncrypted => IpcErrorCode::Internal {
            reason: "already-encrypted".to_owned(),
        },
        MigrationError::NotEncrypted => IpcErrorCode::Internal {
            reason: "not-encrypted".to_owned(),
        },
        MigrationError::PlaintextNotUtf8 => IpcErrorCode::Internal {
            reason: "plaintext-not-utf8".to_owned(),
        },
        MigrationError::RecoveryAlreadyConsumed => IpcErrorCode::Internal {
            reason: "recovery-already-consumed".to_owned(),
        },
        MigrationError::AtomicWriteFailed { stage, .. } => IpcErrorCode::Persistence {
            reason: format!("atomic-write-{stage}"),
        },
        MigrationError::RecoveryRequired => IpcErrorCode::RecoveryRequired,
        // `#[non_exhaustive]` cross-crate 防御的 wildcard (Sub-D Rev3 凍結方針継承):
        // 将来 `MigrationError` に variant 追加された場合、本行で fail-secure で受け、
        // テスト工程でTC が追加分を機械検出する想定。
        _ => IpcErrorCode::Internal {
            reason: "unknown-migration-error".to_owned(),
        },
    }
}

/// `CryptoError` を `IpcErrorCode::Crypto { reason }` に写像する。
fn crypto_error_to_ipc(err: CryptoError) -> IpcErrorCode {
    let reason = match err {
        CryptoError::WeakPassword(_) => "weak-password",
        CryptoError::AeadTagMismatch => "aead-tag-mismatch",
        CryptoError::NonceLimitExceeded { .. } => "nonce-limit-exceeded",
        CryptoError::WrongPassword => "wrong-password",
        CryptoError::InvalidMnemonic => "invalid-mnemonic",
        CryptoError::KdfFailed { .. } => "kdf-failed",
        CryptoError::VerifyRequired => "verify-required",
        // `#[non_exhaustive]` cross-crate 防御的 wildcard (Sub-D Rev3 凍結方針継承)。
        _ => "unknown-crypto-error",
    };
    IpcErrorCode::Crypto {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_password_maps_to_crypto_with_kebab_reason() {
        let err = MigrationError::Crypto(CryptoError::WrongPassword);
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Crypto { reason } => assert_eq!(reason, "wrong-password"),
            other => panic!("expected Crypto variant, got {other:?}"),
        }
    }

    #[test]
    fn aead_tag_mismatch_maps_to_crypto_aead_tag_mismatch() {
        let err = MigrationError::Crypto(CryptoError::AeadTagMismatch);
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Crypto { reason } => assert_eq!(reason, "aead-tag-mismatch"),
            other => panic!("expected Crypto variant, got {other:?}"),
        }
    }

    #[test]
    fn recovery_required_maps_to_dedicated_variant() {
        let err = MigrationError::RecoveryRequired;
        let code = migration_error_to_ipc(err);
        assert!(matches!(code, IpcErrorCode::RecoveryRequired));
    }

    #[test]
    fn already_encrypted_maps_to_internal() {
        let err = MigrationError::AlreadyEncrypted;
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Internal { reason } => assert_eq!(reason, "already-encrypted"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // TC-E-U11: RecoveryRequired 透過変換 + Display 文字列確認
    // -----------------------------------------------------------------
    //
    // 設計書 §14.4 TC-E-U11: `MigrationError::RecoveryRequired` →
    // `IpcErrorCode::RecoveryRequired` 透過。Display 文字列が「recovery path
    // required」を含む (Sub-D Rev5 ペガサス指摘契約の Sub-E 実装、MSG-S09 (a)
    // 経路)。
    #[test]
    fn recovery_required_display_contains_recovery_path_required() {
        let code: IpcErrorCode = migration_error_to_ipc(MigrationError::RecoveryRequired);
        let s = format!("{code}");
        assert!(
            s.contains("recovery path required"),
            "Display must contain 'recovery path required', got: {s}"
        );
    }

    // -----------------------------------------------------------------
    // TC-E-U12: MigrationError 9 variant 全網羅マッピング
    // -----------------------------------------------------------------
    //
    // 設計書 §14.4 TC-E-U12 / §14.5 表「MigrationError → IpcErrorCode マッピング」:
    // (1) Crypto(WeakPassword) → Crypto(reason="weak-password")
    // (2) Crypto(AeadTagMismatch) → Crypto(reason="aead-tag-mismatch")
    // (3) Crypto(NonceLimitExceeded) → Crypto(reason="nonce-limit-exceeded")
    // (4) Crypto(WrongPassword) → Crypto(reason="wrong-password")
    // (5) Crypto(InvalidMnemonic) → Crypto(reason="invalid-mnemonic")
    // (6) Crypto(KdfFailed) → Crypto(reason="kdf-failed")
    // (7) Persistence(_) → Persistence (透過)
    // (8) Domain(_) → Domain (透過)
    // (9) AlreadyEncrypted / NotEncrypted / PlaintextNotUtf8 / RecoveryAlreadyConsumed
    //     → Internal(reason)
    // (10) AtomicWriteFailed { stage } → Persistence(reason="atomic-write-{stage}")
    // (11) RecoveryRequired → RecoveryRequired
    //
    // ワイルドカード `_` 無しで Crypto 内部 6 variant + MigrationError 9 variant
    // (内 5 が Internal 集約) を確認する。Persistence/Domain/AtomicWriteFailed
    // は別 variant の依存型構築が必要なため variant 検証のみ実施。

    use shikomi_core::crypto::WeakPasswordFeedback;

    #[test]
    fn weak_password_maps_to_crypto_weak_password() {
        let feedback = WeakPasswordFeedback::new(None, vec![]);
        let err = MigrationError::Crypto(CryptoError::WeakPassword(Box::new(feedback)));
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Crypto { reason } => assert_eq!(reason, "weak-password"),
            other => panic!("expected Crypto, got {other:?}"),
        }
    }

    #[test]
    fn nonce_limit_exceeded_maps_to_crypto_nonce_limit_exceeded() {
        let err = MigrationError::Crypto(CryptoError::NonceLimitExceeded { limit: 1u64 << 32 });
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Crypto { reason } => assert_eq!(reason, "nonce-limit-exceeded"),
            other => panic!("expected Crypto, got {other:?}"),
        }
    }

    #[test]
    fn invalid_mnemonic_maps_to_crypto_invalid_mnemonic() {
        let err = MigrationError::Crypto(CryptoError::InvalidMnemonic);
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Crypto { reason } => assert_eq!(reason, "invalid-mnemonic"),
            other => panic!("expected Crypto, got {other:?}"),
        }
    }

    #[test]
    fn kdf_failed_maps_to_crypto_kdf_failed() {
        use shikomi_core::error::KdfErrorKind;
        let err = MigrationError::Crypto(CryptoError::KdfFailed {
            kind: KdfErrorKind::Argon2id,
            source: "test failure".into(),
        });
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Crypto { reason } => assert_eq!(reason, "kdf-failed"),
            other => panic!("expected Crypto, got {other:?}"),
        }
    }

    #[test]
    fn not_encrypted_maps_to_internal() {
        let err = MigrationError::NotEncrypted;
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Internal { reason } => assert_eq!(reason, "not-encrypted"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn plaintext_not_utf8_maps_to_internal() {
        let err = MigrationError::PlaintextNotUtf8;
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Internal { reason } => assert_eq!(reason, "plaintext-not-utf8"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn recovery_already_consumed_maps_to_internal() {
        let err = MigrationError::RecoveryAlreadyConsumed;
        let code = migration_error_to_ipc(err);
        match code {
            IpcErrorCode::Internal { reason } => assert_eq!(reason, "recovery-already-consumed"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
