//! vault 操作系 Tauri Commands。
//!
//! | コマンド | IpcRequest | 正常応答 |
//! |---|---|---|
//! | `get_vault_status` | `ListRecords` | `protection_mode` のみ |
//! | `encrypt_vault` | `Encrypt` | `Encrypted { disclosure }` → BIP-39 24 語 |
//! | `decrypt_vault` | `Decrypt` | `Decrypted` |
//! | `unlock_vault` | `Unlock { recovery: None }` | `Unlocked` |
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md REQ-IPC-07〜10
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §3.7〜3.10

use serde::Serialize;
use shikomi_core::ipc::{IpcRequest, IpcResponse, ProtectionModeBanner, SerializableSecretBytes};
use shikomi_core::SecretBytes;
use tauri::State;

use crate::ipc_client::error::GUIError;
use crate::ipc_client::{exec_with_client, AppState};

// ---------------------------------------------------------------------------
// 出力型
// ---------------------------------------------------------------------------

/// `get_vault_status` の戻り値。
#[derive(Debug, Serialize)]
pub struct VaultStatusOutput {
    /// vault の保護モード。
    pub vault_status: ProtectionModeBanner,
}

/// `encrypt_vault` の戻り値。
#[derive(Debug, Serialize)]
pub struct EncryptOutput {
    /// BIP-39 24 語（R1-GUI-11）。Sub-C は表示後即 null クリアする責務を持つ。
    pub disclosure: Vec<String>,
}

/// `decrypt_vault` / `unlock_vault` の戻り値（成功のみ、UI は `get_vault_status` で再取得）。
#[derive(Debug, Serialize)]
pub struct EmptyOutput {}

// ---------------------------------------------------------------------------
// get_vault_status（REQ-IPC-07）
// ---------------------------------------------------------------------------

/// vault の保護モードを取得する（vault 状態の単独取得 API）。
///
/// `ListRecords` を 1 往復し、`Records` の `protection_mode` のみ返却する。
///
/// # Errors
/// `GUIError::NotConnected` / `GUIError::ConnectionFailed` / `GUIError::Decode` 等。
#[tauri::command]
pub async fn get_vault_status(state: State<'_, AppState>) -> Result<VaultStatusOutput, GUIError> {
    exec_with_client(&state, |client| async move {
        match client.round_trip(&IpcRequest::ListRecords).await? {
            IpcResponse::Records {
                protection_mode, ..
            } => Ok(VaultStatusOutput {
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
// encrypt_vault（REQ-IPC-08）
// ---------------------------------------------------------------------------

/// vault を暗号化し、BIP-39 recovery 24 語を返す。
///
/// Rust 側バリデーション（R1-GUI-19）:
/// - `master_password` が空文字列 → `GUIError::InvalidInput`
///
/// # Errors
/// `GUIError::InvalidInput` / `GUIError::NotConnected` /
/// `GUIError::Ipc(Crypto { reason: "weak-password" })` 等。
#[tauri::command]
pub async fn encrypt_vault(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<EncryptOutput, GUIError> {
    if master_password.is_empty() {
        return Err(GUIError::InvalidInput(
            "master password must not be empty".to_owned(),
        ));
    }

    // 機密値: String → SerializableSecretBytes に即変換してドロップ（§4.1）
    let secret = SerializableSecretBytes::new(SecretBytes::from_vec(master_password.into_bytes()));

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::Encrypt {
            master_password: secret,
            accept_limits: false,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Encrypted { disclosure } => {
                // SerializableSecretBytes → String に変換（R1-GUI-11）
                let words: Vec<String> = disclosure
                    .into_iter()
                    .map(|w| w.to_lossy_string_for_handler())
                    .collect();
                Ok(EncryptOutput { disclosure: words })
            }
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Encrypted, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// decrypt_vault（REQ-IPC-09）
// ---------------------------------------------------------------------------

/// vault を平文に戻す。
///
/// Rust 側バリデーション（R1-GUI-19）:
/// - `confirmed == false` → `GUIError::InvalidInput`（バイパス試行として即 Fail Fast）
/// - `master_password` が空文字列 → `GUIError::InvalidInput`
///
/// `confirmed` の意味論（R1-GUI-12）: JS 側チェックボックスが `checked == true` の場合のみ
/// `confirmed: true` で本コマンドを呼び出す。
///
/// # Errors
/// `GUIError::InvalidInput` / `GUIError::NotConnected` /
/// `GUIError::Ipc(Crypto { reason: "wrong-password" })` 等。
#[tauri::command]
pub async fn decrypt_vault(
    state: State<'_, AppState>,
    master_password: String,
    confirmed: bool,
) -> Result<EmptyOutput, GUIError> {
    // confirmed == false は即 Fail Fast（R1-GUI-12、バイパス対策）
    if !confirmed {
        return Err(GUIError::InvalidInput(
            "decrypt confirmation required".to_owned(),
        ));
    }
    if master_password.is_empty() {
        return Err(GUIError::InvalidInput(
            "master password must not be empty".to_owned(),
        ));
    }

    // 機密値: String → SerializableSecretBytes に即変換してドロップ（§4.1）
    let secret = SerializableSecretBytes::new(SecretBytes::from_vec(master_password.into_bytes()));

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::Decrypt {
            master_password: secret,
            confirmed: true,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Decrypted => Ok(EmptyOutput {}),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Decrypted, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// unlock_vault（REQ-IPC-10）
// ---------------------------------------------------------------------------

/// 暗号化 vault をアンロックする。
///
/// vault がロック状態での書き込み操作前にアンロックモーダルから呼ばれる（R1-GUI-13）。
/// recovery 経路（24 語）は `recovery: None` で省略（パスワード経路のみ）。
///
/// Rust 側バリデーション（R1-GUI-19）:
/// - `master_password` が空文字列 → `GUIError::InvalidInput`
///
/// # Errors
/// `GUIError::InvalidInput` / `GUIError::NotConnected` /
/// `GUIError::Ipc(Crypto { reason: "wrong-password" })` /
/// `GUIError::Ipc(BackoffActive)` / `GUIError::Ipc(RecoveryRequired)` 等。
#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<EmptyOutput, GUIError> {
    if master_password.is_empty() {
        return Err(GUIError::InvalidInput(
            "master password must not be empty".to_owned(),
        ));
    }

    // 機密値: String → SerializableSecretBytes に即変換してドロップ（§4.1）
    let secret = SerializableSecretBytes::new(SecretBytes::from_vec(master_password.into_bytes()));

    exec_with_client(&state, move |client| async move {
        let request = IpcRequest::Unlock {
            master_password: secret,
            recovery: None,
        };
        match client.round_trip(&request).await? {
            IpcResponse::Unlocked => Ok(EmptyOutput {}),
            IpcResponse::Error(code) => Err(GUIError::Ipc(code)),
            other => Err(GUIError::UnexpectedResponse(format!(
                "expected Unlocked, got {}",
                other.variant_name()
            ))),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc_client::{AppState, GuiIpcClient};
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    fn build_none_app() -> tauri::App<tauri::test::MockRuntime> {
        mock_builder()
            .manage(tokio::sync::Mutex::new(None::<GuiIpcClient>) as AppState)
            .build(mock_context(noop_assets()))
            .expect("failed to build mock Tauri app")
    }

    // TC-GUI-IPC-UT07 — decrypt_vault: confirmed=false → InvalidInput (Fail Fast)
    #[tokio::test]
    async fn ut07_decrypt_vault_confirmed_false_returns_invalid_input() {
        let app = build_none_app();
        let state = app.state::<AppState>();
        let result = decrypt_vault(state, "correct-password".to_owned(), false).await;
        assert!(
            matches!(&result, Err(GUIError::InvalidInput(m)) if m == "decrypt confirmation required"),
            "Expected InvalidInput(decrypt confirmation required), got: {result:?}"
        );
    }
}
