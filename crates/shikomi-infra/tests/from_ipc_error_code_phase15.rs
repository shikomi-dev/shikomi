//! `From<IpcErrorCode> for PersistenceError` 6 バリアント完全網羅 UT (Phase 1.5)
//! — test-design/unit.md §2.13 TC-UT-110〜114b
//!
//! 案 D で確定した写像規約:
//! - `EncryptionUnsupported` → `UnsupportedYet`
//! - `NotFound { id }`        → `RecordNotFound(id)`（**Issue #30 新バリアント**）
//! - `InvalidLabel/Persistence/Domain/Internal` → 全て `Internal { reason }` に集約
//!   （実装担当の方針 X 採用、reason 文字列は **daemon 側固定文言** をそのまま保持）
//!
//! 横串: 全 Display 出力に SECRET / 絶対パス / PID 等が漏れていないこと
//!
//! 対応 Issue: #30 / PR #32

use shikomi_core::ipc::IpcErrorCode;
use shikomi_core::RecordId;
use shikomi_infra::persistence::PersistenceError;
use uuid::Uuid;

const SECRET_MARKER: &str = "SECRET_TEST_VALUE";

fn assert_no_leak(rendered: &str) {
    assert!(
        !rendered.contains(SECRET_MARKER),
        "secret marker leaked: {rendered}"
    );
    assert!(
        !rendered.contains("/home/"),
        "absolute home path leaked: {rendered}"
    );
    assert!(
        !rendered.to_lowercase().contains("pid="),
        "pid leaked: {rendered}"
    );
}

// -------------------------------------------------------------------
// TC-UT-110: EncryptionUnsupported → UnsupportedYet 写像
// -------------------------------------------------------------------
#[test]
fn tc_ut_110_encryption_unsupported_maps_to_unsupported_yet() {
    let pe: PersistenceError = IpcErrorCode::EncryptionUnsupported.into();
    match pe {
        PersistenceError::UnsupportedYet { feature, .. } => {
            assert!(
                feature.to_lowercase().contains("encrypt"),
                "feature should describe encryption: {feature}"
            );
        }
        other => panic!("expected UnsupportedYet, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-UT-111: NotFound { id } → RecordNotFound(id)（id 構造保持）
// -------------------------------------------------------------------
#[test]
fn tc_ut_111_not_found_maps_to_record_not_found_preserving_id() {
    let id = RecordId::new(Uuid::now_v7()).expect("v7");
    let pe: PersistenceError = IpcErrorCode::NotFound { id: id.clone() }.into();
    match pe {
        PersistenceError::RecordNotFound(returned_id) => {
            assert_eq!(returned_id, id);
        }
        other => panic!("expected RecordNotFound, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-UT-112: InvalidLabel { reason } → Internal { reason }（方針 X 集約）
// -------------------------------------------------------------------
#[test]
fn tc_ut_112_invalid_label_maps_to_internal_preserving_reason() {
    let pe: PersistenceError = IpcErrorCode::InvalidLabel {
        reason: "invalid label".to_owned(),
    }
    .into();
    match pe {
        PersistenceError::Internal { reason } => {
            assert_eq!(reason, "invalid label");
            assert_no_leak(&reason);
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-UT-113: Persistence { reason } → Internal { reason }（方針 X 集約）
// -------------------------------------------------------------------
#[test]
fn tc_ut_113_persistence_maps_to_internal_preserving_reason() {
    let pe: PersistenceError = IpcErrorCode::Persistence {
        reason: "persistence error".to_owned(),
    }
    .into();
    match pe {
        PersistenceError::Internal { reason } => {
            assert_eq!(reason, "persistence error");
            assert_no_leak(&reason);
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-UT-114: Domain { reason } → Internal { reason }（方針 X 集約）
// -------------------------------------------------------------------
#[test]
fn tc_ut_114_domain_maps_to_internal_preserving_reason() {
    let pe: PersistenceError = IpcErrorCode::Domain {
        reason: "duplicate record id".to_owned(),
    }
    .into();
    match pe {
        PersistenceError::Internal { reason } => {
            assert_eq!(reason, "duplicate record id");
            assert_no_leak(&reason);
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// TC-UT-114b: Internal { reason } → Internal { reason }（identity）
// -------------------------------------------------------------------
#[test]
fn tc_ut_114b_internal_maps_to_internal_identity() {
    let pe: PersistenceError = IpcErrorCode::Internal {
        reason: "unexpected error".to_owned(),
    }
    .into();
    match pe {
        PersistenceError::Internal { reason } => {
            assert_eq!(reason, "unexpected error");
            assert_no_leak(&reason);
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// -------------------------------------------------------------------
// 横串: Display 出力に secret / 絶対パス / PID が出ないこと
// -------------------------------------------------------------------
#[test]
fn tc_ut_114c_display_outputs_never_leak_sensitive_strings() {
    let cases: Vec<PersistenceError> = vec![
        IpcErrorCode::EncryptionUnsupported.into(),
        IpcErrorCode::NotFound {
            id: RecordId::new(Uuid::now_v7()).unwrap(),
        }
        .into(),
        IpcErrorCode::InvalidLabel {
            reason: "invalid label".to_owned(),
        }
        .into(),
        IpcErrorCode::Persistence {
            reason: "persistence error".to_owned(),
        }
        .into(),
        IpcErrorCode::Domain {
            reason: "domain error".to_owned(),
        }
        .into(),
        IpcErrorCode::Internal {
            reason: "unexpected error".to_owned(),
        }
        .into(),
    ];
    for case in &cases {
        let rendered = case.to_string();
        assert_no_leak(&rendered);
    }
}
