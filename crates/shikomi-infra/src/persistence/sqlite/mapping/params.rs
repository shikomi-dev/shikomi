//! `vault_header` / `records` INSERT 用パラメータ型。

// -------------------------------------------------------------------
// パラメータ型
// -------------------------------------------------------------------

/// `vault_header` INSERT 用パラメータ。
pub(crate) struct HeaderParams {
    /// 保護モード文字列（`"plaintext"` / `"encrypted"`）。
    pub(crate) protection_mode: &'static str,
    /// vault バージョン番号。
    pub(crate) vault_version: u16,
    /// 作成時刻 RFC3339 文字列。
    pub(crate) created_at_rfc3339: String,
    /// KDF ソルト（平文モードは `None`）。
    pub(crate) kdf_salt: Option<Vec<u8>>,
    /// パスワード経路 Wrapped VEK（平文モードは `None`）。
    pub(crate) wrapped_vek_by_pw: Option<Vec<u8>>,
    /// リカバリ経路 Wrapped VEK（平文モードは `None`）。
    pub(crate) wrapped_vek_by_recovery: Option<Vec<u8>>,
}

/// `records` INSERT 用パラメータ。
pub(crate) struct RecordParams<'a> {
    /// レコード ID 文字列。
    pub(crate) id: String,
    /// レコード種別文字列（`"text"` / `"secret"`）。
    pub(crate) kind: &'static str,
    /// ラベル文字列への参照。
    pub(crate) label: &'a str,
    /// ペイロードバリアント（`"plaintext"` / `"encrypted"`）。
    pub(crate) payload_variant: &'static str,
    /// 平文値（平文ペイロード時のみ）。
    pub(crate) plaintext_value: Option<&'a str>,
    /// nonce バイト列（暗号化ペイロード時のみ）。
    pub(crate) nonce: Option<&'a [u8]>,
    /// ciphertext バイト列（暗号化ペイロード時のみ）。
    pub(crate) ciphertext: Option<&'a [u8]>,
    /// AAD の canonical 26 バイト（暗号化ペイロード時のみ）。
    pub(crate) aad_bytes: Option<[u8; 26]>,
    /// 作成時刻 RFC3339 文字列。
    pub(crate) created_at: String,
    /// 更新時刻 RFC3339 文字列。
    pub(crate) updated_at: String,
}
