//! エントリ CRUD 系 Tauri Commands。
//!
//! | コマンド | IpcRequest | 正常応答 |
//! |---|---|---|
//! | `list_entries` | `ListRecords` | `Records { records, protection_mode }` |
//! | `add_entry` | `AddRecord` | `Added { id }` |
//! | `update_entry` | `EditRecord` | `Edited { id }` |
//! | `delete_entry` | `RemoveRecord` | `Removed { id }` |
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md REQ-IPC-01〜04
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §3.1〜3.4

use serde::Serialize;
use shikomi_core::ipc::{
    IpcRequest, IpcResponse, ProtectionModeBanner, RecordSummary, SerializableSecretBytes,
};
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretBytes};
use tauri::State;
use time::OffsetDateTime;

use crate::ipc_client::error::GUIError;
use crate::ipc_client::{exec_with_client, AppState};

// ---------------------------------------------------------------------------
// 出力型
// ---------------------------------------------------------------------------

/// `list_entries` の戻り値。
#[derive(Debug, Serialize)]
pub struct ListEntriesOutput {
    /// レコード summary 列。
    pub entries: Vec<RecordSummary>,
    /// vault の保護モード。
    pub vault_status: ProtectionModeBanner,
}

/// `add_entry` / `update_entry` / `delete_entry` の戻り値。
#[derive(Debug, Serialize)]
pub struct EntryIdOutput {
    /// 対象エントリの ID（UUIDv7 文字列）。
    pub id: String,
}

// ---------------------------------------------------------------------------
// list_entries（REQ-IPC-01）
// ---------------------------------------------------------------------------

/// vault の全エントリ一覧と保護モードを取得する。
///
/// # Errors
/// `GUIError::NotConnected` / `GUIError::ConnectionFailed` / `GUIError::Decode` 等。
#[tauri::command]
pub async fn list_entries(state: State<'_, AppState>) -> Result<ListEntriesOutput, GUIError> {
    exec_with_client(&state, |client| async move {
        match client.round_trip(&IpcRequest::ListRecords).await? {
            IpcResponse::Records {
                records,
                protection_mode,
            } => Ok(ListEntriesOutput {
                entries: records,
                vault_status: protection_mode,
            }),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Records, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// add_entry（REQ-IPC-02）
// ---------------------------------------------------------------------------

/// 新しいエントリを vault に追加する。
///
/// Rust 側バリデーション（R1-GUI-19）:
/// - `label` が空文字列 → `GUIError::InvalidInput`
/// - `value` が空文字列 → `GUIError::InvalidInput`
///
/// # Errors
/// `GUIError::InvalidInput` / `GUIError::NotConnected` / `GUIError::Ipc` 等。
#[tauri::command]
pub async fn add_entry(
    state: State<'_, AppState>,
    kind: RecordKind,
    label: String,
    value: String,
    hotkey: Option<String>,
) -> Result<EntryIdOutput, GUIError> {
    // Rust 側バリデーション（バイパス対策、R1-GUI-19）
    if label.is_empty() {
        return Err(GUIError::InvalidInput("label must not be empty".to_owned()));
    }
    if value.is_empty() {
        return Err(GUIError::InvalidInput("value must not be empty".to_owned()));
    }

    // ラベル検証
    let record_label =
        RecordLabel::try_new(label).map_err(|e| GUIError::InvalidInput(e.to_string()))?;

    // 機密値: String → SerializableSecretBytes に即変換してドロップ（§4.1）
    let secret_value = SerializableSecretBytes::new(SecretBytes::from_vec(value.into_bytes()));

    let now = OffsetDateTime::now_utc();

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::AddRecord {
            kind,
            label: record_label,
            value: secret_value,
            now,
            hotkey,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Added { id } => Ok(EntryIdOutput { id: id.to_string() }),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Added, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// update_entry（REQ-IPC-03）
// ---------------------------------------------------------------------------

/// 既存エントリを更新する。
///
/// ハンドラに到達した場合は**必ず** IPC 送信する（Silent Failure 禁止）。
/// Sub-C（UI 層）は変更なし時に `invoke` を呼ばない契約を持つ（Tell, Don't Ask）。
///
/// # Errors
/// `GUIError::InvalidInput`（不正 UUID）/ `GUIError::NotConnected` / `GUIError::Ipc` 等。
#[tauri::command]
pub async fn update_entry(
    state: State<'_, AppState>,
    id: String,
    label: Option<String>,
    value: Option<String>,
) -> Result<EntryIdOutput, GUIError> {
    // ID 検証
    let record_id = RecordId::try_from_str(&id)
        .map_err(|_| GUIError::InvalidInput("invalid record id format".to_owned()))?;

    // ラベル検証（指定された場合のみ）
    let record_label = label
        .map(|l| RecordLabel::try_new(l).map_err(|e| GUIError::InvalidInput(e.to_string())))
        .transpose()?;

    // 機密値変換（指定された場合のみ）
    let secret_value =
        value.map(|v| SerializableSecretBytes::new(SecretBytes::from_vec(v.into_bytes())));

    let now = OffsetDateTime::now_utc();

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::EditRecord {
            id: record_id,
            label: record_label,
            value: secret_value,
            now,
            hotkey: None,
            clear_hotkey: false,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Edited { id } => Ok(EntryIdOutput { id: id.to_string() }),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Edited, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// delete_entry（REQ-IPC-04）
// ---------------------------------------------------------------------------

/// エントリを vault から削除する。
///
/// # Errors
/// `GUIError::InvalidInput`（不正 UUID）/ `GUIError::NotConnected` / `GUIError::Ipc` 等。
#[tauri::command]
pub async fn delete_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<EntryIdOutput, GUIError> {
    let record_id = RecordId::try_from_str(&id)
        .map_err(|_| GUIError::InvalidInput("invalid record id format".to_owned()))?;

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::RemoveRecord { id: record_id };
        match client.round_trip(&request).await? {
            IpcResponse::Removed { id } => Ok(EntryIdOutput { id: id.to_string() }),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Removed, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}
