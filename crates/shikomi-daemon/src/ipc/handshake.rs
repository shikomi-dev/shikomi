//! ハンドシェイク（接続直後の必須 1 往復、5 秒タイムアウト）。

use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest, IpcResponse};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

/// ハンドシェイクタイムアウト。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

// -------------------------------------------------------------------
// HandshakeError
// -------------------------------------------------------------------

/// ハンドシェイク失敗の理由。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HandshakeError {
    /// 5 秒以内に Handshake フレームが届かなかった。
    #[error("handshake timeout")]
    Timeout,
    /// クライアントが Handshake 送信前に接続を閉じた。
    #[error("connection closed before handshake")]
    ConnectionClosed,
    /// フレーム受信エラー（length 超過 / I/O 失敗等）。
    #[error("frame error: {0}")]
    FrameError(std::io::Error),
    /// MessagePack デコード失敗。
    #[error("decode failed: {0}")]
    Decode(rmp_serde::decode::Error),
    /// 最初のフレームが Handshake でなかった。
    #[error("first frame must be handshake; got {got}")]
    ExpectedHandshake {
        /// 受信した variant 名。
        got: &'static str,
    },
    /// プロトコルバージョン不一致（応答済み、接続切断）。
    #[error("protocol version mismatch: server={server}, client={client}")]
    VersionMismatch {
        /// daemon 側バージョン。
        server: IpcProtocolVersion,
        /// クライアント側バージョン。
        client: IpcProtocolVersion,
    },
    /// 応答送信失敗（I/O / encode）。
    #[error("response send failed: {0}")]
    Send(std::io::Error),
    /// 応答エンコード失敗。
    #[error("response encode failed: {0}")]
    Encode(rmp_serde::encode::Error),
}

// -------------------------------------------------------------------
// negotiate
// -------------------------------------------------------------------

/// ハンドシェイク 1 往復を行う。
///
/// - 5 秒以内に `IpcRequest::Handshake` を受信
/// - `client_version == IpcProtocolVersion::current()` なら `Handshake` 返送 → `Ok(())`
/// - 不一致なら `IpcResponse::ProtocolVersionMismatch` 返送 → `Err(VersionMismatch)`
///
/// # Errors
/// `HandshakeError` 各バリアント。
pub async fn negotiate<S>(
    framed: &mut Framed<S, LengthDelimitedCodec>,
) -> Result<(), HandshakeError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let frame_opt = tokio::time::timeout(HANDSHAKE_TIMEOUT, framed.next())
        .await
        .map_err(|_| HandshakeError::Timeout)?;

    let bytes = match frame_opt {
        None => return Err(HandshakeError::ConnectionClosed),
        Some(Err(e)) => return Err(HandshakeError::FrameError(e)),
        Some(Ok(b)) => b,
    };

    let request: IpcRequest = rmp_serde::from_slice(&bytes).map_err(HandshakeError::Decode)?;

    let client_version = match request {
        IpcRequest::Handshake { client_version } => client_version,
        other => {
            return Err(HandshakeError::ExpectedHandshake {
                got: other.variant_name(),
            });
        }
    };

    let server_version = IpcProtocolVersion::current();
    if client_version == server_version {
        let response = IpcResponse::Handshake { server_version };
        let bytes = rmp_serde::to_vec(&response).map_err(HandshakeError::Encode)?;
        framed
            .send(Bytes::from(bytes))
            .await
            .map_err(HandshakeError::Send)?;
        Ok(())
    } else {
        let response = IpcResponse::ProtocolVersionMismatch {
            server: server_version,
            client: client_version,
        };
        let bytes = rmp_serde::to_vec(&response).map_err(HandshakeError::Encode)?;
        // 不一致応答は best-effort で送る（送信失敗時もエラー本体を優先して返す）
        let _ = framed.send(Bytes::from(bytes)).await;
        Err(HandshakeError::VersionMismatch {
            server: server_version,
            client: client_version,
        })
    }
}
