//! SQLite 永続化形式 ↔ `VaultEncryptedHeader` のエンコード / デコード (Sub-D 新規)。
//!
//! ## 設計判断: 既存スキーマ流用 + composite BLOB 化
//!
//! 既存 `vault_header` テーブルは 3 BLOB カラム (`kdf_salt` / `wrapped_vek_by_pw` /
//! `wrapped_vek_by_recovery`) のみ持ち、`nonce_counter` / `kdf_params` /
//! `header_aead_envelope` 専用カラムは存在しない。
//!
//! Sub-D では DDL マイグレーション (`PRAGMA user_version` bump + ALTER TABLE) を
//! 避けて既存スキーマで完結させるため、**`wrapped_vek_by_pw` 列を composite container BLOB
//! として再解釈** する：
//!
//! ```text
//! wrapped_vek_by_pw_blob = magic(4 "SHKE")
//!                       ‖ container_version(2 BE u16 = 1)
//!                       ‖ nonce_counter(8 BE u64)
//!                       ‖ kdf_params(12 = m||t||p, each 4B BE)
//!                       ‖ wrapped_pw_section_len(2 BE u16) ‖ wrapped_pw_section
//!                       ‖ header_aead_section_len(2 BE u16) ‖ header_aead_section
//!
//! wrapped_pw_section = nonce(12) ‖ tag(16) ‖ ciphertext(N)
//!                      // 既存 WrappedVek の 3 フィールド構造、N >= 32
//!
//! header_aead_section = nonce(12) ‖ tag(16) ‖ ciphertext_len(2 BE u16) ‖ ciphertext(M)
//!                       // M=0 が標準 (改竄検出専用)
//! ```
//!
//! - `kdf_salt` 列: 既存仕様通り 16B 直書き。
//! - `wrapped_vek_by_recovery` 列: 単純な `nonce(12) ‖ tag(16) ‖ ciphertext(N)` 形式。
//! - `wrapped_vek_by_pw` 列: 上記 composite container BLOB。
//!
//! 既存 schema の CHECK `length(wrapped_vek_by_pw) >= 32` は composite container でも
//! 容易に満たす (最小サイズ ~80B 以上)。
//!
//! 設計書 §`SQLite スキーマ` / §`WrappedVek の SQLite 永続化フォーマット` に準拠
//! (DDL 追加の必要性は設計書原文では言及されているが、本 Sub-D 実装段階では
//!  既存スキーマで完結する妥協を採用)。

use shikomi_core::{
    AuthTag, KdfSalt, NonceBytes, NonceCounter, ProtectionMode, Vault, VaultHeader, WrappedVek,
};

use super::error::MigrationError;
use super::header::{HeaderAeadEnvelope, KdfParams, VaultEncryptedHeader};
use crate::persistence::error::{CorruptedReason, PersistenceError};

/// composite container マジックバイト ("SHKE" = Shikomi Header Container Encrypted)。
const HEADER_CONTAINER_MAGIC: &[u8; 4] = b"SHKE";

/// composite container の現行バージョン (v1)。
const HEADER_CONTAINER_VERSION: u16 = 1;

// -------------------------------------------------------------------
// encode: VaultEncryptedHeader → shikomi-core VaultHeader (BLOB 詰込)
// -------------------------------------------------------------------

/// `VaultEncryptedHeader` を shikomi-core `VaultHeader::Encrypted` に詰める。
///
/// `wrapped_vek_by_pw` フィールドに composite container を入れ、追加メタデータを保持する。
pub(super) fn encode_encrypted_header_for_storage(
    header: &VaultEncryptedHeader,
) -> Result<VaultHeader, MigrationError> {
    // composite container BLOB 構築。
    let container = build_container_blob(header);

    // composite container を WrappedVek::new に詰める (length >= 32 を満たす)。
    let composite_wrapped_pw = WrappedVek::new(
        container,
        header.wrapped_vek_by_pw().nonce().clone(),
        header.wrapped_vek_by_pw().tag().clone(),
    )
    .map_err(MigrationError::Domain)?;

    let core_header = VaultHeader::new_encrypted(
        header.version(),
        header.created_at(),
        header.kdf_salt().clone(),
        composite_wrapped_pw,
        header.wrapped_vek_by_recovery().clone(),
    )
    .map_err(MigrationError::Domain)?;

    Ok(core_header)
}

/// composite container BLOB を構築する。
fn build_container_blob(header: &VaultEncryptedHeader) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    // magic + container version
    buf.extend_from_slice(HEADER_CONTAINER_MAGIC);
    buf.extend_from_slice(&HEADER_CONTAINER_VERSION.to_be_bytes());
    // nonce_counter (8B BE u64)
    buf.extend_from_slice(&header.nonce_counter().current().to_be_bytes());
    // kdf_params (12B = m||t||p)
    buf.extend_from_slice(&header.kdf_params().to_canonical_bytes());

    // wrapped_pw_section: nonce(12) ‖ tag(16) ‖ ciphertext(N)
    let wrapped_pw = header.wrapped_vek_by_pw();
    let wrapped_pw_section = serialize_wrapped_section(wrapped_pw);
    write_u16_len_prefixed(&mut buf, &wrapped_pw_section);

    // header_aead_section: nonce(12) ‖ tag(16) ‖ ciphertext_len(2 BE u16) ‖ ciphertext(M)
    let envelope = header.header_aead_envelope();
    let mut env_section = Vec::with_capacity(12 + 16 + 2 + envelope.ciphertext.len());
    env_section.extend_from_slice(envelope.nonce.as_array());
    env_section.extend_from_slice(envelope.tag.as_array());
    let env_ct_len = u16::try_from(envelope.ciphertext.len()).unwrap_or(0);
    env_section.extend_from_slice(&env_ct_len.to_be_bytes());
    env_section.extend_from_slice(&envelope.ciphertext);
    write_u16_len_prefixed(&mut buf, &env_section);

    buf
}

/// `WrappedVek` を `nonce(12) ‖ tag(16) ‖ ciphertext(N)` 形式に直列化する。
fn serialize_wrapped_section(w: &WrappedVek) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + 16 + w.ciphertext().len());
    out.extend_from_slice(w.nonce().as_array());
    out.extend_from_slice(w.tag().as_array());
    out.extend_from_slice(w.ciphertext());
    out
}

/// u16 長さプレフィックス付きで `body` を `buf` に追記する。
fn write_u16_len_prefixed(buf: &mut Vec<u8>, body: &[u8]) {
    let len = u16::try_from(body.len()).unwrap_or(0);
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(body);
}

// -------------------------------------------------------------------
// decode: shikomi-core Vault → VaultEncryptedHeader
// -------------------------------------------------------------------

/// shikomi-core 側 `Vault` から `VaultEncryptedHeader` を復元する。
///
/// `vault.header()` が `VaultHeader::Encrypted` であることを前提とする。
/// `wrapped_vek_by_pw` 列に詰まった composite container を分解して
/// `nonce_counter` / `kdf_params` / `header_aead_envelope` / 真の wrapped_vek_by_pw を取り出す。
pub(super) fn decode_vault_to_encrypted_header(
    vault: &Vault,
) -> Result<VaultEncryptedHeader, MigrationError> {
    if vault.protection_mode() != ProtectionMode::Encrypted {
        return Err(MigrationError::NotEncrypted);
    }
    let core_header = vault.header();
    let version = core_header.version();
    let created_at = core_header.created_at();
    let kdf_salt = core_header
        .kdf_salt()
        .cloned()
        .ok_or(MigrationError::Persistence(corrupted_missing_field(
            "kdf_salt",
        )))?;
    let composite_wrapped_pw =
        core_header
            .wrapped_vek_by_pw()
            .cloned()
            .ok_or(MigrationError::Persistence(corrupted_missing_field(
                "wrapped_vek_by_pw",
            )))?;
    let wrapped_recovery =
        core_header
            .wrapped_vek_by_recovery()
            .cloned()
            .ok_or(MigrationError::Persistence(corrupted_missing_field(
                "wrapped_vek_by_recovery",
            )))?;

    let (nonce_counter, kdf_params, header_envelope, real_wrapped_pw) =
        parse_container_blob(&composite_wrapped_pw)?;

    // WrappedVek (real wrapped_vek_by_pw) を構築。
    let real_wrapped_pw = WrappedVek::new(
        real_wrapped_pw.ciphertext,
        real_wrapped_pw.nonce,
        real_wrapped_pw.tag,
    )
    .map_err(MigrationError::Domain)?;

    Ok(VaultEncryptedHeader::new(
        version,
        created_at,
        kdf_salt,
        real_wrapped_pw,
        wrapped_recovery,
        nonce_counter,
        kdf_params,
        header_envelope,
    ))
}

/// 仮 wrapped_vek_by_pw の構成要素を保持する内部構造体 (composite parse 中継用)。
///
/// `Debug` 派生は `#[cfg(test)]` モジュール内の `assert_eq!` / `unwrap` 等が
/// `Result<ParsedWrappedSection, MigrationError>` の `Err` 経路で要求するため。
/// 本構造体は内部 (`pub` でない) かつバイト断片を保持するだけで秘密値ではない。
#[derive(Debug)]
struct ParsedWrappedSection {
    ciphertext: Vec<u8>,
    nonce: NonceBytes,
    tag: AuthTag,
}

/// composite container を分解して 4 要素を返す。
#[allow(clippy::type_complexity)]
fn parse_container_blob(
    composite: &WrappedVek,
) -> Result<
    (
        NonceCounter,
        KdfParams,
        HeaderAeadEnvelope,
        ParsedWrappedSection,
    ),
    MigrationError,
> {
    let buf = composite.ciphertext();
    let mut cursor = 0usize;

    // magic 4B + container version 2B
    let magic = read_slice(buf, &mut cursor, 4)?;
    if magic != HEADER_CONTAINER_MAGIC {
        return Err(MigrationError::Persistence(corrupted_with_detail(
            "wrapped_vek_by_pw composite magic mismatch",
        )));
    }
    let version_bytes = read_array_2(buf, &mut cursor)?;
    let container_version = u16::from_be_bytes(version_bytes);
    if container_version != HEADER_CONTAINER_VERSION {
        return Err(MigrationError::Persistence(corrupted_with_detail(
            "wrapped_vek_by_pw composite container version unsupported",
        )));
    }

    // nonce_counter 8B
    let counter_bytes = read_array_8(buf, &mut cursor)?;
    let nonce_counter = NonceCounter::resume(u64::from_be_bytes(counter_bytes));

    // kdf_params 12B
    let kdf_bytes = read_array_12(buf, &mut cursor)?;
    let kdf_params = KdfParams::from_canonical_bytes(kdf_bytes);

    // wrapped_pw_section: u16 len + body
    let wrapped_pw_section_len = read_array_2(buf, &mut cursor)?;
    let wrapped_pw_section_len = u16::from_be_bytes(wrapped_pw_section_len) as usize;
    let wrapped_pw_section = read_slice(buf, &mut cursor, wrapped_pw_section_len)?;
    let real_wrapped_pw = parse_wrapped_section(wrapped_pw_section)?;

    // header_aead_section: u16 len + body
    let env_section_len = read_array_2(buf, &mut cursor)?;
    let env_section_len = u16::from_be_bytes(env_section_len) as usize;
    let env_section = read_slice(buf, &mut cursor, env_section_len)?;
    let header_envelope = parse_header_aead_section(env_section)?;

    Ok((nonce_counter, kdf_params, header_envelope, real_wrapped_pw))
}

/// `nonce(12) ‖ tag(16) ‖ ciphertext(N)` 形式の section を分解する。
fn parse_wrapped_section(section: &[u8]) -> Result<ParsedWrappedSection, MigrationError> {
    if section.len() < 12 + 16 {
        return Err(MigrationError::Persistence(corrupted_with_detail(
            "wrapped section too short",
        )));
    }
    let mut nonce_arr = [0u8; 12];
    nonce_arr.copy_from_slice(&section[0..12]);
    let mut tag_arr = [0u8; 16];
    tag_arr.copy_from_slice(&section[12..28]);
    let ciphertext = section[28..].to_vec();
    Ok(ParsedWrappedSection {
        ciphertext,
        nonce: NonceBytes::from_random(nonce_arr),
        tag: AuthTag::from_array(tag_arr),
    })
}

/// header AEAD section を分解する。
fn parse_header_aead_section(section: &[u8]) -> Result<HeaderAeadEnvelope, MigrationError> {
    if section.len() < 12 + 16 + 2 {
        return Err(MigrationError::Persistence(corrupted_with_detail(
            "header AEAD section too short",
        )));
    }
    let mut nonce_arr = [0u8; 12];
    nonce_arr.copy_from_slice(&section[0..12]);
    let mut tag_arr = [0u8; 16];
    tag_arr.copy_from_slice(&section[12..28]);
    let mut ct_len_arr = [0u8; 2];
    ct_len_arr.copy_from_slice(&section[28..30]);
    let ct_len = u16::from_be_bytes(ct_len_arr) as usize;
    if section.len() < 30 + ct_len {
        return Err(MigrationError::Persistence(corrupted_with_detail(
            "header AEAD section ciphertext truncated",
        )));
    }
    let ciphertext = section[30..30 + ct_len].to_vec();
    Ok(HeaderAeadEnvelope::new(
        ciphertext,
        NonceBytes::from_random(nonce_arr),
        AuthTag::from_array(tag_arr),
    ))
}

// -------------------------------------------------------------------
// バイト列読出ヘルパ
// -------------------------------------------------------------------

fn read_slice<'a>(buf: &'a [u8], cursor: &mut usize, n: usize) -> Result<&'a [u8], MigrationError> {
    if *cursor + n > buf.len() {
        return Err(MigrationError::Persistence(corrupted_with_detail(
            "container BLOB truncated",
        )));
    }
    let s = &buf[*cursor..*cursor + n];
    *cursor += n;
    Ok(s)
}

fn read_array_2(buf: &[u8], cursor: &mut usize) -> Result<[u8; 2], MigrationError> {
    let s = read_slice(buf, cursor, 2)?;
    let mut a = [0u8; 2];
    a.copy_from_slice(s);
    Ok(a)
}

fn read_array_8(buf: &[u8], cursor: &mut usize) -> Result<[u8; 8], MigrationError> {
    let s = read_slice(buf, cursor, 8)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(s);
    Ok(a)
}

fn read_array_12(buf: &[u8], cursor: &mut usize) -> Result<[u8; 12], MigrationError> {
    let s = read_slice(buf, cursor, 12)?;
    let mut a = [0u8; 12];
    a.copy_from_slice(s);
    Ok(a)
}

fn corrupted_missing_field(field: &'static str) -> PersistenceError {
    PersistenceError::Corrupted {
        table: "vault_header",
        row_key: Some("1".to_string()),
        reason: CorruptedReason::NullViolation { column: field },
        source: None,
    }
}

fn corrupted_with_detail(detail: &str) -> PersistenceError {
    PersistenceError::Corrupted {
        table: "vault_header",
        row_key: Some("1".to_string()),
        reason: CorruptedReason::InvalidRowCombination {
            detail: detail.to_string(),
        },
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{KdfSalt, NonceBytes, VaultVersion};
    use time::OffsetDateTime;

    fn dummy_kdf_salt() -> KdfSalt {
        KdfSalt::from_array([0xABu8; 16])
    }

    fn dummy_wrapped_vek_with_marker(marker: u8) -> WrappedVek {
        WrappedVek::new(
            vec![marker; 32],
            NonceBytes::from_random([marker; 12]),
            AuthTag::from_array([marker; 16]),
        )
        .unwrap()
    }

    fn dummy_envelope() -> HeaderAeadEnvelope {
        HeaderAeadEnvelope::new(
            Vec::new(),
            NonceBytes::from_random([0xCDu8; 12]),
            AuthTag::from_array([0xEFu8; 16]),
        )
    }

    fn make_header() -> VaultEncryptedHeader {
        VaultEncryptedHeader::new(
            VaultVersion::CURRENT,
            OffsetDateTime::UNIX_EPOCH,
            dummy_kdf_salt(),
            dummy_wrapped_vek_with_marker(0x11),
            dummy_wrapped_vek_with_marker(0x22),
            NonceCounter::resume(42),
            KdfParams::FROZEN,
            dummy_envelope(),
        )
    }

    /// composite container エンコード → デコード往復で全フィールド bit-exact 復元。
    #[test]
    fn encode_decode_round_trip_preserves_all_fields() {
        let original = make_header();
        let core_header = encode_encrypted_header_for_storage(&original).expect("encode ok");
        let vault = Vault::new(core_header);
        let restored = decode_vault_to_encrypted_header(&vault).expect("decode ok");

        assert_eq!(restored.version(), original.version());
        assert_eq!(restored.created_at(), original.created_at());
        assert_eq!(
            restored.kdf_salt().as_array(),
            original.kdf_salt().as_array()
        );
        assert_eq!(restored.nonce_counter().current(), 42);
        assert_eq!(restored.kdf_params(), KdfParams::FROZEN);
        assert_eq!(
            restored.wrapped_vek_by_pw().ciphertext(),
            original.wrapped_vek_by_pw().ciphertext()
        );
        assert_eq!(
            restored.wrapped_vek_by_pw().nonce().as_array(),
            original.wrapped_vek_by_pw().nonce().as_array()
        );
        assert_eq!(
            restored.wrapped_vek_by_pw().tag().as_array(),
            original.wrapped_vek_by_pw().tag().as_array()
        );
        assert_eq!(
            restored.wrapped_vek_by_recovery().ciphertext(),
            original.wrapped_vek_by_recovery().ciphertext()
        );
        assert_eq!(
            restored.header_aead_envelope().nonce.as_array(),
            original.header_aead_envelope().nonce.as_array()
        );
    }

    /// container magic 不一致は corrupted エラー。
    #[test]
    fn parse_container_with_wrong_magic_returns_corrupted() {
        // 空の WrappedVek (32B 0 埋め) を渡す → magic 不一致。
        let bad = WrappedVek::new(
            vec![0u8; 32],
            NonceBytes::from_random([0u8; 12]),
            AuthTag::from_array([0u8; 16]),
        )
        .unwrap();
        let err = parse_container_blob(&bad).unwrap_err();
        assert!(matches!(err, MigrationError::Persistence(_)));
    }

    /// 平文 vault に対する decode は NotEncrypted エラー。
    #[test]
    fn decode_plaintext_vault_returns_not_encrypted() {
        let header =
            VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::UNIX_EPOCH).unwrap();
        let vault = Vault::new(header);
        let err = decode_vault_to_encrypted_header(&vault).unwrap_err();
        assert!(matches!(err, MigrationError::NotEncrypted));
    }
}
