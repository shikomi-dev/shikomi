use rusqlite::Connection;
use shikomi_core::{
    ProtectionMode, Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault,
    VaultHeader, VaultVersion,
};
use time::OffsetDateTime;
use uuid::Uuid;

use super::*;
use crate::persistence::error::{CorruptedReason, PersistenceError};

fn open_in_memory() -> Connection {
    Connection::open_in_memory().unwrap()
}

fn setup_schema(conn: &Connection) {
    conn.execute_batch(concat!(
        "PRAGMA application_id = 1936223085;",
        "PRAGMA user_version = 1;",
        "CREATE TABLE IF NOT EXISTS vault_header (",
        "  id INTEGER PRIMARY KEY CHECK(id = 1),",
        "  protection_mode TEXT NOT NULL CHECK(protection_mode IN ('plaintext', 'encrypted')),",
        "  vault_version INTEGER NOT NULL CHECK(vault_version >= 1),",
        "  created_at TEXT NOT NULL,",
        "  kdf_salt BLOB,",
        "  wrapped_vek_by_pw BLOB,",
        "  wrapped_vek_by_recovery BLOB",
        ");",
        "CREATE TABLE IF NOT EXISTS records (",
        "  id TEXT PRIMARY KEY,",
        "  kind TEXT NOT NULL,",
        "  label TEXT NOT NULL,",
        "  payload_variant TEXT NOT NULL,",
        "  plaintext_value TEXT,",
        "  nonce BLOB,",
        "  ciphertext BLOB,",
        "  aad BLOB,",
        "  created_at TEXT NOT NULL,",
        "  updated_at TEXT NOT NULL",
        ");",
    ))
    .unwrap();
}

fn setup_schema_no_check(conn: &Connection) {
    conn.execute_batch(concat!(
        "CREATE TABLE IF NOT EXISTS vault_header (",
        "  id INTEGER PRIMARY KEY,",
        "  protection_mode TEXT NOT NULL,",
        "  vault_version INTEGER NOT NULL,",
        "  created_at TEXT NOT NULL,",
        "  kdf_salt BLOB,",
        "  wrapped_vek_by_pw BLOB,",
        "  wrapped_vek_by_recovery BLOB",
        ");",
        "CREATE TABLE IF NOT EXISTS records (",
        "  id TEXT PRIMARY KEY,",
        "  kind TEXT NOT NULL,",
        "  label TEXT NOT NULL,",
        "  payload_variant TEXT NOT NULL,",
        "  plaintext_value TEXT,",
        "  nonce BLOB,",
        "  ciphertext BLOB,",
        "  aad BLOB,",
        "  created_at TEXT NOT NULL,",
        "  updated_at TEXT NOT NULL",
        ");",
    ))
    .unwrap();
}

// --- TC-U02: Mapping::vault_header_to_params — 平文モード ---

#[test]
fn tc_u02_vault_header_to_params_plaintext() {
    let header =
        VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap();
    let params = Mapping::vault_header_to_params(&header).unwrap();

    assert_eq!(params.protection_mode, "plaintext");
    assert!(params.kdf_salt.is_none(), "kdf_salt は None のはず");
    assert!(params.wrapped_vek_by_pw.is_none());
    assert!(params.wrapped_vek_by_recovery.is_none());
}

// --- TC-U03: Mapping::row_to_vault_header — 正常系 ---

#[test]
fn tc_u03_row_to_vault_header_plaintext() {
    let conn = open_in_memory();
    setup_schema(&conn);
    conn.execute(
        "INSERT INTO vault_header VALUES (1, 'plaintext', 1, '2026-01-01T00:00:00+00:00', NULL, NULL, NULL)",
        [],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, wrapped_vek_by_recovery FROM vault_header WHERE id = 1")
        .unwrap();
    let header = stmt
        .query_row([], |row| Ok(Mapping::row_to_vault_header(row).unwrap()))
        .unwrap();

    assert_eq!(header.protection_mode(), ProtectionMode::Plaintext);
}

// --- TC-U04: Mapping::row_to_vault_header — UnknownProtectionMode ---

#[test]
fn tc_u04_row_to_vault_header_unknown_mode() {
    let conn = open_in_memory();
    setup_schema_no_check(&conn);
    conn.execute(
        "INSERT INTO vault_header VALUES (1, 'unknown_future_mode', 1, '2026-01-01T00:00:00+00:00', NULL, NULL, NULL)",
        [],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, wrapped_vek_by_recovery FROM vault_header WHERE id = 1")
        .unwrap();
    let result = stmt
        .query_row([], |row| Ok(Mapping::row_to_vault_header(row)))
        .unwrap();

    match result {
        Err(PersistenceError::Corrupted {
            reason: CorruptedReason::UnknownProtectionMode { raw },
            ..
        }) => {
            assert_eq!(raw, "unknown_future_mode");
        }
        other => panic!("UnknownProtectionMode を期待したが {other:?}"),
    }
}

// --- TC-U05: Mapping::row_to_vault_header — InvalidRfc3339 ---

#[test]
fn tc_u05_row_to_vault_header_invalid_rfc3339() {
    let conn = open_in_memory();
    setup_schema_no_check(&conn);
    conn.execute(
        "INSERT INTO vault_header VALUES (1, 'plaintext', 1, 'not-a-date', NULL, NULL, NULL)",
        [],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, wrapped_vek_by_recovery FROM vault_header WHERE id = 1")
        .unwrap();
    let result = stmt
        .query_row([], |row| Ok(Mapping::row_to_vault_header(row)))
        .unwrap();

    match result {
        Err(PersistenceError::Corrupted {
            reason: CorruptedReason::InvalidRfc3339 { column, raw },
            ..
        }) => {
            assert_eq!(column, "created_at");
            assert_eq!(raw, "not-a-date");
        }
        other => panic!("InvalidRfc3339 を期待したが {other:?}"),
    }
}

// --- TC-U06: Mapping::row_to_record — 正常系（plaintext variant）---

#[test]
fn tc_u06_row_to_record_plaintext() {
    let conn = open_in_memory();
    setup_schema(&conn);
    let id = Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO records VALUES (?, 'secret', 'test-label', 'plaintext', 'test value', NULL, NULL, NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        [&id],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, created_at, updated_at FROM records ORDER BY created_at ASC, id ASC")
        .unwrap();
    let record = stmt
        .query_row([], |row| Ok(Mapping::row_to_record(row).unwrap()))
        .unwrap();

    assert_eq!(record.label().as_str(), "test-label");
    assert!(matches!(record.payload(), RecordPayload::Plaintext(_)));
}

// --- TC-U07: Mapping::row_to_record — InvalidUuidString ---

#[test]
fn tc_u07_row_to_record_invalid_uuid() {
    let conn = open_in_memory();
    setup_schema_no_check(&conn);
    conn.execute(
        "INSERT INTO records VALUES ('not-a-uuid', 'secret', 'label', 'plaintext', 'value', NULL, NULL, NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        [],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, created_at, updated_at FROM records ORDER BY created_at ASC, id ASC")
        .unwrap();
    let result = stmt
        .query_row([], |row| Ok(Mapping::row_to_record(row)))
        .unwrap();

    assert!(
        matches!(
            result,
            Err(PersistenceError::Corrupted {
                reason: CorruptedReason::InvalidUuidString { ref raw },
                ..
            }) if raw == "not-a-uuid"
        ),
        "InvalidUuidString を期待したが Err={:?}",
        result.err()
    );
}

// --- TC-U08: Mapping::row_to_record — NullViolation ---

#[test]
fn tc_u08_row_to_record_null_violation() {
    let conn = open_in_memory();
    setup_schema_no_check(&conn);
    let id = Uuid::now_v7().to_string();
    // payload_variant='plaintext' だが plaintext_value=NULL
    conn.execute(
        "INSERT INTO records VALUES (?, 'secret', 'label', 'plaintext', NULL, NULL, NULL, NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        [&id],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, created_at, updated_at FROM records ORDER BY created_at ASC, id ASC")
        .unwrap();
    let result = stmt
        .query_row([], |row| Ok(Mapping::row_to_record(row)))
        .unwrap();

    assert!(
        matches!(
            result,
            Err(PersistenceError::Corrupted {
                reason: CorruptedReason::NullViolation {
                    column: "plaintext_value"
                },
                ..
            })
        ),
        "NullViolation を期待したが {result:?}"
    );
}

// --- TC-U09: Mapping — save→load ラウンドトリップ（平文レコード）---

#[test]
fn tc_u09_roundtrip_plaintext_record() {
    use crate::persistence::sqlite::schema::SchemaSql;
    use time::format_description::well_known::Rfc3339;

    let conn = open_in_memory();
    setup_schema(&conn);

    let header =
        VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap();
    let vault = Vault::new(header.clone());

    let record_id = RecordId::new(Uuid::now_v7()).unwrap();
    let label = RecordLabel::try_new("my-label".to_string()).unwrap();
    let secret = SecretString::from_string("secret-value".to_string());
    let record = Record::new(
        record_id.clone(),
        RecordKind::Secret,
        label.clone(),
        RecordPayload::Plaintext(secret),
        OffsetDateTime::now_utc(),
    );

    // INSERT header
    let hp = Mapping::vault_header_to_params(vault.header()).unwrap();
    conn.execute(
        SchemaSql::INSERT_VAULT_HEADER,
        rusqlite::params![
            hp.protection_mode,
            hp.vault_version,
            hp.created_at_rfc3339,
            hp.kdf_salt,
            hp.wrapped_vek_by_pw,
            hp.wrapped_vek_by_recovery,
        ],
    )
    .unwrap();

    // INSERT record
    let rp = Mapping::record_to_params(&record).unwrap();
    conn.execute(
        SchemaSql::INSERT_RECORD,
        rusqlite::params![
            rp.id,
            rp.kind,
            rp.label,
            rp.payload_variant,
            rp.plaintext_value,
            rp.nonce,
            rp.ciphertext,
            rp.aad_bytes.map(|b| b.to_vec()),
            rp.created_at,
            rp.updated_at,
        ],
    )
    .unwrap();

    // SELECT & verify
    let mut stmt = conn
        .prepare("SELECT id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, created_at, updated_at FROM records ORDER BY created_at ASC, id ASC")
        .unwrap();
    let loaded = stmt
        .query_row([], |row| Ok(Mapping::row_to_record(row).unwrap()))
        .unwrap();

    assert_eq!(loaded.id().to_string(), record.id().to_string());
    assert_eq!(loaded.label().as_str(), "my-label");
    assert_eq!(
        loaded.created_at().format(&Rfc3339).unwrap(),
        record.created_at().format(&Rfc3339).unwrap()
    );
}
