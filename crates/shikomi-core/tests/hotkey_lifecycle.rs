//! domain sub-feature の結合テスト + プロパティテスト（Issue #89）。
//!
//! 対応テスト設計書: `docs/features/daemon-hotkey-clipboard/domain/test-design.md`
//! - TC-HD-I01: `IpcRequest::AddRecord` ホットキーフィールドの serde ラウンドトリップ
//! - TC-HD-I02: `RecordSummary` ホットキーフィールド伝播
//! - TC-HD-P01: parse → `to_string` → 再 parse 不変条件
//! - TC-HD-P02: `assign_hotkey` → `find_by_hotkey` 不変条件
//!
//! TC-HD-I03（V1→V2 `SQLite` スキーママイグレーション）は shikomi-infra の結合テストとして配置。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use shikomi_core::ipc::RecordSummary;
use shikomi_core::ipc::{IpcRequest, SerializableSecretBytes};
use shikomi_core::secret::{SecretBytes, SecretString};
use shikomi_core::{
    Hotkey, Record, RecordId, RecordKind, RecordLabel, RecordPayload, Vault, VaultHeader,
    VaultVersion,
};
use time::OffsetDateTime;
use uuid::Uuid;

// ── ヘルパ ──────────────────────────────────────────────────────────────────

fn fixed_now() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)
}

fn make_id() -> RecordId {
    RecordId::new(Uuid::now_v7()).unwrap()
}

fn make_plaintext_record_with_hotkey(hotkey_str: &str) -> Record {
    let id = make_id();
    let label = RecordLabel::try_new("test-label".to_owned()).unwrap();
    let payload = RecordPayload::Plaintext(SecretString::from_string("test-value".to_owned()));
    let record = Record::new(id, RecordKind::Text, label, payload, fixed_now());
    let hotkey = Hotkey::parse(hotkey_str).unwrap();
    record.with_hotkey(hotkey)
}

fn empty_secret() -> SerializableSecretBytes {
    SerializableSecretBytes::new(SecretBytes::from_vec(b"test-value".to_vec()))
}

// ── TC-HD-I01: IpcRequest::AddRecord ホットキーフィールドの serde ラウンドトリップ ─

/// TC-HD-I01: `rmp_serde` でシリアライズ → デシリアライズして元の値と一致
#[test]
fn tc_hd_i01_ipc_add_record_hotkey_serde_roundtrip() {
    // IPC AddRecord に hotkey フィールドを含めてシリアライズ→デシリアライズ
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("e2e-entry".to_owned()).unwrap(),
        value: empty_secret(),
        now: fixed_now(),
        hotkey: Some("ctrl+alt+1".to_owned()),
    };

    let bytes = rmp_serde::to_vec(&req).expect("rmp_serde serialize");
    let restored: IpcRequest = rmp_serde::from_slice(&bytes).expect("rmp_serde deserialize");

    // hotkey フィールドが正しく保持されていることを確認
    if let IpcRequest::AddRecord { hotkey, label, .. } = restored {
        assert_eq!(hotkey, Some("ctrl+alt+1".to_owned()));
        assert_eq!(label.as_str(), "e2e-entry");
    } else {
        panic!("expected IpcRequest::AddRecord, got different variant");
    }
}

/// TC-HD-I01 variant: hotkey = None の場合も serde ラウンドトリップ
#[test]
fn tc_hd_i01_ipc_add_record_no_hotkey_roundtrip() {
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: RecordLabel::try_new("no-hotkey".to_owned()).unwrap(),
        value: empty_secret(),
        now: fixed_now(),
        hotkey: None,
    };

    let bytes = rmp_serde::to_vec(&req).expect("serialize");
    let restored: IpcRequest = rmp_serde::from_slice(&bytes).expect("deserialize");

    if let IpcRequest::AddRecord { hotkey, .. } = restored {
        assert_eq!(hotkey, None);
    } else {
        panic!("unexpected variant");
    }
}

// ── TC-HD-I02: RecordSummary ホットキーフィールド伝播 ──────────────────────

/// TC-HD-I02-a: ホットキー付きレコードから `RecordSummary` を生成すると hotkey が伝播する
#[test]
fn tc_hd_i02_a_record_summary_propagates_hotkey() {
    let record = make_plaintext_record_with_hotkey("ctrl+alt+1");
    let summary = RecordSummary::from_record(&record);
    // 正規化済み文字列が伝播する（"ctrl+alt+1" → "alt+ctrl+1" に正規化）
    assert_eq!(summary.hotkey, Some("alt+ctrl+1".to_owned()));
}

/// TC-HD-I02-b: ホットキーなしレコードから `RecordSummary` は hotkey = None
#[test]
fn tc_hd_i02_b_record_summary_no_hotkey_is_none() {
    let id = make_id();
    let label = RecordLabel::try_new("no-hotkey-label".to_owned()).unwrap();
    let payload = RecordPayload::Plaintext(SecretString::from_string("value".to_owned()));
    let record = Record::new(id, RecordKind::Text, label, payload, fixed_now());
    let summary = RecordSummary::from_record(&record);
    assert_eq!(summary.hotkey, None);
}

/// TC-HD-I02-c: `RecordSummary` から `rmp_serde` ラウンドトリップ（hotkey フィールド保持確認）
#[test]
fn tc_hd_i02_c_record_summary_serde_roundtrip_with_hotkey() {
    let record = make_plaintext_record_with_hotkey("alt+ctrl+2");
    let summary = RecordSummary::from_record(&record);
    let bytes = rmp_serde::to_vec(&summary).expect("serialize");
    let restored: RecordSummary = rmp_serde::from_slice(&bytes).expect("deserialize");
    assert_eq!(restored.hotkey, Some("alt+ctrl+2".to_owned()));
}

// ── TC-HD-P01: parse → to_string → 再 parse 不変条件（プロパティテスト）────

use proptest::prelude::*;

/// 有効なコンボ文字列のストラテジ
fn arb_valid_combo() -> impl Strategy<Value = String> {
    // modifier 1〜4 個 + 主キー 1 個の組み合わせ
    let modifiers = prop::sample::subsequence(vec!["alt", "ctrl", "shift", "meta"], 1..=4);
    let main_keys = prop::sample::select(vec!["a", "b", "z", "1", "2", "9", "f1", "f6", "f12"]);
    (modifiers, main_keys).prop_map(|(mods, key)| {
        let mut parts = mods;
        parts.push(key);
        parts.join("+")
    })
}

proptest! {
    /// TC-HD-P01: 有効入力を parse → to_string → 再 parse が同一 Hotkey を返す
    #[test]
    fn tc_hd_p01_parse_roundtrip_idempotent(combo in arb_valid_combo()) {
        let h1 = Hotkey::parse(&combo).expect("valid combo");
        let s = h1.to_string();
        let h2 = Hotkey::parse(&s).expect("re-parse normalized");
        prop_assert_eq!(h1, h2);
    }
}

/// TC-HD-P02: `assign_hotkey` → `find_by_hotkey` 不変条件
///
/// 重複なく登録した場合、`find_by_hotkey` が必ず Some を返す。
#[test]
fn tc_hd_p02_assign_then_find_always_some() {
    let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, fixed_now()).unwrap();
    let mut vault = Vault::new(header);

    let combos = ["ctrl+alt+1", "ctrl+alt+2", "alt+shift+a"];
    let mut ids = Vec::new();

    for (i, combo) in combos.iter().enumerate() {
        let id = make_id();
        let label = RecordLabel::try_new(format!("label-{i}")).unwrap();
        let payload = RecordPayload::Plaintext(SecretString::from_string(format!("val-{i}")));
        let record = Record::new(id.clone(), RecordKind::Text, label, payload, fixed_now());
        vault.add_record(record).unwrap();
        vault
            .assign_hotkey(&id, Hotkey::parse(combo).unwrap())
            .unwrap();
        ids.push((id, *combo));
    }

    // 全て find_by_hotkey で取得できる
    for (id, combo) in &ids {
        let found = vault.find_by_hotkey(&Hotkey::parse(combo).unwrap());
        assert!(found.is_some(), "find_by_hotkey returned None for {combo}");
        assert_eq!(found.unwrap().id(), id);
    }
}
