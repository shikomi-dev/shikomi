//! GUI 統一エラー型。
//!
//! `GUIError` は全 Tauri Commands の統一エラー型。`serde::Serialize` を実装し、
//! SolidJS 側で JSON として受け取れる。
//!
//! ## JSON フィールド仕様
//!
//! | フィールド | 全 variant | 説明 |
//! |---|---|---|
//! | `kind` | 常に存在 | Sub-C が `switch` する最上位判別子 |
//! | `message` | 常に存在 | デバッグ・ログ用英語技術情報。**ユーザーに直接表示禁止** |
//! | `ipc_code` | `kind == "ipc_error"` のみ | daemon エラー種別の安定識別子（§2.3）|
//! | `wait_secs` | `ipc_code == "backoff_active"` のみ | 次回試行可能までの待機秒数 |
//! | `crypto_reason` | `ipc_code == "crypto"` のみ | 暗号エラー詳細識別子（kebab-case 固定文言: `wrong-password` / `weak-password` / `nonce-limit-exceeded` 等）。Sub-C はこの値で UI 分岐する |
//! | `hotkey_conflict_entry` | `ipc_code == "hotkey_conflict"` のみ | 競合している既存エントリ名の文字列。Sub-C は競合エントリ名を UI 表示する（R1-GUI-08）。`message` パースへの依存禁止 |
//!
//! | `invalid_input_code` | `kind == "invalid_input"` のみ | バリデーション失敗種別の安定識別子。Sub-C はこの値で UI 分岐する（`message` パース禁止）|
//!
//! Sub-C は `kind` → `ipc_code` / `invalid_input_code` の順で分岐し、`message` は開発ツール専用。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md §2.2
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §2

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use shikomi_core::ipc::IpcErrorCode;
use thiserror::Error;

// ---------------------------------------------------------------------------
// GUIError
// ---------------------------------------------------------------------------

/// Tauri Commands の統一エラー型。
///
/// `Serialize` 実装でモジュール doc の JSON フィールド仕様に従い写像する。
/// `Ipc` variant のみ `ipc_code` を持ち、さらに `BackoffActive` は `wait_secs`、
/// `Crypto` は `crypto_reason`、`HotkeyConflict` は `hotkey_conflict_entry` を追加フィールドとして持つ。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GUIError {
    /// UDS / Named Pipe が存在しない（daemon 未起動）。
    #[error("daemon is not running")]
    DaemonNotRunning,

    /// 接続後の IO エラー（切断含む）。
    ///
    /// `message` には `io::Error::kind().to_string()` のみを使用し、
    /// OS 内部情報（ソケットパス・FD 番号等）を含めない（OWASP A04）。
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Handshake バージョン不一致。
    #[error("protocol version mismatch (server={server}, client={client})")]
    ProtocolVersionMismatch {
        /// daemon 側のバージョン文字列。
        server: String,
        /// クライアント側のバージョン文字列。
        client: String,
    },

    /// daemon から返却された `IpcErrorCode` の透過伝搬。
    #[error("ipc error: {0}")]
    Ipc(IpcErrorCode),

    /// `MessagePack` シリアライズ失敗。
    #[error("encode error: {0}")]
    Encode(String),

    /// `MessagePack` デシリアライズ失敗。
    #[error("decode error: {0}")]
    Decode(String),

    /// 予期しない `IpcResponse` variant。
    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),

    /// Rust 側 input validation 失敗（R1-GUI-19）。
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// `AppState` が `None`（daemon 未接続）。
    #[error("not connected to daemon")]
    NotConnected,
}

// ---------------------------------------------------------------------------
// Serialize 実装
// ---------------------------------------------------------------------------

impl Serialize for GUIError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // Ipc variant: kind + ipc_code + message。
            // variant ごとの追加フィールド（§2.3）:
            //   BackoffActive → wait_secs
            //   Crypto        → crypto_reason
            //   HotkeyConflict→ hotkey_conflict_entry
            // Sub-C は ipc_code で daemon エラー種別を switch する（detailed-design.md §2.3）。
            Self::Ipc(code) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("kind", "ipc_error")?;
                map.serialize_entry("ipc_code", ipc_code_key(code))?;
                if let IpcErrorCode::BackoffActive { wait_secs } = code {
                    map.serialize_entry("wait_secs", wait_secs)?;
                }
                if let IpcErrorCode::Crypto { reason } = code {
                    map.serialize_entry("crypto_reason", reason)?;
                }
                if let IpcErrorCode::HotkeyConflict { reason } = code {
                    map.serialize_entry("hotkey_conflict_entry", reason)?;
                }
                map.serialize_entry("message", &code.to_string())?;
                map.end()
            }
            // InvalidInput variant: kind + invalid_input_code + message の 3 フィールド。
            // Sub-C は invalid_input_code で分岐し、message パースに依存しない（§2.2）。
            Self::InvalidInput(msg) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("kind", "invalid_input")?;
                map.serialize_entry("invalid_input_code", invalid_input_code_key(msg))?;
                map.serialize_entry("message", msg)?;
                map.end()
            }
            // 他の variant: kind + message の 2 フィールド。
            other => {
                let mut map = serializer.serialize_map(Some(2))?;
                let (kind, message): (&str, String) = match other {
                    Self::DaemonNotRunning => {
                        ("daemon_not_running", "daemon is not running".to_owned())
                    }
                    Self::ConnectionFailed(msg) => ("connection_failed", msg.clone()),
                    Self::ProtocolVersionMismatch { server, client } => (
                        "protocol_version_mismatch",
                        format!("server={server}, client={client}"),
                    ),
                    Self::Encode(msg) => ("encode_error", msg.clone()),
                    Self::Decode(msg) => ("decode_error", msg.clone()),
                    Self::UnexpectedResponse(msg) => ("unexpected_response", msg.clone()),
                    Self::NotConnected => ("not_connected", "not connected to daemon".to_owned()),
                    Self::Ipc(_) | Self::InvalidInput(_) => {
                        unreachable!("handled in dedicated arms")
                    }
                };
                map.serialize_entry("kind", kind)?;
                map.serialize_entry("message", &message)?;
                map.end()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ipc_code_key: IpcErrorCode → snake_case 安定識別子
// ---------------------------------------------------------------------------

/// `InvalidInput` の message 文字列を Sub-C が `switch` する安定識別子へ写像する（§2.2）。
///
/// 各エントリポイントの `InvalidInput` 生成箇所（entries.rs, vault.rs, hotkey.rs）の
/// 固定文言を網羅する。新たな `InvalidInput` を追加した場合はここに必ず追記すること。
fn invalid_input_code_key(msg: &str) -> &'static str {
    match msg {
        "label must not be empty" => "label_empty",
        "value must not be empty" => "value_empty",
        "invalid record id format" => "id_invalid",
        "master password must not be empty" => "password_empty",
        "decrypt confirmation required" => "confirmation_required",
        "invalid hotkey format" => "hotkey_invalid",
        // entries.rs: RecordLabel::try_new のエラーを凍結文言で包んだもの（§2.2）
        "invalid label format" => "label_invalid",
        _ => "unknown",
    }
}

/// `IpcErrorCode` variant を Sub-C が `switch` する安定 snake_case 識別子へ写像する（§2.3）。
///
/// `IpcErrorCode` は `#[non_exhaustive]` のため将来バリアント追加時の
/// フォールバック `"unknown"` を保持する。Sub-C 側は `"unknown"` を汎用エラーとして処理せよ。
fn ipc_code_key(code: &IpcErrorCode) -> &'static str {
    match code {
        IpcErrorCode::EncryptionUnsupported => "encryption_unsupported",
        IpcErrorCode::NotFound { .. } => "not_found",
        IpcErrorCode::InvalidLabel { .. } => "invalid_label",
        IpcErrorCode::Persistence { .. } => "persistence",
        IpcErrorCode::Domain { .. } => "domain",
        IpcErrorCode::Internal { .. } => "internal",
        IpcErrorCode::VaultLocked => "vault_locked",
        IpcErrorCode::BackoffActive { .. } => "backoff_active",
        IpcErrorCode::RecoveryRequired => "recovery_required",
        IpcErrorCode::ProtocolDowngrade => "protocol_downgrade",
        IpcErrorCode::Crypto { .. } => "crypto",
        IpcErrorCode::HotkeyConflict { .. } => "hotkey_conflict",
        IpcErrorCode::HotkeyParseError { .. } => "hotkey_parse_error",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::GUIError;
    use shikomi_core::ipc::IpcErrorCode;

    // TC-GUI-IPC-UT10
    #[test]
    fn ut10_daemon_not_running_kind() {
        let e = GUIError::DaemonNotRunning;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "daemon_not_running");
        assert!(!v["message"].as_str().unwrap_or("").is_empty());
    }

    // TC-GUI-IPC-UT11
    #[test]
    fn ut11_not_connected_kind() {
        let e = GUIError::NotConnected;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "not_connected");
        assert!(!v["message"].as_str().unwrap_or("").is_empty());
    }

    // TC-GUI-IPC-UT12
    #[test]
    fn ut12_protocol_version_mismatch_kind_and_message() {
        let e = GUIError::ProtocolVersionMismatch {
            server: "v1".to_owned(),
            client: "v2".to_owned(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "protocol_version_mismatch");
        let msg = v["message"].as_str().unwrap();
        assert!(
            msg.contains("v1"),
            "message should contain server version: {msg}"
        );
        assert!(
            msg.contains("v2"),
            "message should contain client version: {msg}"
        );
    }

    // TC-GUI-IPC-UT13 — GUIError::Ipc(VaultLocked): kind/ipc_code/message 全フィールド検証
    //
    // ペガサス指摘 Option A（§2.3 ipc_code 追加）対応: Sub-C は ipc_code で分岐する。
    // ipc_code フィールドの存在と値を検証する（旧実装では kind のみ検証で ipc_code 欠落）。
    #[test]
    fn ut13_ipc_vault_locked_kind_ipc_code_and_message() {
        let e = GUIError::Ipc(IpcErrorCode::VaultLocked);
        let v = serde_json::to_value(&e).unwrap();
        // kind: "ipc_error"（全 IpcErrorCode 共通）
        assert_eq!(v["kind"], "ipc_error", "kind must be ipc_error: {v}");
        // ipc_code: "vault_locked"（Sub-C が UI 分岐に使う安定識別子）
        assert_eq!(
            v["ipc_code"], "vault_locked",
            "ipc_code must be vault_locked: {v}"
        );
        // message: VaultLocked の Display 文字列（デバッグ専用）
        let msg = v["message"].as_str().unwrap();
        let expected = IpcErrorCode::VaultLocked.to_string();
        assert_eq!(
            msg, expected,
            "message must match IpcErrorCode::VaultLocked Display"
        );
        // ipc_code == "vault_locked" のとき wait_secs は存在しない
        assert!(
            v.get("wait_secs").is_none() || v["wait_secs"].is_null(),
            "wait_secs must not be present for VaultLocked: {v}"
        );
    }

    // TC-GUI-IPC-UT13b — GUIError::Ipc(BackoffActive): wait_secs フィールド検証
    //
    // BackoffActive のみ wait_secs フィールドが追加される（§2.3 特例）。
    // Sub-C が待機カウントダウンを UI 表示するために必要。
    #[test]
    fn ut13b_ipc_backoff_active_has_wait_secs() {
        let e = GUIError::Ipc(IpcErrorCode::BackoffActive { wait_secs: 42 });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "ipc_error", "kind must be ipc_error: {v}");
        assert_eq!(
            v["ipc_code"], "backoff_active",
            "ipc_code must be backoff_active: {v}"
        );
        // wait_secs フィールドが数値として存在する
        assert_eq!(
            v["wait_secs"], 42,
            "wait_secs must be 42 for BackoffActive{{ wait_secs: 42 }}: {v}"
        );
        // message にも wait_secs が含まれる（Display 準拠）
        let msg = v["message"].as_str().unwrap();
        assert!(
            msg.contains("42"),
            "message must contain wait_secs value '42': {msg}"
        );
    }

    // TC-GUI-IPC-UT13d — GUIError::Ipc(Crypto): crypto_reason フィールド検証
    //
    // Crypto variant は ipc_code == "crypto" かつ crypto_reason が
    // kebab-case 固定文言（"wrong-password" 等）として存在する（§2.3 特例）。
    // Sub-C は crypto_reason で UI 分岐する（パスワード不一致モーダル / 再暗号化必須警告 等）。
    #[test]
    fn ut13d_ipc_crypto_has_crypto_reason() {
        let e = GUIError::Ipc(IpcErrorCode::Crypto {
            reason: "wrong-password".to_owned(),
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "ipc_error", "kind must be ipc_error: {v}");
        assert_eq!(v["ipc_code"], "crypto", "ipc_code must be crypto: {v}");
        // crypto_reason フィールドが設計書 §2.3 の安定識別子として存在する
        assert_eq!(
            v["crypto_reason"], "wrong-password",
            "crypto_reason must be 'wrong-password': {v}"
        );
        // message はデバッグ専用（Display 準拠）。Sub-C は crypto_reason を使う
        let msg = v["message"].as_str().unwrap();
        assert!(!msg.is_empty(), "message must not be empty: {v}");
        // BackoffActive 固有の wait_secs は Crypto には存在しない
        assert!(
            v.get("wait_secs").is_none() || v["wait_secs"].is_null(),
            "wait_secs must not be present for Crypto: {v}"
        );
    }

    // TC-GUI-IPC-UT13c — GUIError::Ipc(HotkeyConflict): ipc_code + hotkey_conflict_entry 検証
    //
    // ペガサス指摘対応: HotkeyConflict は Sub-C が競合エントリ名を UI 表示するために
    // hotkey_conflict_entry フィールドが必要（R1-GUI-08）。message パースへの依存禁止（§2.3）。
    // ipc_code のみだった旧実装から hotkey_conflict_entry フィールド検証を追加。
    #[test]
    fn ut13c_ipc_hotkey_conflict_ipc_code_and_entry() {
        let e = GUIError::Ipc(IpcErrorCode::HotkeyConflict {
            reason: "slot occupied".to_owned(),
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "ipc_error", "kind must be ipc_error: {v}");
        assert_eq!(
            v["ipc_code"], "hotkey_conflict",
            "ipc_code must be hotkey_conflict: {v}"
        );
        // hotkey_conflict_entry フィールドが競合エントリ名を持つこと（R1-GUI-08）
        assert_eq!(
            v["hotkey_conflict_entry"], "slot occupied",
            "hotkey_conflict_entry must be 'slot occupied': {v}"
        );
        // Crypto 固有の crypto_reason は HotkeyConflict には存在しない
        assert!(
            v.get("crypto_reason").is_none() || v["crypto_reason"].is_null(),
            "crypto_reason must not be present for HotkeyConflict: {v}"
        );
        // BackoffActive 固有の wait_secs も存在しない
        assert!(
            v.get("wait_secs").is_none() || v["wait_secs"].is_null(),
            "wait_secs must not be present for HotkeyConflict: {v}"
        );
    }

    // TC-GUI-IPC-UT13d(2) — crypto_reason: weak-password
    //
    // §2.3 凍結契約に列挙された crypto_reason 全値のうち "weak-password" を検証。
    #[test]
    fn ut13d_crypto_reason_weak_password() {
        let e = GUIError::Ipc(IpcErrorCode::Crypto {
            reason: "weak-password".to_owned(),
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["ipc_code"], "crypto");
        assert_eq!(v["crypto_reason"], "weak-password");
    }

    // TC-GUI-IPC-UT13d(3) — crypto_reason: nonce-limit-exceeded
    //
    // §2.3 凍結契約に列挙された crypto_reason 全値のうち "nonce-limit-exceeded" を検証。
    // この値は Sub-C が「再暗号化必須」警告 UI を表示するためのトリガーになる。
    #[test]
    fn ut13d_crypto_reason_nonce_limit_exceeded() {
        let e = GUIError::Ipc(IpcErrorCode::Crypto {
            reason: "nonce-limit-exceeded".to_owned(),
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["ipc_code"], "crypto");
        assert_eq!(v["crypto_reason"], "nonce-limit-exceeded");
    }

    // TC-GUI-IPC-UT14 — GUIError::InvalidInput: kind / invalid_input_code / message 全フィールド検証
    //
    // ペテルギウス指摘対応（§2.2 invalid_input_code 追加）:
    // invalid_input の message パースを廃止し、invalid_input_code 安定識別子で分岐する。
    // Sub-C は invalid_input_code で UI テキストを決定し、message は表示しない。
    #[test]
    fn ut14_invalid_input_kind_invalid_input_code_and_message() {
        // label_empty マッピング検証
        let e = GUIError::InvalidInput("label must not be empty".to_owned());
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "invalid_input");
        assert_eq!(
            v["invalid_input_code"], "label_empty",
            "invalid_input_code must be label_empty: {v}"
        );
        assert!(!v["message"].as_str().unwrap_or("").is_empty());

        // value_empty マッピング検証（旧実装では "empty" で label_empty に誤マッチ）
        let e2 = GUIError::InvalidInput("value must not be empty".to_owned());
        let v2 = serde_json::to_value(&e2).unwrap();
        assert_eq!(
            v2["invalid_input_code"], "value_empty",
            "invalid_input_code must be value_empty (not label_empty): {v2}"
        );

        // password_empty マッピング検証（旧実装では "empty" で label_empty に誤マッチ）
        let e3 = GUIError::InvalidInput("master password must not be empty".to_owned());
        let v3 = serde_json::to_value(&e3).unwrap();
        assert_eq!(
            v3["invalid_input_code"], "password_empty",
            "invalid_input_code must be password_empty: {v3}"
        );
    }

    // TC-GUI-IPC-UT14b — invalid_input_code 全 7 値の網羅テスト（key テーブル凍結契約）
    #[test]
    fn ut14b_invalid_input_code_exhaustive() {
        let cases = [
            ("label must not be empty", "label_empty"),
            ("value must not be empty", "value_empty"),
            ("invalid record id format", "id_invalid"),
            ("master password must not be empty", "password_empty"),
            ("decrypt confirmation required", "confirmation_required"),
            ("invalid hotkey format", "hotkey_invalid"),
            ("invalid label format", "label_invalid"),
            ("unknown message", "unknown"),
        ];
        for (msg, expected_code) in cases {
            let e = GUIError::InvalidInput(msg.to_owned());
            let v = serde_json::to_value(&e).unwrap();
            assert_eq!(
                v["kind"], "invalid_input",
                "kind must be invalid_input for '{msg}': {v}"
            );
            assert_eq!(
                v["invalid_input_code"], expected_code,
                "invalid_input_code must be '{expected_code}' for msg='{msg}': {v}"
            );
        }
    }

    // TC-GUI-IPC-UT16 — 全 InvalidInput 生成箇所の実文言 → invalid_input_code 網羅テスト
    //
    // ペテルギウス指摘対応（Sub-B UT15 同型）: `ipc_code_key()` と §2.3 凍結契約の
    // 完全一致を CI で保証したように、`invalid_input_code_key()` と実際の
    // `InvalidInput` 生成箇所（entries.rs / vault.rs / hotkey.rs）の固定文言を
    // 構造的に照合する。
    //
    // このテストが Fail → 実文言の変更または `invalid_input_code_key` の更新漏れ。
    // 「unknown」にフォールバックしているエントリが出た場合は §2.2 の凍結契約更新が必要。
    //
    // 実文言の出所（grep ソース）:
    //   entries.rs L92:  "label must not be empty"
    //   entries.rs L95:  "value must not be empty"
    //   entries.rs L100: "invalid label format"  ← RecordLabel::try_new error path
    //   entries.rs L144: "invalid record id format"
    //   entries.rs L148: "invalid label format"  ← update_entry RecordLabel path
    //   entries.rs L189: "invalid record id format"
    //   vault.rs   L87:  "master password must not be empty"
    //   vault.rs   L140: "master password must not be empty"
    //   vault.rs   L145: "decrypt confirmation required"
    //   vault.rs   L189: "master password must not be empty"
    //   hotkey.rs  L55:  "invalid record id format"
    //   hotkey.rs  L92:  "invalid record id format"
    //   hotkey.rs  L131: "invalid hotkey format"   ← validate_hotkey_combo
    #[test]
    fn ut16_all_invalid_input_sources_map_to_known_code() {
        // (ファイル:行, 実文言, 期待 invalid_input_code) — "unknown" は許容しない
        let sources: &[(&str, &str, &str)] = &[
            ("entries.rs:92", "label must not be empty", "label_empty"),
            ("entries.rs:95", "value must not be empty", "value_empty"),
            ("entries.rs:100", "invalid label format", "label_invalid"),
            ("entries.rs:144", "invalid record id format", "id_invalid"),
            ("entries.rs:148", "invalid label format", "label_invalid"),
            ("entries.rs:189", "invalid record id format", "id_invalid"),
            (
                "vault.rs:87",
                "master password must not be empty",
                "password_empty",
            ),
            (
                "vault.rs:140",
                "master password must not be empty",
                "password_empty",
            ),
            (
                "vault.rs:145",
                "decrypt confirmation required",
                "confirmation_required",
            ),
            (
                "vault.rs:189",
                "master password must not be empty",
                "password_empty",
            ),
            ("hotkey.rs:55", "invalid record id format", "id_invalid"),
            ("hotkey.rs:92", "invalid record id format", "id_invalid"),
            ("hotkey.rs:131", "invalid hotkey format", "hotkey_invalid"),
        ];

        for (location, msg, expected_code) in sources {
            let e = GUIError::InvalidInput(msg.to_owned().to_owned());
            let v = serde_json::to_value(&e).unwrap();
            assert_ne!(
                v["invalid_input_code"], "unknown",
                "§2.2 凍結契約違反: {location} の文言 '{msg}' が unknown にフォールバック: {v}"
            );
            assert_eq!(
                v["invalid_input_code"], *expected_code,
                "§2.2 凍結契約違反: {location} の文言 '{msg}' が期待 '{expected_code}' でなく '{}': {v}",
                v["invalid_input_code"]
            );
        }
    }

    // TC-GUI-IPC-UT15 — §2.3 凍結 API 契約 全 13 variant 網羅テスト（将来 rename 防衛線）
    //
    // ペテルギウス指摘: `ipc_code_key()` と §2.3 凍結契約テーブルの完全一致を構造的に保証する。
    // 新 variant 追加・既存 variant rename 時にこのテストが Fail することで設計書更新を強制する。
    // `#[non_exhaustive]` のため将来追加分は `"unknown"` にフォールバックすることも
    // ここで暗黙に保証される（rust の網羅性チェック + フォールバックアームの存在）。
    #[test]
    fn ut15_ipc_code_key_exhaustive_contract_check() {
        use shikomi_core::RecordId;
        use uuid::Uuid;

        // §2.3 凍結契約テーブルの全 13 variant: (GUIError, 期待 ipc_code) のペア
        let cases: Vec<(GUIError, &str)> = vec![
            (
                GUIError::Ipc(IpcErrorCode::EncryptionUnsupported),
                "encryption_unsupported",
            ),
            (
                GUIError::Ipc(IpcErrorCode::NotFound {
                    id: RecordId::new(Uuid::nil()).unwrap(),
                }),
                "not_found",
            ),
            (
                GUIError::Ipc(IpcErrorCode::InvalidLabel {
                    reason: "invalid label".to_owned(),
                }),
                "invalid_label",
            ),
            (
                GUIError::Ipc(IpcErrorCode::Persistence {
                    reason: "persistence error".to_owned(),
                }),
                "persistence",
            ),
            (
                GUIError::Ipc(IpcErrorCode::Domain {
                    reason: "domain error".to_owned(),
                }),
                "domain",
            ),
            (
                GUIError::Ipc(IpcErrorCode::Internal {
                    reason: "unexpected error".to_owned(),
                }),
                "internal",
            ),
            (GUIError::Ipc(IpcErrorCode::VaultLocked), "vault_locked"),
            (
                GUIError::Ipc(IpcErrorCode::BackoffActive { wait_secs: 10 }),
                "backoff_active",
            ),
            (
                GUIError::Ipc(IpcErrorCode::RecoveryRequired),
                "recovery_required",
            ),
            (
                GUIError::Ipc(IpcErrorCode::ProtocolDowngrade),
                "protocol_downgrade",
            ),
            (
                GUIError::Ipc(IpcErrorCode::Crypto {
                    reason: "wrong-password".to_owned(),
                }),
                "crypto",
            ),
            (
                GUIError::Ipc(IpcErrorCode::HotkeyConflict {
                    reason: "hotkey conflict".to_owned(),
                }),
                "hotkey_conflict",
            ),
            (
                GUIError::Ipc(IpcErrorCode::HotkeyParseError {
                    reason: "invalid hotkey format".to_owned(),
                }),
                "hotkey_parse_error",
            ),
        ];

        for (error, expected_ipc_code) in cases {
            let v = serde_json::to_value(&error).unwrap();
            assert_eq!(
                v["kind"], "ipc_error",
                "kind must be ipc_error for {expected_ipc_code}: {v}"
            );
            assert_eq!(
                v["ipc_code"], expected_ipc_code,
                "§2.3 凍結契約違反: ipc_code_key() が '{expected_ipc_code}' を返すべきだが実際は '{ipc_code}'",
                ipc_code = v["ipc_code"]
            );
        }

        // §2.3 追加フィールド契約の整合性チェック
        // hotkey_conflict: hotkey_conflict_entry が必ず存在すること
        let hotkey_e = GUIError::Ipc(IpcErrorCode::HotkeyConflict {
            reason: "hotkey conflict".to_owned(),
        });
        let hotkey_v = serde_json::to_value(&hotkey_e).unwrap();
        assert_eq!(
            hotkey_v["hotkey_conflict_entry"], "hotkey conflict",
            "§2.3 凍結契約違反: hotkey_conflict に hotkey_conflict_entry フィールドが必要: {hotkey_v}"
        );

        // crypto: crypto_reason が必ず存在すること
        let crypto_e = GUIError::Ipc(IpcErrorCode::Crypto {
            reason: "wrong-password".to_owned(),
        });
        let crypto_v = serde_json::to_value(&crypto_e).unwrap();
        assert_eq!(
            crypto_v["crypto_reason"], "wrong-password",
            "§2.3 凍結契約違反: crypto に crypto_reason フィールドが必要: {crypto_v}"
        );

        // backoff_active: wait_secs が必ず存在すること
        let backoff_e = GUIError::Ipc(IpcErrorCode::BackoffActive { wait_secs: 5 });
        let backoff_v = serde_json::to_value(&backoff_e).unwrap();
        assert_eq!(
            backoff_v["wait_secs"], 5,
            "§2.3 凍結契約違反: backoff_active に wait_secs フィールドが必要: {backoff_v}"
        );
    }
}
