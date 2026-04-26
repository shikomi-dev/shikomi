//! AEAD アダプタ層 (AES-256-GCM)。
//!
//! Sub-C (#41) で新規追加。`shikomi-core::AeadKey` クロージャインジェクション
//! 経由で `Vek` / `Kek<_>` / `HeaderAeadKey` の鍵バイトを借り受け、
//! `aes-gcm` crate (RustCrypto) の `Aes256Gcm` で `encrypt_in_place_detached` /
//! `decrypt_in_place_detached` の **tag 分離 API** を呼び出す。
//!
//! AEAD タグ検証成功時のみ `verify_aead_decrypt_to_plaintext` 経由で
//! `Verified<Plaintext>` を構築し、`Plaintext::new_within_module` の
//! `pub(in crate::crypto::verified)` 限定可視性を破らない。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/nonce-and-aead.md`
//!         §`AesGcmAeadAdapter`

pub mod aes_gcm;
mod kat;

pub use aes_gcm::AesGcmAeadAdapter;
