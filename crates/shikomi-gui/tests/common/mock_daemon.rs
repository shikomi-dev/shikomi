//! MockDaemon — UDS テスト用擬似 daemon (Unix 専用)。
//!
//! IT テストの外部 I/O 依存（UDS socket + IPC フレーム）を差し替える。
//! 実 MessagePack フォーマット（shikomi-core::ipc 実型）を使用する（assumed mock 禁止）。

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest, IpcResponse, MAX_FRAME_LENGTH};
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio::sync::oneshot;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_length(4)
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_codec()
}

/// テスト用 UDS daemon。1 接続のみ受け付け、Handshake 後にコマンドリクエストを
/// 1 件受信してプリセットレスポンスを返す。
pub struct MockDaemon {
    /// UDS ソケットパス。
    pub socket_path: std::path::PathBuf,
    /// MockDaemon が受信した最初のコマンドリクエスト（テストで検証用）。
    pub received_request: oneshot::Receiver<IpcRequest>,
    /// TempDir を保持してソケットファイルの寿命を管理する。
    _tmpdir: TempDir,
}

impl MockDaemon {
    /// V2 Handshake を行い、コマンドリクエストを 1 件受信してプリセットレスポンスを返す daemon を起動する。
    pub async fn spawn(response: IpcResponse) -> Self {
        Self::spawn_inner(response, IpcProtocolVersion::V2, false).await
    }

    /// Handshake で `server_version` を返す daemon を起動する（IT03: バージョン不一致テスト用）。
    pub async fn spawn_with_server_version(
        response: IpcResponse,
        server_version: IpcProtocolVersion,
    ) -> Self {
        Self::spawn_inner(response, server_version, false).await
    }

    /// Handshake 後に接続を強制切断する daemon を起動する（IT18: 切断復旧テスト用）。
    pub async fn spawn_disconnect_after_handshake() -> Self {
        // response は使わないが型を合わせるためダミーを渡す
        Self::spawn_inner(IpcResponse::Decrypted, IpcProtocolVersion::V2, true).await
    }

    async fn spawn_inner(
        response: IpcResponse,
        server_version: IpcProtocolVersion,
        disconnect_after_handshake: bool,
    ) -> Self {
        let tmpdir = TempDir::new().expect("tempdir creation failed");
        let socket_path = tmpdir.path().join("daemon.sock");
        let (ready_tx, ready_rx) = oneshot::channel::<()>();
        let (request_tx, request_rx) = oneshot::channel::<IpcRequest>();

        let socket_path_clone = socket_path.clone();
        tokio::spawn(async move {
            let listener =
                UnixListener::bind(&socket_path_clone).expect("failed to bind mock daemon socket");
            // 準備完了を通知
            let _ = ready_tx.send(());

            let (stream, _) = listener.accept().await.expect("accept failed");
            let mut framed = Framed::new(stream, codec());

            // Handshake リクエスト受信
            let raw = match framed.next().await {
                Some(Ok(b)) => b,
                _ => return,
            };
            let _: IpcRequest = rmp_serde::from_slice(&raw).expect("handshake deserialization failed");

            // Handshake レスポンス送信
            let hs = IpcResponse::Handshake { server_version };
            let hs_bytes = rmp_serde::to_vec(&hs).expect("handshake serialization failed");
            framed
                .send(Bytes::from(hs_bytes))
                .await
                .expect("handshake response send failed");

            if disconnect_after_handshake {
                // 接続を強制切断（drop で stream が閉じる）
                drop(framed);
                return;
            }

            // コマンドリクエスト受信
            let raw = match framed.next().await {
                Some(Ok(b)) => b,
                _ => return,
            };
            let cmd: IpcRequest =
                rmp_serde::from_slice(&raw).expect("command request deserialization failed");

            // コマンドレスポンス送信
            let resp_bytes = rmp_serde::to_vec(&response).expect("response serialization failed");
            framed
                .send(Bytes::from(resp_bytes))
                .await
                .expect("command response send failed");

            // 受信したリクエストをテストに渡す
            let _ = request_tx.send(cmd);
        });

        // daemon がソケットをバインドするまで待つ
        ready_rx.await.expect("mock daemon ready signal failed");

        MockDaemon {
            socket_path,
            received_request: request_rx,
            _tmpdir: tmpdir,
        }
    }
}
