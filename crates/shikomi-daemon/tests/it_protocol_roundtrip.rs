//! IPC プロトコル round-trip IT — test-design/integration.md §3 TC-IT-001〜009。
//!
//! `rmp-serde` による MessagePack シリアライズと、`LengthDelimitedCodec` による
//! フレーミング境界を跨いだ end-to-end 経路を検証する。
//!
//! 対応 Issue: #26

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{
    IpcErrorCode, IpcProtocolVersion, IpcRequest, IpcResponse, RecordSummary,
    SerializableSecretBytes, MAX_FRAME_LENGTH,
};
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretBytes};
use time::OffsetDateTime;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use uuid::Uuid;

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

fn text_label(s: &str) -> RecordLabel {
    RecordLabel::try_new(s.to_owned()).unwrap()
}

fn record_id() -> RecordId {
    RecordId::new(Uuid::now_v7()).unwrap()
}

// -------------------------------------------------------------------
// TC-IT-001: Handshake V1 の round-trip
// -------------------------------------------------------------------
#[test]
fn tc_it_001_roundtrip_handshake_v1() {
    let req = IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V1,
    };
    let bytes = rmp_serde::to_vec(&req).unwrap();
    let decoded: IpcRequest = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcRequest::Handshake { client_version } => {
            assert_eq!(client_version, IpcProtocolVersion::V1)
        }
        other => panic!("expected Handshake, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-002: ListRecords unit variant の round-trip
// -------------------------------------------------------------------
#[test]
fn tc_it_002_roundtrip_list_records() {
    let req = IpcRequest::ListRecords;
    let bytes = rmp_serde::to_vec(&req).unwrap();
    let decoded: IpcRequest = rmp_serde::from_slice(&bytes).unwrap();
    assert!(matches!(decoded, IpcRequest::ListRecords));
}

// -------------------------------------------------------------------
// TC-IT-003: AddRecord(Text) の round-trip — value バイト列が一致
// -------------------------------------------------------------------
#[test]
fn tc_it_003_roundtrip_add_record_text_preserves_value_bytes() {
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Text,
        label: text_label("L"),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(b"v".to_vec())),
        now: fixed_time(),
    };
    let bytes = rmp_serde::to_vec(&req).unwrap();
    let decoded: IpcRequest = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcRequest::AddRecord {
            kind,
            label,
            value,
            now,
        } => {
            assert_eq!(kind, RecordKind::Text);
            assert_eq!(label.as_str(), "L");
            assert_eq!(now, fixed_time());
            // tests/ 配下は audit-secret-paths.sh の対象外（crates/shikomi-daemon/src/ 限定）
            assert_eq!(value.inner().expose_secret(), b"v");
        }
        other => panic!("expected AddRecord, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-004: AddRecord(Secret) の round-trip — 平文 ASCII が MessagePack バイト列に
// 出現するが、Debug/Display 経由では漏れない（§脅威モデル整合）
// -------------------------------------------------------------------
#[test]
fn tc_it_004_secret_bytes_are_carried_but_redacted_in_debug() {
    let req = IpcRequest::AddRecord {
        kind: RecordKind::Secret,
        label: text_label("api-key"),
        value: SerializableSecretBytes::new(SecretBytes::from_vec(b"SECRET_TEST_VALUE".to_vec())),
        now: fixed_time(),
    };
    let bytes = rmp_serde::to_vec(&req).unwrap();
    // wire 上では平文 ASCII が出現する（OS プロセス境界 + UDS 0600 で保護される前提）
    assert!(bytes.windows(17).any(|w| w == b"SECRET_TEST_VALUE"));

    // しかし Debug 経由では `[REDACTED]` に置換される
    let debug_str = format!("{req:?}");
    assert!(!debug_str.contains("SECRET_TEST_VALUE"));
    assert!(debug_str.to_uppercase().contains("REDACTED"));

    // round-trip しても内容は保持
    let decoded: IpcRequest = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcRequest::AddRecord { value, .. } => {
            // tests/ 配下は audit-secret-paths.sh の対象外（crates/shikomi-daemon/src/ 限定）
            assert_eq!(value.inner().expose_secret(), b"SECRET_TEST_VALUE");
        }
        other => panic!("expected AddRecord, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-005: IpcResponse::Records(Vec<RecordSummary>) の round-trip
// -------------------------------------------------------------------
#[test]
fn tc_it_005_roundtrip_records_response_preserves_projections() {
    let id1 = record_id();
    let id2 = record_id();
    let id3 = record_id();
    let summaries = vec![
        RecordSummary {
            id: id1,
            kind: RecordKind::Text,
            label: text_label("t1"),
            value_preview: Some("hello".to_owned()),
            value_masked: false,
        },
        RecordSummary {
            id: id2,
            kind: RecordKind::Secret,
            label: text_label("s1"),
            value_preview: None,
            value_masked: true,
        },
        RecordSummary {
            id: id3,
            kind: RecordKind::Text,
            label: text_label("t2"),
            value_preview: Some("world".to_owned()),
            value_masked: false,
        },
    ];
    let resp = IpcResponse::Records(summaries.clone());
    let bytes = rmp_serde::to_vec(&resp).unwrap();
    let decoded: IpcResponse = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcResponse::Records(got) => {
            assert_eq!(got.len(), 3);
            assert_eq!(got[1].value_preview, None);
            assert!(got[1].value_masked);
            assert_eq!(got[0].value_preview.as_deref(), Some("hello"));
        }
        other => panic!("expected Records, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-006: ProtocolVersionMismatch round-trip（両者 V1 でも型として round-trip 可）
// -------------------------------------------------------------------
#[test]
fn tc_it_006_roundtrip_protocol_version_mismatch() {
    let resp = IpcResponse::ProtocolVersionMismatch {
        server: IpcProtocolVersion::V1,
        client: IpcProtocolVersion::V1,
    };
    let bytes = rmp_serde::to_vec(&resp).unwrap();
    let decoded: IpcResponse = rmp_serde::from_slice(&bytes).unwrap();
    assert!(matches!(
        decoded,
        IpcResponse::ProtocolVersionMismatch { .. }
    ));
}

// -------------------------------------------------------------------
// TC-IT-006b: 未知 version 文字列 → `IpcProtocolVersion::Unknown` 吸収
//
// **BUG-DAEMON-IPC-001 retest**: `#[serde(other)]` フォールバックが
// rmp-serde の wire 経路でも有効であることを確認する。これにより daemon は
// decode 失敗で応答なし切断するのではなく、`ProtocolVersionMismatch` 応答を
// 返してから切断できる（fail-secure + diagnostics 両立）。
// -------------------------------------------------------------------
#[test]
fn tc_it_006b_unknown_version_string_decodes_as_unknown_variant() {
    // 正常 V1 を MessagePack 化してから "v1" → "v9" に書換える
    let req = IpcRequest::Handshake {
        client_version: IpcProtocolVersion::V1,
    };
    let mut bytes = rmp_serde::to_vec(&req).unwrap();
    let pos = bytes
        .windows(2)
        .position(|w| w == b"v1")
        .expect("v1 bytes present");
    bytes[pos + 1] = b'9';

    let decoded: IpcRequest = rmp_serde::from_slice(&bytes).expect("decode v9 yields Unknown");
    match decoded {
        IpcRequest::Handshake { client_version } => {
            assert_eq!(client_version, IpcProtocolVersion::Unknown);
        }
        other => panic!("expected Handshake, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-006c: `ProtocolVersionMismatch { client: Unknown }` の wire round-trip
// -------------------------------------------------------------------
#[test]
fn tc_it_006c_roundtrip_protocol_version_mismatch_with_unknown_client() {
    let resp = IpcResponse::ProtocolVersionMismatch {
        server: IpcProtocolVersion::V1,
        client: IpcProtocolVersion::Unknown,
    };
    let bytes = rmp_serde::to_vec(&resp).unwrap();
    let decoded: IpcResponse = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcResponse::ProtocolVersionMismatch { server, client } => {
            assert_eq!(server, IpcProtocolVersion::V1);
            assert_eq!(client, IpcProtocolVersion::Unknown);
        }
        other => panic!("expected ProtocolVersionMismatch, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-007: IpcErrorCode::NotFound round-trip
// -------------------------------------------------------------------
#[test]
fn tc_it_007_roundtrip_error_notfound_preserves_id() {
    let id = record_id();
    let resp = IpcResponse::Error(IpcErrorCode::NotFound { id: id.clone() });
    let bytes = rmp_serde::to_vec(&resp).unwrap();
    let decoded: IpcResponse = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcResponse::Error(IpcErrorCode::NotFound { id: got }) => assert_eq!(got, id),
        other => panic!("expected Error::NotFound, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-008: IpcErrorCode::Persistence の reason 文字列維持
// -------------------------------------------------------------------
#[test]
fn tc_it_008_roundtrip_error_persistence_preserves_reason() {
    let resp = IpcResponse::Error(IpcErrorCode::Persistence {
        reason: "persistence error".to_owned(),
    });
    let bytes = rmp_serde::to_vec(&resp).unwrap();
    let decoded: IpcResponse = rmp_serde::from_slice(&bytes).unwrap();
    match decoded {
        IpcResponse::Error(IpcErrorCode::Persistence { reason }) => {
            assert_eq!(reason, "persistence error");
        }
        other => panic!("expected Error::Persistence, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-IT-009: Framed + LengthDelimitedCodec で frame + rmp の 2 層通過
// -------------------------------------------------------------------
#[tokio::test]
async fn tc_it_009_framed_codec_roundtrip_through_duplex() {
    let (a, b) = tokio::io::duplex(64 * 1024);
    let mut fa = Framed::new(a, codec());
    let mut fb = Framed::new(b, codec());

    let req = IpcRequest::ListRecords;
    let bytes = rmp_serde::to_vec(&req).unwrap();
    fa.send(Bytes::from(bytes)).await.unwrap();

    let received = fb.next().await.unwrap().unwrap();
    let decoded: IpcRequest = rmp_serde::from_slice(&received).unwrap();
    assert!(matches!(decoded, IpcRequest::ListRecords));
}
