//! IpcServer 接続単位 IT — test-design/integration.md §4 TC-IT-010〜025。
//!
//! 実 `UnixListener` + `UnixStream` を tempfile ソケットパスで起動し、
//! `IpcServer::start_with_shutdown` を spawn する。同プロセスで動く client は同 UID を
//! 持つため `peer_credential::verify` は必ず成功する（多層防御の OS 拒否経路は別途
//! E2E で `sudo -u` テストが担当）。
//!
//! 対応 Issue: #26

#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{
    IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse, SerializableSecretBytes,
    MAX_FRAME_LENGTH,
};
use shikomi_core::{
    RecordId, RecordKind, RecordLabel, SecretBytes, Vault, VaultHeader, VaultVersion,
};
use shikomi_daemon::ipc::server::IpcServer;
use shikomi_daemon::ipc::transport::ListenerEnum;
use shikomi_daemon::lifecycle::single_instance::SingleInstanceLock;
use shikomi_infra::persistence::{PersistenceError, SqliteVaultRepository, VaultRepository};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::net::UnixStream;
use tokio::sync::{Mutex, Notify};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use uuid::Uuid;

// -------------------------------------------------------------------
// ヘルパー
// -------------------------------------------------------------------

fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
}

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .little_endian()
        .length_field_length(4)
        .new_codec()
}

/// tempdir に 0700 を適用して返す。
fn fresh_socket_dir() -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700");
    dir
}

/// 平文空 vault を作成し、file system に永続化してから load で返す
/// （repo.save → repo.load の実経路を通す）。
fn fresh_vault_and_repo(dir: &std::path::Path) -> (Arc<Mutex<Vault>>, Arc<SqliteVaultRepository>) {
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_time()).unwrap();
    let vault = Vault::new(header);
    let repo = SqliteVaultRepository::from_directory(dir).expect("repo");
    repo.save(&vault).expect("initial save");
    (Arc::new(Mutex::new(vault)), Arc::new(repo))
}

/// 起動済み server の Unix stream に connect し Framed を返す。
async fn connect_framed(sock_path: &std::path::Path) -> Framed<UnixStream, LengthDelimitedCodec> {
    let stream = UnixStream::connect(sock_path)
        .await
        .expect("client connect");
    Framed::new(stream, codec())
}

/// ハンドシェイク V1 1 往復を完了させる。
async fn client_handshake_v1(framed: &mut Framed<UnixStream, LengthDelimitedCodec>) {
    let req = IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V1,
    };
    let bytes = rmp_serde::to_vec(&req).unwrap();
    framed
        .send(Bytes::from(bytes))
        .await
        .expect("send handshake");
    let received = framed
        .next()
        .await
        .expect("handshake response")
        .expect("framed ok");
    let resp: IpcResponse = rmp_serde::from_slice(&received).expect("decode handshake resp");
    match resp {
        IpcResponse::Handshake { .. } => {}
        other => panic!("expected Handshake response, got {other:?}"),
    }
}

async fn send_request(framed: &mut Framed<UnixStream, LengthDelimitedCodec>, req: &IpcRequest) {
    let bytes = rmp_serde::to_vec(req).unwrap();
    framed.send(Bytes::from(bytes)).await.expect("send req");
}

async fn recv_response(framed: &mut Framed<UnixStream, LengthDelimitedCodec>) -> IpcResponse {
    let received = framed.next().await.expect("response").expect("framed ok");
    rmp_serde::from_slice(&received).expect("decode resp")
}

/// IpcServer を tempfile ソケットで起動し、`(socket_path, lock_guard, shutdown, server_handle)`
/// を返す。shutdown.notify_waiters() で server を停止できる。
async fn spawn_test_server(dir: &TempDir) -> TestServerHandle {
    let (vault_arc, repo_arc) = fresh_vault_and_repo(dir.path());

    let mut lock = SingleInstanceLock::acquire_unix(dir.path()).expect("acquire_unix");
    let ListenerEnum::Unix {
        listener,
        socket_path,
    } = lock.take_listener().expect("take_listener")
    else {
        unreachable!("test only cfg(unix)")
    };

    let shutdown = Arc::new(Notify::new());
    let mut server = IpcServer::new(
        ListenerEnum::Unix {
            listener,
            socket_path: socket_path.clone(),
        },
        Arc::clone(&repo_arc),
        Arc::clone(&vault_arc),
    );
    let shutdown_for_server = Arc::clone(&shutdown);
    let server_handle = tokio::spawn(async move {
        let _ = server.start_with_shutdown(shutdown_for_server).await;
    });
    // accept loop の開始を保証するため微小 yield
    tokio::time::sleep(Duration::from_millis(30)).await;
    TestServerHandle {
        socket_path,
        _lock: lock,
        shutdown,
        server_handle,
        vault: vault_arc,
        repo: repo_arc,
    }
}

struct TestServerHandle {
    socket_path: std::path::PathBuf,
    _lock: SingleInstanceLock,
    shutdown: Arc<Notify>,
    server_handle: tokio::task::JoinHandle<()>,
    #[allow(dead_code)]
    vault: Arc<Mutex<Vault>>,
    #[allow(dead_code)]
    repo: Arc<SqliteVaultRepository>,
}

impl TestServerHandle {
    async fn shutdown_and_join(self) {
        self.shutdown.notify_waiters();
        // graceful shutdown 完了を待つ（最大 5 秒）
        let _ = tokio::time::timeout(Duration::from_secs(5), self.server_handle).await;
    }
}

// -------------------------------------------------------------------
// TC-IT-010: ハンドシェイク → List 空 vault → close
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_010_handshake_then_list_empty() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;

    send_request(&mut framed, &IpcRequest::ListRecords).await;
    let resp = recv_response(&mut framed).await;
    match resp {
        IpcResponse::Records(v) => assert!(v.is_empty()),
        other => panic!("expected Records, got {other:?}"),
    }
    drop(framed);
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-011: Add → List ラウンドトリップ
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_011_add_then_list_roundtrip() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;

    // Add
    let add_req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("L".into()).unwrap(),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(b"V".to_vec())),
        now: fixed_time(),
    };
    send_request(&mut framed, &add_req).await;
    let resp = recv_response(&mut framed).await;
    let added_id = match resp {
        IpcResponse::Added { id } => id,
        other => panic!("expected Added, got {other:?}"),
    };

    // List 確認
    send_request(&mut framed, &IpcRequest::ListRecords).await;
    let list_resp = recv_response(&mut framed).await;
    match list_resp {
        IpcResponse::Records(v) => {
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].id, added_id);
            assert_eq!(v[0].label.as_str(), "L");
        }
        other => panic!("expected Records, got {other:?}"),
    }
    drop(framed);
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-012: Edit label → receive Edited
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_012_edit_label_returns_edited() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;

    // まず Add
    let add = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("old".into()).unwrap(),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(b"V".to_vec())),
        now: fixed_time(),
    };
    send_request(&mut framed, &add).await;
    let added = recv_response(&mut framed).await;
    let id = match added {
        IpcResponse::Added { id } => id,
        other => panic!("expected Added, got {other:?}"),
    };

    // Edit
    let edit = IpcRequest::EditRecord {
        id: id.clone(),
        label: Some(RecordLabel::try_new("new".into()).unwrap()),
        value: None,
        now: fixed_time() + time::Duration::seconds(1),
    };
    send_request(&mut framed, &edit).await;
    let edited = recv_response(&mut framed).await;
    match edited {
        IpcResponse::Edited { id: rid } => assert_eq!(rid, id),
        other => panic!("expected Edited, got {other:?}"),
    }
    drop(framed);
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-013: Remove → receive Removed
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_013_remove_existing_returns_removed() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;

    let add = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("bye".into()).unwrap(),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(b"V".to_vec())),
        now: fixed_time(),
    };
    send_request(&mut framed, &add).await;
    let added = recv_response(&mut framed).await;
    let id = match added {
        IpcResponse::Added { id } => id,
        other => panic!("expected Added, got {other:?}"),
    };

    send_request(&mut framed, &IpcRequest::RemoveRecord { id: id.clone() }).await;
    let removed = recv_response(&mut framed).await;
    match removed {
        IpcResponse::Removed { id: rid } => assert_eq!(rid, id),
        other => panic!("expected Removed, got {other:?}"),
    }
    drop(framed);
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-015: Edit NotFound
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_015_edit_nonexistent_id_returns_not_found() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;

    let ghost = RecordId::new(Uuid::now_v7()).unwrap();
    let req = IpcRequest::EditRecord {
        id: ghost.clone(),
        label: Some(RecordLabel::try_new("new".into()).unwrap()),
        value: None,
        now: fixed_time(),
    };
    send_request(&mut framed, &req).await;
    let resp = recv_response(&mut framed).await;
    match resp {
        IpcResponse::Error(IpcErrorCode::NotFound { id }) => assert_eq!(id, ghost),
        other => panic!("expected NotFound, got {other:?}"),
    }
    drop(framed);
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-020: プロトコル不一致（server-side 判定）
//
// **重要な観察（BUG-DAEMON-IPC-001）**:
// test-design §4.3 TC-IT-020 は「未知 version 文字列 → `ProtocolVersionMismatch`
// 応答 → 切断」を期待しているが、現実装の `IpcProtocolVersion` enum は V1 のみで
// `#[serde(other)]` 等の未知 variant フォールバックを持たない。その結果、未知
// version 文字列を含むフレームは rmp-serde の decode 時点で失敗し、
// `HandshakeError::Decode` が返り **`ProtocolVersionMismatch` 応答を送らずに
// 接続が即切断される**。
//
// 本 TC は現実装の挙動（decode reject → close）を検証する。将来 `V2` 等の新
// バリアント追加時に「V2 daemon ↔ V1 client」の組合せで初めて ProtocolVersionMismatch
// 経路が実呼出されるため、本 TC は `V2` 追加時に応答検証版へ昇格予定。
//
// 詳細はバグレポート `daemon-ipc-bugs.md` §BUG-DAEMON-IPC-001 参照。
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_020_unknown_version_bytes_close_connection() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;

    // "v1" → "v9" に書換えて未知 variant を作る
    let mut bytes = rmp_serde::to_vec(&IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V1,
    })
    .unwrap();
    if let Some(pos) = bytes.windows(2).position(|w| w == b"v1") {
        bytes[pos + 1] = b'9';
    } else {
        panic!("could not find v1 bytes in serialized request: {bytes:?}");
    }

    framed.send(Bytes::from(bytes)).await.expect("send");

    // 現実装では decode 失敗 → 応答なしで close。test-design の期待値
    // （`ProtocolVersionMismatch` 応答）は V2 variant 追加後に満たされる。
    // 本 TC は **接続が切断されること**（fail-secure）のみ検証する。
    let next = tokio::time::timeout(Duration::from_secs(3), framed.next())
        .await
        .expect("response within 3s");
    assert!(
        next.is_none() || matches!(next, Some(Err(_))),
        "connection should close on unknown version variant (current impl: decode reject); got {next:?}"
    );
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-021: 最初のフレームが Handshake でない → 即切断
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_021_first_frame_not_handshake_closes_connection() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;

    // ListRecords を先に送る
    send_request(&mut framed, &IpcRequest::ListRecords).await;
    // サーバ側は handshake::ExpectedHandshake で即切断する
    let next = framed.next().await;
    assert!(
        next.is_none() || matches!(next, Some(Err(_))),
        "connection should close when first frame is not Handshake: got {next:?}"
    );
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-023: MessagePack 破損フレーム → 当該接続のみ切断
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_023_broken_msgpack_frame_closes_connection() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;

    // まず handshake を成功させる
    client_handshake_v1(&mut framed).await;

    // 不正バイト列を送る
    framed
        .send(Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]))
        .await
        .expect("send garbage");

    // 接続が閉じる（サーバは decode error で handle_connection から抜ける）
    let next = framed.next().await;
    assert!(
        next.is_none() || matches!(next, Some(Err(_))),
        "connection should close on decode error: got {next:?}"
    );
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-025: 複数接続の独立性 — 接続 A の破損が接続 B に影響しない
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_025_broken_connection_does_not_affect_other_connection() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;

    // 接続 A — 破損送信
    let mut framed_a = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed_a).await;
    framed_a
        .send(Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))
        .await
        .expect("send garbage from A");

    // 接続 B — 正常 List
    let mut framed_b = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed_b).await;
    send_request(&mut framed_b, &IpcRequest::ListRecords).await;
    let resp_b = recv_response(&mut framed_b).await;
    match resp_b {
        IpcResponse::Records(_) => {} // 成功
        other => panic!("expected Records on B, got {other:?}"),
    }
    drop(framed_a);
    drop(framed_b);
    handle.shutdown_and_join().await;
}

// -------------------------------------------------------------------
// TC-IT-030: graceful shutdown — idle 接続が close される
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_030_graceful_shutdown_closes_idle_connection() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;

    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;
    // アイドル状態で shutdown を発火
    handle.shutdown.notify_waiters();
    // client 側は接続 close を観測する
    let next = tokio::time::timeout(Duration::from_secs(3), framed.next())
        .await
        .expect("wait next within 3s");
    assert!(
        next.is_none() || matches!(next, Some(Err(_))),
        "idle connection should be closed on shutdown: got {next:?}"
    );
    // server のタスクも完了（shutdown_and_join は内部で shutdown を再度発火するが冪等）
    let _ = tokio::time::timeout(Duration::from_secs(5), handle.server_handle).await;
}

// -------------------------------------------------------------------
// TC-IT-016: InvalidLabel — UTF-8 不正バイト列で InvalidLabel 返送 + reason 固定文言
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_016_invalid_utf8_value_returns_invalid_label_with_fixed_reason() {
    let dir = fresh_socket_dir();
    let handle = spawn_test_server(&dir).await;
    let mut framed = connect_framed(&handle.socket_path).await;
    client_handshake_v1(&mut framed).await;

    // UTF-8 不正な secret bytes を value として送る
    let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
    let add = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("L".into()).unwrap(),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(invalid_utf8)),
        now: fixed_time(),
    };
    send_request(&mut framed, &add).await;
    let resp = recv_response(&mut framed).await;
    match resp {
        IpcResponse::Error(IpcErrorCode::InvalidLabel { reason }) => {
            // reason は固定文言 — 絶対パス / SECRET_TEST_VALUE / pid を含まない
            assert!(!reason.contains("/home/"));
            assert!(!reason.to_lowercase().contains("pid"));
            assert!(!reason.contains("SECRET_TEST_VALUE"));
        }
        other => panic!("expected InvalidLabel, got {other:?}"),
    }
    drop(framed);
    handle.shutdown_and_join().await;
}
