//! IPC ホットキーハンドラ結合テスト（TC-HD-DI04〜TC-HD-DI09）。
//!
//! IpcServer + MockBackend を使って IPC 経由のホットキー登録・競合・クリア・解除・Fail Fast 経路を検証する。
//! Unix UDS ソケットで実際の IPC を行うため Unix 専用 (`#[cfg(unix)]`)。
//!
//! 設計根拠: `docs/features/daemon-hotkey-clipboard/daemon/test-design.md §5`
//! 対応 Issue: #89

#[cfg(unix)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use bytes::Bytes;
    use futures_util::{SinkExt, StreamExt};
    use shikomi_core::ipc::{
        IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse, SerializableSecretBytes,
        MAX_FRAME_LENGTH,
    };
    use shikomi_core::{RecordKind, RecordLabel, SecretBytes, Vault, VaultHeader, VaultVersion};
    use shikomi_daemon::backoff::UnlockBackoff;
    use shikomi_daemon::cache::VekCache;
    use shikomi_daemon::hotkey::backend::{BackendEnum, MockBackend};
    use shikomi_daemon::hotkey::notifier::NullNotifier;
    use shikomi_daemon::hotkey::HotkeyManager;
    use shikomi_daemon::ipc::server::IpcServer;
    use shikomi_daemon::ipc::transport::ListenerEnum;
    use shikomi_daemon::lifecycle::single_instance::SingleInstanceLock;
    use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use tokio::net::UnixStream;
    use tokio::sync::{watch, Mutex};
    use tokio_util::codec::{Framed, LengthDelimitedCodec};

    // -------------------------------------------------------------------
    // ヘルパ
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

    fn fresh_socket_dir() -> TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().expect("tempdir");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod 0700");
        dir
    }

    fn fresh_vault_and_repo(
        dir: &std::path::Path,
    ) -> (Arc<Mutex<Vault>>, Arc<SqliteVaultRepository>) {
        let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_time()).unwrap();
        let vault = Vault::new(header);
        let repo = SqliteVaultRepository::from_directory(dir).expect("repo");
        repo.save(&vault).expect("initial save");
        (Arc::new(Mutex::new(vault)), Arc::new(repo))
    }

    async fn connect_framed(
        sock_path: &std::path::Path,
    ) -> Framed<UnixStream, LengthDelimitedCodec> {
        let stream = UnixStream::connect(sock_path)
            .await
            .expect("client connect");
        Framed::new(stream, codec())
    }

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
        let resp: IpcResponse = rmp_serde::from_slice(&received).expect("decode resp");
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

    fn empty_secret_bytes() -> SerializableSecretBytes {
        SerializableSecretBytes::new(SecretBytes::from_vec(b"value".to_vec()))
    }

    struct TestServerHandle {
        socket_path: std::path::PathBuf,
        _lock: SingleInstanceLock,
        shutdown: watch::Sender<bool>,
        server_handle: tokio::task::JoinHandle<()>,
        /// MockBackend への参照（OS ホットキー登録状態を検証するため）。
        mock_backend: Arc<BackendEnum>,
    }

    impl TestServerHandle {
        async fn shutdown_and_join(self) {
            let _ = self.shutdown.send(true);
            let _ = tokio::time::timeout(Duration::from_secs(5), self.server_handle).await;
        }

        /// 登録済みホットキーの一覧を取得する。
        fn registered_hotkeys(&self) -> std::collections::HashSet<String> {
            match self.mock_backend.as_ref() {
                BackendEnum::Mock(m) => m.registered(),
                _ => panic!("expected MockBackend"),
            }
        }

        /// 指定コンボの OS 登録を強制失敗させる（Fail Fast テスト用）。
        /// `combo` は正規化済み形式で渡すこと（e.g. `"alt+ctrl+3"`）。
        fn set_fail_on_register(&self, normalized_combo: &str) {
            match self.mock_backend.as_ref() {
                BackendEnum::Mock(m) => m.set_fail_on_register(normalized_combo),
                _ => panic!("expected MockBackend"),
            }
        }
    }

    /// MockBackend を持つ IpcServer を起動して `TestServerHandle` を返す。
    async fn spawn_test_server_with_mock_backend(dir: &TempDir) -> TestServerHandle {
        let (vault_arc, repo_arc) = fresh_vault_and_repo(dir.path());

        let mut lock = SingleInstanceLock::acquire_unix(dir.path()).expect("acquire_unix");
        let ListenerEnum::Unix {
            listener,
            socket_path,
        } = lock.take_listener().expect("take_listener");

        let (mock_backend_raw, _event_tx) = MockBackend::new_with_sender();
        let backend = Arc::new(BackendEnum::Mock(mock_backend_raw));
        let notifier = Arc::new(NullNotifier);

        let hotkey_manager = Arc::new(HotkeyManager::new(
            Arc::clone(&backend),
            &vault_arc.try_lock().expect("vault not yet shared"),
            notifier as Arc<dyn shikomi_daemon::hotkey::notifier::Notifier>,
        ));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let cache = VekCache::new();
        let backoff = Arc::new(Mutex::new(UnlockBackoff::new()));

        // Sub-D: countdown_started_at はテスト用ダミー（GetClipboardStatus IT では使用しない）
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
            Arc::clone(&hotkey_manager),
            countdown_started_at,
        );
        let server_handle = tokio::spawn(async move {
            let _ = server.start_with_shutdown(shutdown_rx).await;
        });

        // accept loop の開始を待機
        tokio::time::sleep(Duration::from_millis(30)).await;

        TestServerHandle {
            socket_path,
            _lock: lock,
            shutdown: shutdown_tx,
            server_handle,
            mock_backend: backend,
        }
    }

    // -------------------------------------------------------------------
    // TC-HD-DI04: IPC AddRecord でホットキーが vault と OS に正規化登録される
    // -------------------------------------------------------------------

    /// TC-HD-DI04: `AddRecord { hotkey: "ctrl+alt+1", ... }` を送信すると
    /// `HotkeyManager::register_one` が `Hotkey::parse` で正規化し、
    /// MockBackend に `"alt+ctrl+1"`（正規化後: alt→ctrl 辞書順）が登録される（SEC-003 / INFO-001 修正）。
    #[tokio::test]
    async fn tc_hd_di04_add_record_with_hotkey_registers_os_hotkey() {
        let dir = fresh_socket_dir();
        let handle = spawn_test_server_with_mock_backend(&dir).await;
        let mut framed = connect_framed(&handle.socket_path).await;
        client_handshake(&mut framed).await;

        let add_req = IpcRequest::AddRecord {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("hotkey-entry".to_owned()).unwrap(),
            value: empty_secret_bytes(),
            now: fixed_time(),
            hotkey: Some("ctrl+alt+1".to_owned()),
        };
        send_request(&mut framed, &add_req).await;
        let resp = recv_response(&mut framed).await;

        match resp {
            IpcResponse::Added { .. } => {}
            other => panic!("expected Added response, got {other:?}"),
        }

        // MockBackend には正規化済みコンボ "alt+ctrl+1" が登録される。
        // HotkeyManager::register_one は Hotkey::parse で正規化してから OS 登録するため、
        // raw の "ctrl+alt+1" ではなく辞書順正規化後の "alt+ctrl+1" が MockBackend に入る。
        let registered = handle.registered_hotkeys();
        assert!(
            registered.contains("alt+ctrl+1"),
            "OS hotkey should be normalized to 'alt+ctrl+1' after AddRecord, got: {registered:?}"
        );

        drop(framed);
        handle.shutdown_and_join().await;
    }

    // -------------------------------------------------------------------
    // TC-HD-DI05: IPC AddRecord でホットキー競合は HotkeyConflict を返す
    // -------------------------------------------------------------------

    /// TC-HD-DI05: 先に `ctrl+alt+1` を別エントリに割り当てた状態で
    /// 同じホットキーで `AddRecord` すると `IpcErrorCode::HotkeyConflict` が返る。
    #[tokio::test]
    async fn tc_hd_di05_add_record_hotkey_conflict_returns_error() {
        let dir = fresh_socket_dir();
        let handle = spawn_test_server_with_mock_backend(&dir).await;
        let mut framed = connect_framed(&handle.socket_path).await;
        client_handshake(&mut framed).await;

        // 1 件目: ctrl+alt+1 を登録
        let add_first = IpcRequest::AddRecord {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("first".to_owned()).unwrap(),
            value: empty_secret_bytes(),
            now: fixed_time(),
            hotkey: Some("ctrl+alt+1".to_owned()),
        };
        send_request(&mut framed, &add_first).await;
        let first_resp = recv_response(&mut framed).await;
        assert!(
            matches!(first_resp, IpcResponse::Added { .. }),
            "first AddRecord should succeed"
        );

        // 2 件目: 同じホットキーで競合
        let add_conflict = IpcRequest::AddRecord {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("conflict".to_owned()).unwrap(),
            value: empty_secret_bytes(),
            now: fixed_time() + time::Duration::seconds(1),
            hotkey: Some("ctrl+alt+1".to_owned()),
        };
        send_request(&mut framed, &add_conflict).await;
        let conflict_resp = recv_response(&mut framed).await;

        assert!(
            matches!(
                conflict_resp,
                IpcResponse::Error(IpcErrorCode::HotkeyConflict { .. })
            ),
            "duplicate hotkey should return HotkeyConflict, got: {conflict_resp:?}"
        );

        drop(framed);
        handle.shutdown_and_join().await;
    }

    // -------------------------------------------------------------------
    // TC-HD-DI06: IPC EditRecord で clear_hotkey + hotkey 同時指定は拒否
    // -------------------------------------------------------------------

    /// TC-HD-DI06: `EditRecord { hotkey: "ctrl+alt+2", clear_hotkey: true, ... }` を送信すると
    /// `IpcErrorCode::HotkeyParseError` が返る（矛盾入力の拒否）。
    #[tokio::test]
    async fn tc_hd_di06_edit_record_clear_and_hotkey_together_returns_error() {
        let dir = fresh_socket_dir();
        let handle = spawn_test_server_with_mock_backend(&dir).await;
        let mut framed = connect_framed(&handle.socket_path).await;
        client_handshake(&mut framed).await;

        // まずエントリを追加
        let add = IpcRequest::AddRecord {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("edit-target".to_owned()).unwrap(),
            value: empty_secret_bytes(),
            now: fixed_time(),
            hotkey: None,
        };
        send_request(&mut framed, &add).await;
        let add_resp = recv_response(&mut framed).await;
        let record_id = match add_resp {
            IpcResponse::Added { id } => id,
            other => panic!("expected Added, got {other:?}"),
        };

        // clear_hotkey + hotkey を同時指定（矛盾）
        let edit = IpcRequest::EditRecord {
            id: record_id,
            label: None,
            value: None,
            now: fixed_time() + time::Duration::seconds(1),
            hotkey: Some("ctrl+alt+2".to_owned()),
            clear_hotkey: true,
        };
        send_request(&mut framed, &edit).await;
        let edit_resp = recv_response(&mut framed).await;

        assert!(
            matches!(
                edit_resp,
                IpcResponse::Error(IpcErrorCode::HotkeyParseError { .. })
            ),
            "conflicting hotkey+clear_hotkey should return HotkeyParseError, got: {edit_resp:?}"
        );

        drop(framed);
        handle.shutdown_and_join().await;
    }

    // -------------------------------------------------------------------
    // TC-HD-DI08: IPC RemoveRecord 後に OS ホットキーが解除される（SEC-002）
    // -------------------------------------------------------------------

    /// TC-HD-DI08: `ctrl+alt+1` を割り当てたエントリを `RemoveRecord` すると
    /// MockBackend から `"alt+ctrl+1"`（正規化コンボ）が解除される。
    #[tokio::test]
    async fn tc_hd_di08_remove_record_unregisters_os_hotkey() {
        let dir = fresh_socket_dir();
        let handle = spawn_test_server_with_mock_backend(&dir).await;
        let mut framed = connect_framed(&handle.socket_path).await;
        client_handshake(&mut framed).await;

        // 1. ホットキー付きでエントリを追加
        let add_req = IpcRequest::AddRecord {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("remove-target".to_owned()).unwrap(),
            value: empty_secret_bytes(),
            now: fixed_time(),
            hotkey: Some("ctrl+alt+1".to_owned()),
        };
        send_request(&mut framed, &add_req).await;
        let add_resp = recv_response(&mut framed).await;
        let record_id = match add_resp {
            IpcResponse::Added { id } => id,
            other => panic!("expected Added, got {other:?}"),
        };

        // OS に "alt+ctrl+1" が登録されていること（正規化済み）
        let registered_before = handle.registered_hotkeys();
        assert!(
            registered_before.contains("alt+ctrl+1"),
            "hotkey should be registered before RemoveRecord, got: {registered_before:?}"
        );

        // 2. RemoveRecord
        let remove_req = IpcRequest::RemoveRecord { id: record_id };
        send_request(&mut framed, &remove_req).await;
        let remove_resp = recv_response(&mut framed).await;
        assert!(
            matches!(remove_resp, IpcResponse::Removed { .. }),
            "expected Removed, got {remove_resp:?}"
        );

        // OS からホットキーが解除されていること（SEC-002 修正検証）
        let registered_after = handle.registered_hotkeys();
        assert!(
            !registered_after.contains("alt+ctrl+1"),
            "OS hotkey 'alt+ctrl+1' should be unregistered after RemoveRecord, got: {registered_after:?}"
        );

        drop(framed);
        handle.shutdown_and_join().await;
    }

    // -------------------------------------------------------------------
    // TC-HD-DI09: OS 登録失敗時は IPC が HotkeyConflict を返す（Fail Fast / P1-②）
    // -------------------------------------------------------------------

    /// TC-HD-DI09: vault ドメインの `AddRecord` は成功するが MockBackend の OS 登録が失敗する場合、
    /// IPC は `IpcErrorCode::HotkeyConflict` を返す（Fail Fast: vault と OS の不整合を防ぐ）。
    #[tokio::test]
    async fn tc_hd_di09_os_register_failure_returns_hotkey_conflict() {
        let dir = fresh_socket_dir();
        let handle = spawn_test_server_with_mock_backend(&dir).await;
        let mut framed = connect_framed(&handle.socket_path).await;
        client_handshake(&mut framed).await;

        // MockBackend に "alt+ctrl+5"（正規化済み）の登録を強制失敗させる
        handle.set_fail_on_register("alt+ctrl+5");

        // vault ドメインとしては初出コンボなので競合なし → domain は Ok だが OS が失敗する
        let add_req = IpcRequest::AddRecord {
            kind: RecordKind::Text,
            label: RecordLabel::try_new("fail-target".to_owned()).unwrap(),
            value: empty_secret_bytes(),
            now: fixed_time(),
            hotkey: Some("ctrl+alt+5".to_owned()),
        };
        send_request(&mut framed, &add_req).await;
        let resp = recv_response(&mut framed).await;

        // Fail Fast: OS 登録失敗は HotkeyConflict として返す（IpcResponse::Added ではない）
        assert!(
            matches!(
                resp,
                IpcResponse::Error(IpcErrorCode::HotkeyConflict { .. })
            ),
            "OS registration failure should return HotkeyConflict (Fail Fast), got: {resp:?}"
        );

        drop(framed);
        handle.shutdown_and_join().await;
    }
}
