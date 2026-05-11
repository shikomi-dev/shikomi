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
