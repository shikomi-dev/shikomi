//! 暗号化 vault フィクスチャ生成ヘルパー。
//!
//! `shikomi-infra` は暗号化 vault の save() を `PersistenceError::UnsupportedYet`
//! で拒否するため、テスト用の暗号化 `vault.db` は rusqlite を直接叩いて生成する。
//! schema 定義は `shikomi-infra/src/persistence/sqlite/schema.rs` と同一構造を
//! 使うが、定数は `pub(crate)` のため本 fixture 内で独立に保持する。
//!
//! 対応 Issue: #20
//! 設計書: `docs/features/cli-vault-commands/test-design/integration.md §5`
//!         `docs/features/cli-vault-commands/test-design/e2e.md §7 暗号化 vault（Fail Fast）`
//!
//! **契約**:
//! - 本 helper が生成する `vault.db` は `load()` 時に `protection_mode = encrypted` を
//!   検出して `PersistenceError::UnsupportedYet` を返す経路の検証専用
//! - 暗号鍵・実データは含まない（ダミーバイト列で CHECK 制約のみ満たす）
//! - git に commit されない（テスト実行時に毎回生成して破棄）

#![allow(dead_code)]

use std::path::Path;

use anyhow::Context;
use rusqlite::{params, Connection};

/// shikomi vault DB の `application_id`（`schema.rs` と同値）。
const APPLICATION_ID: u32 = 0x73_68_6B_6D;

/// `user_version`（Phase 1 では 1 固定）。
const USER_VERSION: u32 = 1;

/// KDF ソルトの要求長（CHECK 制約で 16 バイト固定）。
const KDF_SALT_LEN: usize = 16;

/// Wrapped VEK BLOB の最小長 (Sub-F #44 工程4 Bug-F-005 修正)。
///
/// Sub-A〜D の Boy Scout で `WrappedVek` が `(ciphertext, nonce, tag)` 3 フィールド
/// 分離型に再設計された。infra 層 `deserialize_wrapped_vek` は composite container
/// `nonce(12) ‖ tag(16) ‖ ciphertext(N>=32)` を期待するため最小は **60 バイト**
/// (`12 + 16 + 32`)。DB schema の CHECK 制約 (>= 32) と core の WrappedVek 最小
/// 制約は満たしつつ、infra 層 deserialize もパスする。
///
/// 旧値 32 バイトはちょうど DB CHECK は満たすが infra deserialize で
/// `WrappedVekTooShort` を返し、`e2e_encrypted` / `e2e_edit::*encrypted_vault*` /
/// `it_usecase_*::*_encrypted_vault_returns_encryption_unsupported` を全壊させていた。
const WRAPPED_VEK_MIN_LEN: usize = 12 + 16 + 32;

/// 指定ディレクトリ `dir` の配下に、`protection_mode = encrypted` な
/// 最小限の `vault.db` を生成する。
///
/// 生成後の `vault.db` は `SqliteVaultRepository::load()` から読み込まれ、
/// `load_inner` の Step 12 で暗号化モード検出 → `PersistenceError::UnsupportedYet`
/// を経由して `CliError::EncryptionUnsupported` に写像される。
///
/// # Errors
/// SQLite 接続・DDL 実行・INSERT 失敗時にエラーを返す。
pub fn create_encrypted_vault(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).context("create vault dir")?;
    let vault_db = dir.join("vault.db");
    // 既存ファイルがあれば削除（再実行時の冪等性）
    if vault_db.exists() {
        std::fs::remove_file(&vault_db).context("remove stale vault.db")?;
    }

    // Unix 環境では infra 側が 0700/0600 を強制するため事前に合わせておく。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .context("chmod 0700 dir")?;
    }

    let conn = Connection::open(&vault_db).context("open sqlite")?;

    // PRAGMA: shikomi 識別子を設定
    conn.pragma_update(None, "journal_mode", "DELETE")
        .context("set journal_mode")?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .context("set application_id")?;
    conn.pragma_update(None, "user_version", USER_VERSION)
        .context("set user_version")?;

    // Schema 作成（`schema.rs` CREATE_VAULT_HEADER / CREATE_RECORDS と同一構造）
    conn.execute_batch(CREATE_VAULT_HEADER)
        .context("create vault_header")?;
    conn.execute_batch(CREATE_RECORDS)
        .context("create records")?;

    // 暗号化ヘッダ行を 1 件 INSERT（CHECK 制約を満たす最小値）
    let kdf_salt = vec![0u8; KDF_SALT_LEN];
    let wrapped_vek_by_pw = vec![0u8; WRAPPED_VEK_MIN_LEN];
    let wrapped_vek_by_recovery = vec![0u8; WRAPPED_VEK_MIN_LEN];
    conn.execute(
        INSERT_ENCRYPTED_VAULT_HEADER,
        params![
            "encrypted",
            USER_VERSION,
            "1970-01-01T00:00:00Z",
            kdf_salt,
            wrapped_vek_by_pw,
            wrapped_vek_by_recovery
        ],
    )
    .context("insert encrypted vault_header row")?;

    // 明示的に close してから chmod（Linux で busy file の permission 変更は可能だが確実に）
    drop(conn);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&vault_db, std::fs::Permissions::from_mode(0o600))
            .context("chmod 0600 vault.db")?;
    }

    Ok(())
}

// -------------------------------------------------------------------
// Schema DDL（`shikomi-infra/src/persistence/sqlite/schema.rs` と同一構造）
// -------------------------------------------------------------------

const CREATE_VAULT_HEADER: &str = concat!(
    "CREATE TABLE IF NOT EXISTS vault_header (",
    "  id INTEGER PRIMARY KEY CHECK(id = 1),",
    "  protection_mode TEXT NOT NULL CHECK(protection_mode IN ('plaintext', 'encrypted')),",
    "  vault_version INTEGER NOT NULL CHECK(vault_version >= 1),",
    "  created_at TEXT NOT NULL,",
    "  kdf_salt BLOB,",
    "  wrapped_vek_by_pw BLOB,",
    "  wrapped_vek_by_recovery BLOB,",
    "  CHECK(",
    "    (protection_mode = 'plaintext'",
    "      AND kdf_salt IS NULL",
    "      AND wrapped_vek_by_pw IS NULL",
    "      AND wrapped_vek_by_recovery IS NULL)",
    "    OR",
    "    (protection_mode = 'encrypted'",
    "      AND kdf_salt IS NOT NULL AND length(kdf_salt) = 16",
    "      AND wrapped_vek_by_pw IS NOT NULL AND length(wrapped_vek_by_pw) >= 32",
    "      AND wrapped_vek_by_recovery IS NOT NULL AND length(wrapped_vek_by_recovery) >= 32)",
    "  )",
    ")",
);

const CREATE_RECORDS: &str = concat!(
    "CREATE TABLE IF NOT EXISTS records (",
    "  id TEXT PRIMARY KEY,",
    "  kind TEXT NOT NULL CHECK(kind IN ('text', 'secret')),",
    "  label TEXT NOT NULL CHECK(length(label) > 0),",
    "  payload_variant TEXT NOT NULL CHECK(payload_variant IN ('plaintext', 'encrypted')),",
    "  plaintext_value TEXT,",
    "  nonce BLOB,",
    "  ciphertext BLOB,",
    "  aad BLOB,",
    "  created_at TEXT NOT NULL,",
    "  updated_at TEXT NOT NULL,",
    "  CHECK(",
    "    (payload_variant = 'plaintext'",
    "      AND plaintext_value IS NOT NULL",
    "      AND nonce IS NULL AND ciphertext IS NULL AND aad IS NULL)",
    "    OR",
    "    (payload_variant = 'encrypted'",
    "      AND plaintext_value IS NULL",
    "      AND nonce IS NOT NULL AND length(nonce) = 12",
    "      AND ciphertext IS NOT NULL AND length(ciphertext) > 0",
    "      AND aad IS NOT NULL AND length(aad) = 26)",
    "  )",
    ")",
);

const INSERT_ENCRYPTED_VAULT_HEADER: &str =
    "INSERT INTO vault_header(id, protection_mode, vault_version, created_at, kdf_salt, \
     wrapped_vek_by_pw, wrapped_vek_by_recovery) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)";
