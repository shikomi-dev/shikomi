//! `list` UseCase — vault 内の全レコードを `RecordView` 列として返す。

use std::path::Path;

use shikomi_core::ProtectionMode;
use shikomi_infra::persistence::VaultRepository;

use crate::error::CliError;
use crate::view::RecordView;

/// vault を読み取り、全レコードを `RecordView` に射影して返す。
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
