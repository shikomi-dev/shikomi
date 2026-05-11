//! GUI IPC クライアント + AppState 型エイリアス。
//!
//! `GuiIpcClient` は daemon との非同期 IPC 接続を保持する。
//! `AppState` は全 Tauri Commands で共有される接続状態を表す。
//!
//! 設計根拠: docs/features/shikomi-gui/ipc-client/basic-design.md §2.1 / §2.4
//! docs/features/shikomi-gui/ipc-client/detailed-design.md §1

use std::path::Path;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest, IpcResponse, MAX_FRAME_LENGTH};
use tokio::sync::Mutex;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use self::error::GUIError;

// ---------------------------------------------------------------------------
// Re-exports and sub-modules
// ---------------------------------------------------------------------------

pub mod commands;
pub mod error;

// ---------------------------------------------------------------------------
// プラットフォーム別 Stream 型（cfg）
// ---------------------------------------------------------------------------

#[cfg(unix)]
type Stream = tokio::net::UnixStream;

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// Tauri アプリ全体で共有する IPC 接続状態。
///
/// - `None`: daemon 未接続（起動直後 / 切断後）
/// - `Some(client)`: daemon 接続済み
///
/// `tokio::sync::Mutex` を使用することで async 関数内でロックを保持できる。
pub type AppState = Mutex<Option<GuiIpcClient>>;

// ---------------------------------------------------------------------------
// GuiIpcClient
// ---------------------------------------------------------------------------

/// daemon との非同期 IPC 接続を保持する構造体。
///
/// UDS（Unix）または Named Pipe（Windows）上で `LengthDelimitedCodec` + `MessagePack` を使用。
/// CLI の `IpcClient` と同一のトランスポート仕様（little-endian 4 byte 長さフィールド、
/// 最大フレーム長 16 MiB）。
#[derive(Debug)]
pub struct GuiIpcClient {
    framed: Framed<Stream, LengthDelimitedCodec>,
}

impl GuiIpcClient {
    /// daemon に接続し、V2 Handshake を確立する。
    ///
    /// # Errors
    /// - ソケット接続失敗: `GUIError::DaemonNotRunning`
    /// - Handshake 送受信失敗: `GUIError::ConnectionFailed`
    /// - プロトコル不一致: `GUIError::ProtocolVersionMismatch`
    /// - 予期しない応答: `GUIError::UnexpectedResponse`
    pub async fn connect(socket_path: &Path) -> Result<Self, GUIError> {
        let stream = open_stream(socket_path).await?;
        let mut framed = Framed::new(stream, codec());

        // Handshake 送信
        let request = IpcRequest::Handshake {
            client_version: IpcProtocolVersion::current(),
        };
        let bytes = rmp_serde::to_vec(&request).map_err(|e| GUIError::Encode(e.to_string()))?;
        framed
            .send(Bytes::from(bytes))
            .await
            .map_err(|e| GUIError::ConnectionFailed(e.kind().to_string()))?;

        // Handshake 応答受信
        let response_bytes = framed
            .next()
            .await
            .ok_or_else(|| {
                GUIError::ConnectionFailed("connection closed before handshake response".to_owned())
            })?
            .map_err(|e| GUIError::ConnectionFailed(e.kind().to_string()))?;

        let response: IpcResponse =
            rmp_serde::from_slice(&response_bytes).map_err(|e| GUIError::Decode(e.to_string()))?;

        match response {
            IpcResponse::Handshake { server_version }
                if server_version == IpcProtocolVersion::current() =>
            {
                Ok(Self { framed })
            }
            IpcResponse::Handshake { server_version } => Err(GUIError::ProtocolVersionMismatch {
                server: server_version.to_string(),
                client: IpcProtocolVersion::current().to_string(),
            }),
            IpcResponse::ProtocolVersionMismatch { server, client } => {
                Err(GUIError::ProtocolVersionMismatch {
                    server: server.to_string(),
                    client: client.to_string(),
                })
            }
            other => Err(GUIError::UnexpectedResponse(format!(
                "unexpected handshake response: {}",
                other.variant_name()
            ))),
        }
    }

    /// リクエスト送信 + レスポンス受信の 1 往復 helper。
    ///
    /// IO エラー時は `GUIError::ConnectionFailed(io_error.kind().to_string())` を返す
    /// （OWASP A04: OS 内部情報を含む生メッセージを使用しない）。
    ///
    /// # Errors
    /// - シリアライズ失敗: `GUIError::Encode`
    /// - 送信失敗: `GUIError::ConnectionFailed`
    /// - EOF（接続切断）: `GUIError::ConnectionFailed("connection closed")`
    /// - デシリアライズ失敗: `GUIError::Decode`
    pub async fn round_trip(&mut self, request: &IpcRequest) -> Result<IpcResponse, GUIError> {
        let bytes = rmp_serde::to_vec(request).map_err(|e| GUIError::Encode(e.to_string()))?;
        self.framed
            .send(Bytes::from(bytes))
            .await
            .map_err(|e| GUIError::ConnectionFailed(e.kind().to_string()))?;

        let response_bytes = self
            .framed
            .next()
            .await
            .ok_or_else(|| GUIError::ConnectionFailed("connection closed".to_owned()))?
            .map_err(|e| GUIError::ConnectionFailed(e.kind().to_string()))?;

        rmp_serde::from_slice(&response_bytes).map_err(|e| GUIError::Decode(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// 内部ヘルパ（OS 別 stream open）
// ---------------------------------------------------------------------------

#[cfg(unix)]
async fn open_stream(socket_path: &Path) -> Result<Stream, GUIError> {
    tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|_| GUIError::DaemonNotRunning)
}

#[cfg(windows)]
async fn open_stream(socket_path: &Path) -> Result<Stream, GUIError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let pipe_name = socket_path.to_str().ok_or(GUIError::DaemonNotRunning)?;
    ClientOptions::new()
        .open(pipe_name)
        .map_err(|_| GUIError::DaemonNotRunning)
}

/// `LengthDelimitedCodec` を CLI / daemon と同一仕様で構築する。
///
/// - little-endian
/// - 長さフィールド: 4 バイト
/// - 最大フレーム長: 16 MiB（`MAX_FRAME_LENGTH`）
fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_length(4)
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_codec()
}

// ---------------------------------------------------------------------------
// AppState 操作ヘルパ
// ---------------------------------------------------------------------------

/// `AppState` 経由で 1 往復 IPC を実行し、生 `IpcResponse` を返す。
///
/// - `AppState` が `None` の場合は `GUIError::NotConnected` を即返却
/// - `ConnectionFailed` の場合は `AppState` を `None` にリセットして返す（Fail Fast §5）
///
/// クロージャ版 `exec_with_client` と異なり `&IpcRequest` を受け取ることで
/// ライフタイム推論問題を回避し、呼び出しコードをシンプルに保つ。
///
/// # Errors
///
/// - `GUIError::NotConnected`: `AppState` が `None`（daemon 未接続）
/// - `GUIError::ConnectionFailed`: IPC 送信または受信に失敗（`AppState` を `None` にリセット）
/// - その他 `GUIError` variant: `GuiIpcClient::round_trip` が返すエラー
///
/// 設計根拠: docs/features/shikomi-gui/ipc-client/detailed-design.md §5
pub async fn round_trip_checked(
    state: &AppState,
    request: &IpcRequest,
) -> Result<IpcResponse, GUIError> {
    let mut guard = state.lock().await;
    let result = match guard.as_mut() {
        None => return Err(GUIError::NotConnected),
        Some(client) => client.round_trip(request).await,
    };
    if matches!(result, Err(GUIError::ConnectionFailed(_))) {
        *guard = None;
    }
    result
}
