//! `list` UseCase — vault 内の全レコードを `RecordView` 列として返す。

use std::path::Path;

use shikomi_core::ipc::RecordSummary;
use shikomi_core::{ProtectionMode, RecordKind};
use shikomi_infra::persistence::VaultRepository;

use crate::error::CliError;
use crate::view::{RecordView, ValueView};

/// vault を読み取り、全レコードを `RecordView` に射影して返す（SQLite 直結経路）。
///
/// `vault_dir` は `VaultNotInitialized` エラー文に埋め込む path（UseCase 純粋性維持のため
/// 呼び出し側から注入、`&dyn VaultRepository` は path を公開しない設計）。
///
/// # Errors
/// - vault 未作成: `CliError::VaultNotInitialized(vault_dir)`
/// - 暗号化モード検出: `CliError::EncryptionUnsupported`
/// - 永続化エラー: `CliError::Persistence`
pub fn list_records(
    repo: &dyn VaultRepository,
    vault_dir: &Path,
) -> Result<Vec<RecordView>, CliError> {
    if !repo.exists()? {
        return Err(CliError::VaultNotInitialized(vault_dir.to_path_buf()));
    }
    let vault = repo.load()?;
    if vault.protection_mode() == ProtectionMode::Encrypted {
        return Err(CliError::EncryptionUnsupported);
    }
    Ok(vault
        .records()
        .iter()
        .map(RecordView::from_record)
        .collect())
}

/// daemon から受け取った `RecordSummary` 列を `RecordView` 列に射影する（`--ipc list` 経路）。
///
/// `Vault` 集約を経由しないため、Secret 用の偽 `Plaintext(empty)` 注入が発生しない
/// （ペテルギウス指摘 §3 に対応）。Secret kind は常に `ValueView::Masked`、Text kind は
/// daemon が返した `value_preview` を `Plain(_)` に写像する。
#[must_use]
pub fn summaries_to_views(summaries: &[RecordSummary]) -> Vec<RecordView> {
    summaries
        .iter()
        .map(|s| RecordView {
            id: s.id.clone(),
            kind: s.kind,
            label: s.label.clone(),
            value: match s.kind {
                RecordKind::Secret => ValueView::Masked,
                RecordKind::Text => s
                    .value_preview
                    .clone()
                    .map_or(ValueView::Masked, ValueView::Plain),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{RecordId, RecordLabel};
    use uuid::Uuid;

    fn make_id() -> RecordId {
        RecordId::new(Uuid::now_v7()).unwrap()
    }

    fn label(s: &str) -> RecordLabel {
        RecordLabel::try_new(s.to_owned()).unwrap()
    }

    #[test]
    fn test_summaries_to_views_secret_yields_masked() {
        let s = RecordSummary {
            id: make_id(),
            kind: RecordKind::Secret,
            label: label("k"),
            value_preview: None,
            value_masked: true,
        };
        let views = summaries_to_views(&[s]);
        assert_eq!(views.len(), 1);
        assert!(matches!(views[0].value, ValueView::Masked));
    }

    #[test]
    fn test_summaries_to_views_text_with_preview_yields_plain() {
        let s = RecordSummary {
            id: make_id(),
            kind: RecordKind::Text,
            label: label("u"),
            value_preview: Some("https://example.com".to_owned()),
            value_masked: false,
        };
        let views = summaries_to_views(&[s]);
        match &views[0].value {
            ValueView::Plain(v) => assert_eq!(v, "https://example.com"),
            ValueView::Masked => panic!("expected Plain"),
        }
    }

    #[test]
    fn test_summaries_to_views_text_without_preview_yields_masked() {
        let s = RecordSummary {
            id: make_id(),
            kind: RecordKind::Text,
            label: label("u"),
            value_preview: None,
            value_masked: false,
        };
        let views = summaries_to_views(&[s]);
        assert!(matches!(views[0].value, ValueView::Masked));
    }
}
