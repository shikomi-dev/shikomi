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
}
