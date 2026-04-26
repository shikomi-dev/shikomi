//! IpcVaultRepository + IpcClient IT — test-design/integration.md §5 TC-IT-040〜044。
//!
//! CLI 側テストのため、daemon crate には依存せずに**最小限のスタブサーバ**を
//! tokio::net::UnixListener + Framed で直接立ち上げてハンドシェイク応答を返す。
//!
//! daemon 実装と CLI の end-to-end は `shikomi-daemon/tests/it_server_connection.rs`
//! が担当するため、本テストはクライアント側の**応答分岐**に特化する。
//!
//! 対応 Issue: #26

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_cli::io::ipc_client::IpcClient;
use shikomi_cli::io::ipc_vault_repository::IpcVaultRepository;
use shikomi_core::ipc::{
    IpcProtocolVersion, IpcRequest, IpcResponse, ProtectionModeBanner, MAX_FRAME_LENGTH,
};
use shikomi_infra::persistence::PersistenceError;
use tempfile::TempDir;
use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

// -------------------------------------------------------------------
// ヘルパー
// -------------------------------------------------------------------

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .little_endian()
        .length_field_length(4)
        .new_codec()
}

fn fresh_socket_dir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700");
    dir
}

/// ハンドシェイク成功応答を返すスタブ server を spawn。
/// `accept` 後 1 回だけ処理して終了する。
async fn spawn_handshake_ok_stub(sock_path: &std::path::Path) {
    let listener = UnixListener::bind(sock_path).expect("bind stub");
    let sock_path = sock_path.to_path_buf();
    let _ = sock_path; // keep path alive
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut framed: Framed<UnixStream, LengthDelimitedCodec> = Framed::new(stream, codec());
            // ハンドシェイク受信
            if let Some(Ok(_)) = framed.next().await {
                let resp = IpcResponse::Handshake {
                    // Sub-F (#44) 工程4 Bug-F-004: client は V2 で接続するため
                    // mock daemon の応答も V2 に追従。
                    server_version: IpcProtocolVersion::V2,
                };
                let bytes = rmp_serde::to_vec(&resp).unwrap();
                let _ = framed.send(Bytes::from(bytes)).await;
                // 接続維持（テストが drop するまで）
                let _ = framed.next().await;
            }
        }
    });
    // bind が完了しても accept が ready になるには一瞬待つ必要あり
    tokio::time::sleep(Duration::from_millis(30)).await;
}

/// `ProtocolVersionMismatch` 応答を返すスタブ server を spawn。
async fn spawn_handshake_mismatch_stub(sock_path: &std::path::Path) {
    let listener = UnixListener::bind(sock_path).expect("bind stub");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut framed: Framed<UnixStream, LengthDelimitedCodec> = Framed::new(stream, codec());
            if let Some(Ok(_req)) = framed.next().await {
                let resp = IpcResponse::ProtocolVersionMismatch {
                    server: IpcProtocolVersion::V1,
                    client: IpcProtocolVersion::V1, // サーバ側は V1、client も V1 (同値でも mismatch 応答を返せば client 側は error にする)
                };
                let bytes = rmp_serde::to_vec(&resp).unwrap();
                let _ = framed.send(Bytes::from(bytes)).await;
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
}

/// 接続受付後に即時 close するスタブ server。
async fn spawn_immediate_close_stub(sock_path: &std::path::Path) {
    let listener = UnixListener::bind(sock_path).expect("bind stub");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            drop(stream); // 即座に close
        }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
}

/// 不正 MessagePack を返すスタブ server。
async fn spawn_bad_msgpack_stub(sock_path: &std::path::Path) {
    let listener = UnixListener::bind(sock_path).expect("bind stub");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut framed: Framed<UnixStream, LengthDelimitedCodec> = Framed::new(stream, codec());
            if let Some(Ok(_)) = framed.next().await {
                // 不正バイト列を応答として送る
                let _ = framed
                    .send(Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]))
                    .await;
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
}

// -------------------------------------------------------------------
// TC-IT-040: IpcClient::connect が Handshake V1 成功応答で Ok を返す
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_040_client_connect_handshake_v1_succeeds() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    spawn_handshake_ok_stub(&sock).await;

    let client = IpcClient::connect(&sock).await;
    assert!(
        client.is_ok(),
        "connect should succeed (err: {:?})",
        client.as_ref().err()
    );
}

// -------------------------------------------------------------------
// TC-IT-041: ProtocolVersionMismatch 応答 → ProtocolVersionMismatch エラー
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_041_version_mismatch_response_maps_to_error() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    spawn_handshake_mismatch_stub(&sock).await;

    let res = IpcClient::connect(&sock).await;
    match res {
        Err(PersistenceError::ProtocolVersionMismatch { server, client }) => {
            assert_eq!(server, IpcProtocolVersion::V1);
            assert_eq!(client, IpcProtocolVersion::V1);
        }
        Err(other) => panic!("expected ProtocolVersionMismatch, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

// -------------------------------------------------------------------
// TC-IT-042: 接続直後 stream close → IpcIo エラー
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_042_stream_closed_before_response_returns_ipc_io() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    spawn_immediate_close_stub(&sock).await;

    let res = IpcClient::connect(&sock).await;
    match res {
        Err(PersistenceError::IpcIo { reason }) => {
            // 実装は「closed before handshake response」「EOF」「Connection reset」等を返す
            // いずれも接続が壊れたことを示す観測可能な文言であれば OK
            let r = reason.to_lowercase();
            assert!(
                r.contains("closed")
                    || r.contains("broken")
                    || r.contains("eof")
                    || r.contains("reset")
                    || r.contains("pipe"),
                "unexpected reason: {reason}"
            );
        }
        Err(other) => panic!("expected IpcIo error, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

// -------------------------------------------------------------------
// TC-IT-043: 不正 MessagePack 応答 → IpcDecode エラー
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_043_bad_msgpack_response_returns_ipc_decode() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    spawn_bad_msgpack_stub(&sock).await;

    let res = IpcClient::connect(&sock).await;
    match res {
        Err(PersistenceError::IpcDecode { .. }) => {} // success
        Err(other) => panic!("expected IpcDecode error, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

// -------------------------------------------------------------------
// TC-IT-044: daemon 未起動 → DaemonNotRunning / IpcIo 系エラー
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_044_connect_to_nonexistent_socket_fails() {
    let dir = fresh_socket_dir();
    let nonexistent = dir.path().join("nonexistent.sock");

    let res = IpcClient::connect(&nonexistent).await;
    assert!(res.is_err(), "connect to nonexistent socket should fail");
    let err = res.err().unwrap();
    match &err {
        PersistenceError::DaemonNotRunning { .. }
        | PersistenceError::IpcIo { .. }
        | PersistenceError::Io { .. } => {}
        other => panic!("unexpected error kind: {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-045+: IpcVaultRepository::connect (sync wrapper) も nonexistent で失敗
// -------------------------------------------------------------------
#[test]
fn tc_it_045_repository_connect_to_nonexistent_socket_fails() {
    let dir = fresh_socket_dir();
    let nonexistent = dir.path().join("nonexistent.sock");
    let res = IpcVaultRepository::connect(&nonexistent);
    assert!(res.is_err());
}

// -------------------------------------------------------------------
// TC-IT-046+: send_request + recv_response ラウンドトリップ（ListRecords → Records）
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_046_list_records_request_response_roundtrip() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");

    // handshake OK → ListRecords 受信 → Records(空) 応答 → close のスタブ
    let listener = UnixListener::bind(&sock).expect("bind");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut framed: Framed<UnixStream, LengthDelimitedCodec> = Framed::new(stream, codec());
            // 1. handshake
            if let Some(Ok(_)) = framed.next().await {
                let resp = IpcResponse::Handshake {
                    // Sub-F (#44) 工程4 Bug-F-004: V2 client に追従。
                    server_version: IpcProtocolVersion::V2,
                };
                let b = rmp_serde::to_vec(&resp).unwrap();
                let _ = framed.send(Bytes::from(b)).await;
            }
            // 2. ListRecords → Records(空) — Sub-F (#44) 構造体化対応
            if let Some(Ok(_)) = framed.next().await {
                let resp = IpcResponse::Records {
                    records: vec![],
                    protection_mode: ProtectionModeBanner::Plaintext,
                };
                let b = rmp_serde::to_vec(&resp).unwrap();
                let _ = framed.send(Bytes::from(b)).await;
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;

    let mut client = IpcClient::connect(&sock).await.expect("connect");
    client
        .send_request(&IpcRequest::ListRecords)
        .await
        .expect("send");
    let resp = client.recv_response().await.expect("recv");
    match resp {
        IpcResponse::Records { records, .. } => assert!(records.is_empty()),
        other => panic!("expected Records, got {other:?}"),
    }
}
