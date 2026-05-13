//! レコードペイロード（平文・暗号化）。

use crate::error::DomainError;
use crate::secret::SecretString;
use crate::vault::crypto_data::{Aad, CipherText};
use crate::vault::nonce::NonceBytes;
use crate::vault::protection_mode::ProtectionMode;

// -------------------------------------------------------------------
// RecordPayloadEncrypted
// -------------------------------------------------------------------

/// 暗号化バリアントのペイロード内部データ。
#[derive(Debug, Clone)]
pub struct RecordPayloadEncrypted {
    nonce: NonceBytes,
    ciphertext: CipherText,
    aad: Aad,
}

impl RecordPayloadEncrypted {
    /// 暗号化ペイロードを構築する。
    ///
    /// `nonce` / `ciphertext` / `aad` はそれぞれの `try_new` / `new` で検証済みの型を渡す。
    ///
    /// # Errors
    /// 現時点では `nonce` / `ciphertext` の検証は各型の `try_new` で行うため、
    /// この関数自体は `Ok` を返す。将来の追加検証のために `Result` を維持する。
    pub fn new(nonce: NonceBytes, ciphertext: CipherText, aad: Aad) -> Result<Self, DomainError> {
        Ok(Self {
            nonce,
            ciphertext,
            aad,
        })
    }

    /// nonce への参照を返す。
    #[must_use]
    pub fn nonce(&self) -> &NonceBytes {
        &self.nonce
    }

    /// ciphertext への参照を返す。
    #[must_use]
    pub fn ciphertext(&self) -> &CipherText {
        &self.ciphertext
    }

    /// AAD への参照を返す。
    #[must_use]
    pub fn aad(&self) -> &Aad {
        &self.aad
    }
}

// -------------------------------------------------------------------
// RecordPayload
// -------------------------------------------------------------------

/// レコードのペイロード。平文と暗号化を enum バリアントで排他する。
#[derive(Debug, Clone)]
pub enum RecordPayload {
    /// 平文ペイロード。SecretString に保持する。
    Plaintext(SecretString),
    /// 暗号化ペイロード（nonce + ciphertext + AAD）。
    Encrypted(RecordPayloadEncrypted),
}

impl RecordPayload {
    /// このペイロードが対応する `ProtectionMode` を返す。
    #[must_use]
    pub fn variant_mode(&self) -> ProtectionMode {
        match self {
            Self::Plaintext(_) => ProtectionMode::Plaintext,
            Self::Encrypted(_) => ProtectionMode::Encrypted,
        }
    }
}
