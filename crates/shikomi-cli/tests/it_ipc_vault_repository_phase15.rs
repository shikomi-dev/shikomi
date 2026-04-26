//! `IpcVaultRepository` 専用メソッド round-trip IT (Phase 1.5)
//! — test-design/integration.md §5.4 TC-IT-080〜089
//!
//! 案 D の核：`IpcVaultRepository::add_record / edit_record / remove_record` の
//! 専用メソッドが `tokio::net::UnixListener` 上のスタブサーバと**ハンドシェイク
//! 込み**で round-trip し、daemon 応答を期待値に写像することを検証する。
//!
//! `IpcVaultRepository::connect` 内部で `current_thread` runtime を構築するため、
//! テスト本体はスタブサーバを**独立スレッドの `multi_thread` runtime**で動かす
//! （`current_thread` を本体側でブロッキング保持しても干渉しない）。
//!
//! 対応 Issue: #30 / PR #32

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_cli::io::ipc_vault_repository::IpcVaultRepository;
use shikomi_core::ipc::{
    IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse, MAX_FRAME_LENGTH,
};
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use shikomi_infra::persistence::PersistenceError;
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use uuid::Uuid;

const SECRET_MARKER: &str = "SECRET_TEST_VALUE";

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

fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("fixed time")
}

/// 受信した `IpcRequest` を記録するシェア。
type Recorder = Arc<Mutex<Vec<IpcRequest>>>;

/// ハンドシェイク後 1 回 `IpcRequest` を受信し、固定 `response` を返してから close するスタブを起動。
///
/// 戻り値の `Recorder` で受信内容を観測する。スタブ自身は別スレッドの `multi_thread`
/// runtime で走るため、テスト本体側 `IpcVaultRepository::connect` の `current_thread`
/// runtime と干渉しない。
fn spawn_one_shot_stub(sock_path: PathBuf, response: IpcResponse) -> Recorder {
    let recorder: Recorder = Arc::new(Mutex::new(Vec::new()));
    let recorder_for_thread = Arc::clone(&recorder);
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("server runtime");
        rt.block_on(async move {
            let listener = UnixListener::bind(&sock_path).expect("bind");
            if let Ok((stream, _)) = listener.accept().await {
                let mut framed: Framed<UnixStream, LengthDelimitedCodec> =
                    Framed::new(stream, codec());
                // 1. ハンドシェイク受信 → 成功応答
                if let Some(Ok(_)) = framed.next().await {
                    let ok = IpcResponse::Handshake {
                        // Sub-F (#44) 工程4 Bug-F-004: client (IpcVaultRepository) は
                        // `IpcProtocolVersion::current() == V2` で handshake を要求するため、
                        // mock daemon の応答も V2 に追従する。
                        server_version: IpcProtocolVersion::V2,
                    };
                    let b = rmp_serde::to_vec(&ok).expect("encode handshake");
                    let _ = framed.send(Bytes::from(b)).await;
                }
                // 2. 主リクエスト受信 → 記録 → 固定応答
                if let Some(Ok(bytes)) = framed.next().await {
                    if let Ok(req) = rmp_serde::from_slice::<IpcRequest>(&bytes) {
                        if let Ok(mut rec) = recorder_for_thread.lock() {
                            rec.push(req);
                        }
                    }
                    let b = rmp_serde::to_vec(&response).expect("encode response");
                    let _ = framed.send(Bytes::from(b)).await;
                }
                // 接続を維持（client が drop されるまで）
                let _ = framed.next().await;
            }
        });
    });
    // bind が完了する猶予
    thread::sleep(Duration::from_millis(50));
    recorder
}

fn fixed_record_id() -> RecordId {
    RecordId::new(Uuid::now_v7()).expect("v7")
}

fn build_label(s: &str) -> RecordLabel {
    RecordLabel::try_new(s.to_owned()).expect("valid label")
}

// -------------------------------------------------------------------
// TC-IT-080 / 081 / 082: add_record の正常 + Persistence + Domain
// -------------------------------------------------------------------

#[test]
fn tc_it_080_add_record_success_returns_daemon_id_and_emits_add_request() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Added {
            id: daemon_id.clone(),
        },
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let label = build_label("tc-it-080");
    let value = SecretString::from_string("v-it-080".to_owned());
    let returned = repo
        .add_record(RecordKind::Text, label, value, fixed_time())
        .expect("add_record ok");

    // 戻り値はスタブ送信 id と bit 同一（**id は daemon 集約**、CLI 側で生成しない）
    assert_eq!(returned, daemon_id);

    // 受信 IpcRequest 検証
    let received = recorder.lock().unwrap().clone();
    assert_eq!(
        received.len(),
        1,
        "expected exactly 1 request, got {received:?}"
    );
    match &received[0] {
        IpcRequest::AddRecord {
            kind,
            label: lbl,
            value: _v,
            now,
        } => {
            assert_eq!(*kind, RecordKind::Text);
            // RecordLabel は PartialEq を実装しないため Debug 文字列で比較
            assert!(format!("{lbl:?}").contains("tc-it-080"));
            assert_eq!(*now, fixed_time());
            // SerializableSecretBytes の Debug は [REDACTED] 固定であるべき
            let dbg = format!("{:?}", received[0]);
            assert!(!dbg.contains("v-it-080"), "value leaked in debug: {dbg}");
        }
        other => panic!("expected AddRecord, got {other:?}"),
    }
}

#[test]
fn tc_it_081_add_record_persistence_error_maps_to_internal() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let _recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Error(IpcErrorCode::Persistence {
            reason: "persistence error".to_owned(),
        }),
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let result = repo.add_record(
        RecordKind::Text,
        build_label("tc-it-081"),
        SecretString::from_string("v".to_owned()),
        fixed_time(),
    );
    match result {
        Err(PersistenceError::Internal { reason }) => {
            assert_eq!(reason, "persistence error");
            assert!(!reason.contains(SECRET_MARKER));
            assert!(!reason.contains('/'));
        }
        other => panic!("expected Internal error, got {other:?}"),
    }
}

#[test]
fn tc_it_082_add_record_domain_error_maps_to_internal() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let _recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Error(IpcErrorCode::Domain {
            reason: "duplicate record id".to_owned(),
        }),
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let result = repo.add_record(
        RecordKind::Text,
        build_label("tc-it-082"),
        SecretString::from_string("v".to_owned()),
        fixed_time(),
    );
    match result {
        Err(PersistenceError::Internal { reason }) => {
            assert_eq!(reason, "duplicate record id");
        }
        other => panic!("expected Internal error, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-083 / 084 / 085: edit_record の label-only / value-only / NotFound
// -------------------------------------------------------------------

#[test]
fn tc_it_083_edit_record_label_only_succeeds() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Edited {
            id: daemon_id.clone(),
        },
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let returned = repo
        .edit_record(
            daemon_id.clone(),
            Some(build_label("new")),
            None,
            fixed_time(),
        )
        .expect("edit ok");
    assert_eq!(returned, daemon_id);

    let received = recorder.lock().unwrap().clone();
    match &received[0] {
        IpcRequest::EditRecord {
            id, label, value, ..
        } => {
            assert_eq!(id, &daemon_id);
            assert!(label.is_some(), "label should be Some for label-only edit");
            assert!(value.is_none(), "value should be None for label-only edit");
        }
        other => panic!("expected EditRecord, got {other:?}"),
    }
}

#[test]
fn tc_it_084_edit_record_value_only_succeeds_and_value_redacted() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Edited {
            id: daemon_id.clone(),
        },
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let returned = repo
        .edit_record(
            daemon_id.clone(),
            None,
            Some(SecretString::from_string("new-value-it-084".to_owned())),
            fixed_time(),
        )
        .expect("edit ok");
    assert_eq!(returned, daemon_id);

    let received = recorder.lock().unwrap().clone();
    match &received[0] {
        IpcRequest::EditRecord {
            label,
            value: Some(_v),
            ..
        } => {
            assert!(label.is_none());
            // Debug 出力に value 平文が漏れない
            let dbg = format!("{:?}", received[0]);
            assert!(
                !dbg.contains("new-value-it-084"),
                "edit value leaked in debug: {dbg}"
            );
        }
        other => panic!("expected EditRecord with value Some, got {other:?}"),
    }
}

#[test]
fn tc_it_085_edit_record_not_found_maps_to_record_not_found() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let _recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Error(IpcErrorCode::NotFound {
            id: daemon_id.clone(),
        }),
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let result = repo.edit_record(
        daemon_id.clone(),
        Some(build_label("x")),
        None,
        fixed_time(),
    );
    match result {
        Err(PersistenceError::RecordNotFound(id)) => {
            assert_eq!(id, daemon_id);
        }
        other => panic!("expected RecordNotFound, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-086 / 087 / 088: remove_record の正常 + NotFound + Persistence
// -------------------------------------------------------------------

#[test]
fn tc_it_086_remove_record_success() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Removed {
            id: daemon_id.clone(),
        },
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let returned = repo.remove_record(daemon_id.clone()).expect("remove ok");
    assert_eq!(returned, daemon_id);

    let received = recorder.lock().unwrap().clone();
    match &received[0] {
        IpcRequest::RemoveRecord { id } => assert_eq!(id, &daemon_id),
        other => panic!("expected RemoveRecord, got {other:?}"),
    }
}

#[test]
fn tc_it_087_remove_record_not_found_maps_to_record_not_found() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let _recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Error(IpcErrorCode::NotFound {
            id: daemon_id.clone(),
        }),
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let result = repo.remove_record(daemon_id.clone());
    match result {
        Err(PersistenceError::RecordNotFound(id)) => assert_eq!(id, daemon_id),
        other => panic!("expected RecordNotFound, got {other:?}"),
    }
}

#[test]
fn tc_it_088_remove_record_persistence_error_maps_to_internal() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let _recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Error(IpcErrorCode::Persistence {
            reason: "persistence error".to_owned(),
        }),
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let result = repo.remove_record(daemon_id);
    match result {
        Err(PersistenceError::Internal { reason }) => {
            assert_eq!(reason, "persistence error");
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-089 横串: 全テスト共通 — secret マーカー不在を再確認
// -------------------------------------------------------------------
#[test]
fn tc_it_089_secret_marker_never_appears_in_request_debug() {
    let dir = fresh_socket_dir();
    let sock = dir.path().join("stub.sock");
    let daemon_id = fixed_record_id();
    let recorder = spawn_one_shot_stub(
        sock.clone(),
        IpcResponse::Added {
            id: daemon_id.clone(),
        },
    );

    let repo = IpcVaultRepository::connect(&sock).expect("connect");
    let _ = repo.add_record(
        RecordKind::Secret,
        build_label("tc-it-089"),
        SecretString::from_string(SECRET_MARKER.to_owned()),
        fixed_time(),
    );
    let received = recorder.lock().unwrap().clone();
    let dbg = format!("{received:?}");
    assert!(
        !dbg.contains(SECRET_MARKER),
        "SECRET_MARKER leaked in IpcRequest Debug: {dbg}"
    );
}
