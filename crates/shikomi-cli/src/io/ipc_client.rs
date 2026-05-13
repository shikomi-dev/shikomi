//! daemon との IPC 送受信の細部（`Framed` 保持 + `send_request` / `recv_response`）。
//!
//! 設計根拠: docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md §`IpcClient`

use std::path::Path;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest, IpcResponse, MAX_FRAME_LENGTH};
use shikomi_infra::persistence::PersistenceError;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[cfg(unix)]
type Stream = tokio::net::UnixStream;
#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

/// daemon との接続 + ハンドシェイク + リクエスト/レスポンス往復を担う非同期クライアント。
pub struct IpcClient {
    framed: Framed<Stream, LengthDelimitedCodec>,
}

impl IpcClient {
    /// daemon に接続し、ハンドシェイクを行う。
    ///
    /// # Errors
    /// 接続失敗 / ハンドシェイク失敗 / プロトコル不一致時に `PersistenceError` を返す。
    pub async fn connect(socket_path: &Path) -> Result<Self, PersistenceError> {
        let stream = open_stream(socket_path).await?;
        let mut framed = Framed::new(stream, codec());

        // ハンドシェイク 1 往復
        let request = IpcRequest::Handshake {
            client_version: IpcProtocolVersion::current(),
        };
        let bytes = rmp_serde::to_vec(&request).map_err(|e| PersistenceError::IpcEncode {
            reason: e.to_string(),
        })?;
        framed
            .send(Bytes::from(bytes))
            .await
            .map_err(|e| PersistenceError::IpcIo {
                reason: e.to_string(),
            })?;

        let response_bytes = framed
            .next()
            .await
            .ok_or_else(|| PersistenceError::IpcIo {
                reason: "connection closed before handshake response".to_owned(),
            })?
            .map_err(|e| PersistenceError::IpcIo {
                reason: e.to_string(),
            })?;
        let response: IpcResponse =
            rmp_serde::from_slice(&response_bytes).map_err(|e| PersistenceError::IpcDecode {
                reason: e.to_string(),
            })?;

        match response {
            IpcResponse::Handshake { server_version }
                if server_version == IpcProtocolVersion::current() =>
            {
                Ok(Self { framed })
            }
            IpcResponse::ProtocolVersionMismatch { server, client } => {
                Err(PersistenceError::ProtocolVersionMismatch { server, client })
            }
            _ => Err(PersistenceError::IpcDecode {
                reason: "unexpected handshake response".to_owned(),
            }),
        }
    }

    /// リクエスト送信。
    ///
    /// # Errors
    /// `PersistenceError::IpcEncode` / `PersistenceError::IpcIo`。
    pub async fn send_request(&mut self, request: &IpcRequest) -> Result<(), PersistenceError> {
        let bytes = rmp_serde::to_vec(request).map_err(|e| PersistenceError::IpcEncode {
            reason: e.to_string(),
        })?;
        self.framed
            .send(Bytes::from(bytes))
            .await
            .map_err(|e| PersistenceError::IpcIo {
                reason: e.to_string(),
            })?;
        Ok(())
    }

    /// レスポンス受信。
    ///
    /// # Errors
    /// `PersistenceError::IpcDecode` / `PersistenceError::IpcIo`。
    pub async fn recv_response(&mut self) -> Result<IpcResponse, PersistenceError> {
        let bytes = self
            .framed
            .next()
            .await
            .ok_or_else(|| PersistenceError::IpcIo {
                reason: "connection closed unexpectedly".to_owned(),
            })?
            .map_err(|e| PersistenceError::IpcIo {
                reason: e.to_string(),
            })?;
        rmp_serde::from_slice(&bytes).map_err(|e| PersistenceError::IpcDecode {
            reason: e.to_string(),
        })
    }

    /// 1 往復 helper。
    ///
    /// # Errors
    /// `send_request` / `recv_response` のいずれかが失敗した場合。
    pub async fn round_trip(
        &mut self,
        request: &IpcRequest,
    ) -> Result<IpcResponse, PersistenceError> {
        self.send_request(request).await?;
        self.recv_response().await
    }
}

// -------------------------------------------------------------------
// 内部関数（OS 別 stream open）
// -------------------------------------------------------------------

#[cfg(unix)]
async fn open_stream(socket_path: &Path) -> Result<Stream, PersistenceError> {
    tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|_| PersistenceError::DaemonNotRunning(socket_path.to_path_buf()))
}

#[cfg(windows)]
async fn open_stream(socket_path: &Path) -> Result<Stream, PersistenceError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    // path を pipe 名として使う（Windows では `default_socket_path` が `\\.\pipe\...` を返す）
    let pipe_name = socket_path
        .to_str()
        .ok_or_else(|| PersistenceError::DaemonNotRunning(socket_path.to_path_buf()))?;
    ClientOptions::new()
        .open(pipe_name)
        .map_err(|_| PersistenceError::DaemonNotRunning(socket_path.to_path_buf()))
}

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_length(4)
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_codec()
}
