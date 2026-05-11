//! `HotkeyEventLoop` 結合テスト（TC-HD-DI01〜TC-HD-DI03）。
//!
//! MockBackend + MockClipboardWriter + MockNotifier を使ってホットキーイベントループの
//! 主要経路を検証する。OS API（クリップボード・通知）は差し替え済みのためヘッドレス環境でも動作。
//!
//! 設計根拠: `docs/features/daemon-hotkey-clipboard/daemon/test-design.md §5`
//! 対応 Issue: #89

mod common;

use std::sync::Arc;
use std::time::Duration;

use shikomi_core::secret::SecretString;
use shikomi_core::{
    Aad, AuthTag, CipherText, Hotkey, KdfSalt, NonceBytes, Record, RecordId, RecordKind,
    RecordLabel, RecordPayload, RecordPayloadEncrypted, Vault, VaultHeader, VaultVersion,
    WrappedVek,
};
use shikomi_daemon::cache::VekCache;
use shikomi_daemon::hotkey::backend::{BackendEnum, HotkeyEvent, MockBackend};
use shikomi_daemon::hotkey::clipboard::ClipboardWriter;
use shikomi_daemon::hotkey::event_loop::HotkeyEventLoop;
use shikomi_daemon::hotkey::notifier::Notifier;
use time::OffsetDateTime;
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use common::mock_clipboard::MockClipboardWriter;
use common::mock_notifier::MockNotifier;

// -------------------------------------------------------------------
// ヘルパ
// -------------------------------------------------------------------

fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
}

fn make_id() -> RecordId {
    RecordId::new(Uuid::now_v7()).unwrap()
}

fn empty_plaintext_vault() -> Vault {
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_time()).unwrap();
    Vault::new(header)
}

/// plaintext vault に Text エントリ + ホットキーを追加して (vault, RecordId, 正規化コンボ) を返す。
fn plaintext_vault_with_text_entry(combo: &str, value: &str) -> (Vault, RecordId, String) {
    let mut vault = empty_plaintext_vault();
    let id = make_id();
    let record = Record::new(
        id.clone(),
        RecordKind::Text,
        RecordLabel::try_new(format!("label-{value}")).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string(value.to_owned())),
        fixed_time(),
    );
    vault.add_record(record).unwrap();
    let hotkey = Hotkey::parse(combo).unwrap();
    let normalized_combo = hotkey.as_str().to_owned();
    vault.assign_hotkey(&id, hotkey).unwrap();
    (vault, id, normalized_combo)
}

/// plaintext vault に Secret エントリ + ホットキーを追加して (vault, RecordId, 正規化コンボ) を返す。
fn plaintext_vault_with_secret_entry(combo: &str, value: &str) -> (Vault, RecordId, String) {
    let mut vault = empty_plaintext_vault();
    let id = make_id();
    let record = Record::new(
        id.clone(),
        RecordKind::Secret,
        RecordLabel::try_new("secret-label".to_owned()).unwrap(),
        RecordPayload::Plaintext(SecretString::from_string(value.to_owned())),
        fixed_time(),
    );
    vault.add_record(record).unwrap();
    let hotkey = Hotkey::parse(combo).unwrap();
    let normalized_combo = hotkey.as_str().to_owned();
    vault.assign_hotkey(&id, hotkey).unwrap();
    (vault, id, normalized_combo)
}

/// encrypted vault に encrypted レコード + ホットキーを追加して (vault, RecordId, 正規化コンボ) を返す。
fn encrypted_vault_with_encrypted_entry(combo: &str) -> (Vault, RecordId, String) {
    // ダミー暗号化ヘッダ（Sub-E テスト用、実際の VEK は設定しない）
    let salt = KdfSalt::try_new(&[0u8; 16]).unwrap();
    let wrapped_vek = WrappedVek::new(
        vec![0u8; 32],
        NonceBytes::from_random([0u8; 12]),
        AuthTag::from_array([0u8; 16]),
    )
    .unwrap();
    let header = VaultHeader::new_encrypted(
        VaultVersion::CURRENT,
        fixed_time(),
        salt,
        wrapped_vek.clone(),
        wrapped_vek,
    )
    .unwrap();
    let mut vault = Vault::new(header);

    let id = make_id();
    // encrypted vault は RecordPayload::Encrypted が必要（ProtectionMode::Encrypted）
    let nonce = NonceBytes::try_new(&[1u8; 12]).unwrap();
    let cipher = CipherText::try_new(vec![2u8; 32].into_boxed_slice()).unwrap();
    let aad = Aad::new(id.clone(), VaultVersion::CURRENT, fixed_time()).unwrap();
    let enc = RecordPayloadEncrypted::new(nonce, cipher, aad).unwrap();
    let record = Record::new(
        id.clone(),
        RecordKind::Secret,
        RecordLabel::try_new("enc-label".to_owned()).unwrap(),
        RecordPayload::Encrypted(enc),
        fixed_time(),
    );
    vault.add_record(record).unwrap();
    let hotkey = Hotkey::parse(combo).unwrap();
    let normalized_combo = hotkey.as_str().to_owned();
    vault.assign_hotkey(&id, hotkey).unwrap();
    (vault, id, normalized_combo)
}

/// `HotkeyEventLoop` を起動して `(shutdown_tx, task_handle, event_tx)` を返す汎用ヘルパ。
fn spawn_event_loop(
    vault: Vault,
    vek_cache: VekCache,
    clipboard: Arc<Mutex<dyn ClipboardWriter + Send>>,
    notifier: Arc<dyn Notifier>,
) -> (
    watch::Sender<bool>,
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::Sender<HotkeyEvent>,
) {
    let (mock_backend, event_tx) = MockBackend::new_with_sender();
    let backend = Arc::new(BackendEnum::Mock(mock_backend));
    let vault_arc = Arc::new(Mutex::new(vault));

    let event_loop = HotkeyEventLoop::new(
        Arc::clone(&backend),
        vault_arc,
        vek_cache,
        clipboard,
        notifier,
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(async move { event_loop.run(shutdown_rx).await });
    (shutdown_tx, task, event_tx)
}

// -------------------------------------------------------------------
// TC-HD-DI01: HotkeyEventLoop — ホットキー → クリップボード書き込み
// -------------------------------------------------------------------

/// TC-HD-DI01: ctrl+alt+1 のホットキーイベントを受信すると MockClipboard に "hello" が書き込まれる。
#[tokio::test]
async fn tc_hd_di01_hotkey_event_writes_clipboard() {
    let (vault, _id, normalized_combo) = plaintext_vault_with_text_entry("ctrl+alt+1", "hello");

    let mock_cb = Arc::new(Mutex::new(MockClipboardWriter::new()));
    let clipboard: Arc<Mutex<dyn ClipboardWriter + Send>> =
        Arc::clone(&mock_cb) as Arc<Mutex<dyn ClipboardWriter + Send>>;
    let notifier: Arc<dyn Notifier> = Arc::new(MockNotifier::new());

    let (shutdown_tx, _task, event_tx) =
        spawn_event_loop(vault, VekCache::new(), clipboard, Arc::clone(&notifier));

    // ホットキーイベントを注入（正規化済みコンボ）
    event_tx
        .send(HotkeyEvent {
            combo: normalized_combo,
        })
        .await
        .unwrap();

    // イベント処理を待機
    tokio::time::sleep(Duration::from_millis(50)).await;

    // クリップボードに "hello" が書き込まれたことを確認
    let cb = mock_cb.lock().await;
    assert_eq!(
        cb.current(),
        Some("hello"),
        "clipboard should contain 'hello' after hotkey event"
    );

    let _ = shutdown_tx.send(true);
}

// -------------------------------------------------------------------
// TC-HD-DI02: ロック中暗号化 vault でのホットキーイベントはスキップ + 通知
// -------------------------------------------------------------------

/// TC-HD-DI02: VekCache がロック中の暗号化 vault でホットキーイベントを受信すると
/// クリップボードへの書き込みはスキップされ、OS 通知が発火する。
#[tokio::test]
async fn tc_hd_di02_locked_encrypted_vault_skips_hotkey_and_notifies() {
    // 暗号化 vault + VekCache デフォルト（Locked 状態）
    let (vault, _id, normalized_combo) = encrypted_vault_with_encrypted_entry("ctrl+alt+5");

    let mock_cb = Arc::new(Mutex::new(MockClipboardWriter::new()));
    let clipboard: Arc<Mutex<dyn ClipboardWriter + Send>> =
        Arc::clone(&mock_cb) as Arc<Mutex<dyn ClipboardWriter + Send>>;
    let mock_notifier = Arc::new(MockNotifier::new());
    let notifier_clone = Arc::clone(&mock_notifier);
    let notifier: Arc<dyn Notifier> = mock_notifier;

    // VekCache はデフォルトで Locked 状態
    let (shutdown_tx, _task, event_tx) =
        spawn_event_loop(vault, VekCache::new(), clipboard, notifier);

    // ホットキーイベントを注入
    event_tx
        .send(HotkeyEvent {
            combo: normalized_combo,
        })
        .await
        .unwrap();

    // イベント処理を待機
    tokio::time::sleep(Duration::from_millis(50)).await;

    // クリップボードは変化なし（write されていない）
    let cb = mock_cb.lock().await;
    assert!(
        cb.history.is_empty(),
        "clipboard should not be written when vault is locked"
    );
    drop(cb);

    // OS 通知が 1 件発火されているはず（vault locked 通知）
    assert_eq!(
        notifier_clone.len(),
        1,
        "one OS notification should be sent when vault is locked"
    );

    let _ = shutdown_tx.send(true);
}

// -------------------------------------------------------------------
// TC-HD-DI03: Secret エントリで ClearTimer が起動する
// -------------------------------------------------------------------

/// TC-HD-DI03: RecordKind::Secret エントリのホットキーイベントを受信すると
/// クリップボードへの書き込み後 30 秒で自動クリアされる。
#[tokio::test(start_paused = true)]
async fn tc_hd_di03_secret_entry_schedules_clear_timer() {
    let (vault, _id, normalized_combo) =
        plaintext_vault_with_secret_entry("ctrl+alt+2", "secret_val");

    let mock_cb = Arc::new(Mutex::new(MockClipboardWriter::new()));
    let clipboard: Arc<Mutex<dyn ClipboardWriter + Send>> =
        Arc::clone(&mock_cb) as Arc<Mutex<dyn ClipboardWriter + Send>>;
    let notifier: Arc<dyn Notifier> = Arc::new(MockNotifier::new());

    let (shutdown_tx, _task, event_tx) =
        spawn_event_loop(vault, VekCache::new(), clipboard, notifier);

    // ホットキーイベントを注入
    event_tx
        .send(HotkeyEvent {
            combo: normalized_combo,
        })
        .await
        .unwrap();

    // イベント処理を待機（paused 状態でも tokio::time::sleep は advance で進む）
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 書き込みが 1 件発生していること（Secret でも write は呼ばれる）
    {
        let cb = mock_cb.lock().await;
        assert_eq!(
            cb.history.len(),
            1,
            "clipboard should have one write before clear timer fires"
        );
    }

    // 31 秒進める → ClearTimer が発火して clear() が呼ばれるはず
    tokio::time::sleep(Duration::from_secs(31)).await;

    // clear() が 1 回呼ばれていることを確認
    let cb = mock_cb.lock().await;
    assert_eq!(
        cb.cleared_count, 1,
        "clipboard should be cleared once after 30s (ClearTimer)"
    );
    // history には write 1 件 + clear 1 件で合計 2 件
    assert_eq!(
        cb.history.len(),
        2,
        "clipboard history should have 2 entries: write + clear"
    );

    let _ = shutdown_tx.send(true);
}
