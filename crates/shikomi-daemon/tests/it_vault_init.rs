//! IT — vault.db 不在での daemon 起動（REQ-DAEMON-028）
//!
//! 設計書: docs/features/daemon-ipc/test-design/integration.md §11
//! TC-IT-100〜102 — `SqliteVaultRepository::load_or_create_plaintext` の
//! in-process 結合テスト。
//! 対応シナリオ: SC-DAEMON-001（受入テスト下位レベル）
//! Issue: #80

#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{
    IpcProtocolVersion, IpcRequest, IpcResponse, SerializableSecretBytes, MAX_FRAME_LENGTH,
};
use shikomi_core::{RecordKind, RecordLabel, SecretBytes};
use shikomi_daemon::backoff::UnlockBackoff;
use shikomi_daemon::cache::VekCache;
use shikomi_daemon::ipc::server::IpcServer;
use shikomi_daemon::ipc::transport::ListenerEnum;
use shikomi_daemon::lifecycle::single_instance::SingleInstanceLock;
use shikomi_infra::persistence::SqliteVaultRepository;
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::net::UnixStream;
use tokio::sync::{watch, Mutex};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

// ---------------------------------------------------------------------------
// ヘルパー
// ---------------------------------------------------------------------------

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

/// 0700 の TempDir を生成する。
fn fresh_dir() -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700");
    dir
}

/// Unix stream に接続して Framed を返す。
async fn connect_framed(sock_path: &std::path::Path) -> Framed<UnixStream, LengthDelimitedCodec> {
    let stream = UnixStream::connect(sock_path)
        .await
        .expect("client connect");
    Framed::new(stream, codec())
}

/// ハンドシェイク（V2）を完了させる。
async fn client_handshake(framed: &mut Framed<UnixStream, LengthDelimitedCodec>) {
    let req = IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V2,
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

// ---------------------------------------------------------------------------
// TC-IT-100: vault.db 不在での load_or_create_plaintext — 空 plaintext vault
// 設計書: integration.md §11.1
// REQ-DAEMON-028 / Issue #80
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tc_it_100_vault_absent_load_or_create_plaintext_returns_empty_vault() {
    let dir = fresh_dir();
    let repo = SqliteVaultRepository::from_directory(dir.path()).expect("repo 構築");
    repo.prepare_dir().expect("prepare_dir");
    // vault.db を作成しない

    let result = repo.load_or_create_plaintext();

    let vault = result.expect("vault.db 不在でも Ok (空 vault 生成) が返るべき");
    assert!(
        vault.records().is_empty(),
        "新規生成した vault はレコード 0 件であるべき"
    );
    assert!(
        matches!(
            vault.protection_mode(),
            shikomi_core::ProtectionMode::Plaintext
        ),
        "新規生成した vault は plaintext モードであるべき"
    );
    // vault.db が生成されていること
    assert!(
        dir.path().join("vault.db").exists(),
        "vault.db が SHIKOMI_VAULT_DIR に生成されているべき"
    );
}

// ---------------------------------------------------------------------------
// TC-IT-101: vault.db 不在 → IPC Add → 再 load で永続確認
// 設計書: integration.md §11.1
// REQ-DAEMON-028 / Issue #80
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tc_it_101_vault_absent_ipc_add_then_reload_persists() {
    let vault_dir = fresh_dir();
    let socket_dir = fresh_dir();

    // Step 1-2: vault.db 不在で load_or_create_plaintext
    let repo = SqliteVaultRepository::from_directory(vault_dir.path()).expect("repo 構築");
    repo.prepare_dir().expect("prepare_dir");
    let vault = repo.load_or_create_plaintext().expect("空 vault 生成");

    // Step 3: IpcServer を起動（SingleInstanceLock 経由で実 Unix socket）
    let mut lock = SingleInstanceLock::acquire_unix(socket_dir.path()).expect("acquire_unix");
    let ListenerEnum::Unix {
        listener,
        socket_path,
    } = lock.take_listener().expect("take_listener");

    let repo_arc = Arc::new(repo);
    let vault_arc = Arc::new(Mutex::new(vault));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let cache = VekCache::new();
    let backoff = Arc::new(Mutex::new(UnlockBackoff::new()));
    let hotkey_manager = Arc::new(shikomi_daemon::hotkey::HotkeyManager::new_null());
    let countdown_started_at = Arc::new(Mutex::new(None::<std::time::Instant>));

    let mut server = IpcServer::new(
        ListenerEnum::Unix {
            listener,
            socket_path: socket_path.clone(),
        },
        Arc::clone(&repo_arc),
        Arc::clone(&vault_arc),
        cache,
        backoff,
        hotkey_manager,
        countdown_started_at,
    );
    let server_handle = tokio::spawn(async move {
        let _ = server.start_with_shutdown(shutdown_rx).await;
    });
    // accept loop 開始を待つ
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Step 3-4: クライアント接続 + ハンドシェイク + AddRecord
    let mut framed = connect_framed(&socket_path).await;
    client_handshake(&mut framed).await;

    let add_req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("test-label".into()).expect("label"),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(b"test-value".to_vec())),
        now: fixed_time(),
        hotkey: None,
    };
    send_request(&mut framed, &add_req).await;
    let resp = recv_response(&mut framed).await;
    let added_id = match resp {
        IpcResponse::Added { id } => id,
        other => panic!("expected Added, got {other:?}"),
    };

    drop(framed);

    // サーバを停止
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

    // Step 5-6: 同 repo で再度 load_or_create_plaintext（再起動相当）
    let repo2 = SqliteVaultRepository::from_directory(vault_dir.path()).expect("repo2 構築");
    let vault2 = repo2.load_or_create_plaintext().expect("再起動後ロード");

    // 追加したレコードが永続化されている（vault.db への persistence 確認）
    let records = vault2.records();
    assert!(
        records.iter().any(|r| r.id() == &added_id),
        "IPC AddRecord で追加した id {added_id} が再ロード後も存在するべき"
    );
}

// ---------------------------------------------------------------------------
// TC-IT-102: vault.db 不在時のトレースログ観測
// 設計書: integration.md §11.2
// REQ-DAEMON-028 / Issue #80
// ---------------------------------------------------------------------------

#[tokio::test]
#[tracing_test::traced_test]
async fn tc_it_102_vault_absent_emits_init_log() {
    let dir = fresh_dir();
    let repo = SqliteVaultRepository::from_directory(dir.path()).expect("repo 構築");
    repo.prepare_dir().expect("prepare_dir");
    // vault.db を作成しない

    let _ = repo
        .load_or_create_plaintext()
        .expect("vault.db 不在でも Ok が返るべき");

    // shikomi_daemon::init target の INFO ログを確認
    assert!(
        logs_contain("vault not found; created new plaintext vault at"),
        "INFO ログ 'vault not found; created new plaintext vault at' が出力されるべき"
    );
    assert!(
        logs_contain("hint: to enable encryption"),
        "INFO ログ 'hint: to enable encryption' が続けて出力されるべき"
    );
    // ログに秘密情報が含まれないこと（横串）
    assert!(
        !logs_contain("SECRET_TEST_VALUE"),
        "ログに SECRET_TEST_VALUE が含まれるべきでない"
    );
}
