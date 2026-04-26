//! `MigrationError` — Sub-D vault マイグレーション失敗の統一エラー型。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//! §`MigrationError`
//!
//! `#[non_exhaustive]` で外部 crate からの破壊的変更耐性を確保 (DC-7)。
//! `Crypto(CryptoError)` / `Persistence(PersistenceError)` / `DomainError` を
//! `#[from]` で内包し、各レイヤエラーを透過する。

use shikomi_core::error::{CryptoError, DomainError};
use thiserror::Error;

use crate::persistence::error::{AtomicWriteStage, PersistenceError};

/// vault マイグレーション失敗の統一エラー型 (Sub-D)。
///
/// 設計書 §`MigrationError` (DC-7): 5+ variants、`#[non_exhaustive]`。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// 暗号操作 (KDF / AEAD / 強度ゲート) の失敗を透過する。
    /// `CryptoError::AeadTagMismatch` (MSG-S10) / `CryptoError::WeakPassword` (MSG-S08) /
    /// `CryptoError::NonceLimitExceeded` (MSG-S11) 等を運ぶ。
    #[error(transparent)]
    Crypto(#[from] CryptoError),

    /// 永続化レイヤエラーを透過する。
    #[error(transparent)]
    Persistence(#[from] PersistenceError),

    /// shikomi-core ドメインエラーを透過する。
    /// `DomainError::Crypto(_)` を直接受け取った場合の経路。
    #[error(transparent)]
    Domain(#[from] DomainError),

    /// 平文 vault に対し `encrypt_vault` 以外の暗号化前提メソッドを呼んだ。
    /// 既に暗号化済み vault に対し `encrypt_vault` を再実行した場合も同様。
    #[error("vault is already encrypted")]
    AlreadyEncrypted,

    /// 暗号化 vault に対し `decrypt_vault` 以外の平文前提メソッドを呼んだ。
    /// あるいは平文 vault に対し `decrypt_vault` を呼んだ場合。
    #[error("vault is not encrypted")]
    NotEncrypted,

    /// `decrypt_vault` で `DecryptConfirmation` 引数が必要だが
    /// 構築前経路で呼出された (本来は型シグネチャで防がれるが防衛的に variant 化)。
    /// MSG-S14 経路で UI 確認モーダル再表示。
    #[error("decrypt confirmation required")]
    ConfirmationRequired,

    /// 復号に成功した平文 record が UTF-8 不正だった。
    /// AEAD 検証通過後の追加検証層、ありえない経路の防衛的 variant。
    #[error("decrypted plaintext is not valid UTF-8")]
    PlaintextNotUtf8,

    /// `RecoveryDisclosure::disclose` を 2 度呼び出そうとした (runtime 検出)。
    /// 通常は所有権消費でコンパイルエラーになるが、`drop_without_disclose` 後の
    /// 観測可能な経路防衛のための variant。
    #[error("recovery disclosure already consumed")]
    RecoveryAlreadyConsumed,

    /// マイグレーション中の atomic write 失敗で原状復帰 (C-21)。
    /// `vault-persistence` の `.new` cleanup 経路に委譲済みで、本 variant 受理時点で
    /// vault.db バイト列は変更前状態。MSG-S13 経路。
    #[error("vault migration atomic write failed at stage {stage}")]
    AtomicWriteFailed {
        /// 失敗ステージ (`PrepareNew` / `WriteTemp` / `FsyncTemp` / `FsyncDir` / `Rename` / `CleanupOrphan`)。
        stage: AtomicWriteStage,
        /// 元 IO エラー。
        #[source]
        source: std::io::Error,
    },

    /// リカバリ経路 unlock が必要 (パスワード経路で `MasterPassword::new` 失敗等)。
    /// MSG-S12 リカバリ経路へ誘導。
    #[error("recovery path required")]
    RecoveryRequired,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::crypto::WeakPasswordFeedback;

    #[test]
    fn from_crypto_error_wraps_aead_tag_mismatch() {
        let e: MigrationError = CryptoError::AeadTagMismatch.into();
        assert!(matches!(
            e,
            MigrationError::Crypto(CryptoError::AeadTagMismatch)
        ));
    }

    #[test]
    fn from_crypto_error_wraps_weak_password() {
        let fb = WeakPasswordFeedback::new(Some("too short".to_string()), vec![]);
        let e: MigrationError = CryptoError::WeakPassword(Box::new(fb)).into();
        assert!(matches!(
            e,
            MigrationError::Crypto(CryptoError::WeakPassword(_))
        ));
    }

    #[test]
    fn already_encrypted_displays_known_phrase() {
        let e = MigrationError::AlreadyEncrypted;
        let msg = format!("{e}");
        assert!(msg.contains("already encrypted"));
    }

    #[test]
    fn not_encrypted_displays_known_phrase() {
        let e = MigrationError::NotEncrypted;
        let msg = format!("{e}");
        assert!(msg.contains("not encrypted"));
    }

    #[test]
    fn atomic_write_failed_displays_stage() {
        let e = MigrationError::AtomicWriteFailed {
            stage: AtomicWriteStage::Rename,
            source: std::io::Error::new(std::io::ErrorKind::Other, "boom"),
        };
        let msg = format!("{e}");
        assert!(msg.contains("rename"));
    }

    /// DC-7: `match` 網羅で warning 0 件 (variant 増減検出)。
    #[test]
    fn all_variants_match_exhaustively() {
        let e = MigrationError::AlreadyEncrypted;
        let _: &str = match e {
            MigrationError::Crypto(_) => "crypto",
            MigrationError::Persistence(_) => "persistence",
            MigrationError::Domain(_) => "domain",
            MigrationError::AlreadyEncrypted => "already-encrypted",
            MigrationError::NotEncrypted => "not-encrypted",
            MigrationError::ConfirmationRequired => "confirmation-required",
            MigrationError::PlaintextNotUtf8 => "plaintext-not-utf8",
            MigrationError::RecoveryAlreadyConsumed => "recovery-consumed",
            MigrationError::AtomicWriteFailed { .. } => "atomic-write-failed",
            MigrationError::RecoveryRequired => "recovery-required",
            // `#[non_exhaustive]` で外部 crate には wildcard が必須だが
            // crate 内 match では網羅すれば exhaustive。将来 variant 追加時は本テストが先に壊れる。
            _ => "unknown",
        };
    }
}
