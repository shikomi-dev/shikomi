//! SQLite スキーマ定数。
//!
//! テーブル定義・PRAGMA・DML クエリを一元管理する。

// -------------------------------------------------------------------
// SchemaSql
// -------------------------------------------------------------------

/// SQLite スキーマに関連する全 SQL 定数を提供するゼロサイズ型。
pub(crate) struct SchemaSql;

impl SchemaSql {
    /// shikomi vault DB の `application_id`。
    ///
    /// ASCII: "shkm" = 0x73_68_6B_6D = 1936223085
    pub(crate) const APPLICATION_ID: u32 = 0x73_68_6B_6D;

    /// 初期スキーマバージョン（`user_version` PRAGMA）。
    pub(crate) const USER_VERSION_INITIAL: u32 = 1;

    /// 読み込みに対応する最小 `user_version`。
    pub(crate) const USER_VERSION_SUPPORTED_MIN: u32 = 1;

    /// 読み込みに対応する最大 `user_version`。
    pub(crate) const USER_VERSION_SUPPORTED_MAX: u32 = 1;

    /// `application_id` を取得する PRAGMA クエリ。
    pub(crate) const PRAGMA_APPLICATION_ID_GET: &'static str = "PRAGMA application_id;";

    /// `user_version` を取得する PRAGMA クエリ。
    pub(crate) const PRAGMA_USER_VERSION_GET: &'static str = "PRAGMA user_version;";

    /// ジャーナルモードを DELETE に設定する PRAGMA クエリ。
    pub(crate) const PRAGMA_JOURNAL_MODE: &'static str = "PRAGMA journal_mode = DELETE;";

    /// `application_id` を shikomi の値に設定する PRAGMA クエリ。
    ///
    /// 0x73_68_6B_6D = 1936223085
    pub(crate) const PRAGMA_APPLICATION_ID_SET: &'static str =
        "PRAGMA application_id = 1936223085;";

    /// `user_version` を初期値に設定する PRAGMA クエリ。
    pub(crate) const PRAGMA_USER_VERSION_SET: &'static str = "PRAGMA user_version = 1;";

    /// `vault_header` テーブル作成クエリ。
    ///
    /// CHECK 制約により平文/暗号化の整合性を DB レベルで強制する。
    pub(crate) const CREATE_VAULT_HEADER: &'static str = concat!(
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

    /// `records` テーブル作成クエリ。
    ///
    /// CHECK 制約によりペイロードバリアントとカラムの整合性を DB レベルで強制する。
    pub(crate) const CREATE_RECORDS: &'static str = concat!(
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

    /// `vault_header` の SELECT クエリ（id=1 のみ）。
    pub(crate) const SELECT_VAULT_HEADER: &'static str =
        "SELECT protection_mode, vault_version, created_at, kdf_salt, wrapped_vek_by_pw, \
         wrapped_vek_by_recovery FROM vault_header WHERE id = 1";

    /// `records` の SELECT クエリ（created_at ASC, id ASC でソート）。
    pub(crate) const SELECT_RECORDS_ORDERED: &'static str =
        "SELECT id, kind, label, payload_variant, plaintext_value, nonce, ciphertext, aad, \
         created_at, updated_at FROM records ORDER BY created_at ASC, id ASC";

    /// `vault_header` の INSERT クエリ。
    pub(crate) const INSERT_VAULT_HEADER: &'static str =
        "INSERT INTO vault_header(id, protection_mode, vault_version, created_at, kdf_salt, \
         wrapped_vek_by_pw, wrapped_vek_by_recovery) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)";

    /// `records` の INSERT クエリ。
    pub(crate) const INSERT_RECORD: &'static str =
        "INSERT INTO records(id, kind, label, payload_variant, plaintext_value, nonce, \
         ciphertext, aad, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";
}
