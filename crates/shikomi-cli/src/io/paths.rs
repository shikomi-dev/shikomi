//! OS デフォルトの vault dir を解決する薄いヘルパ。
//!
//! env 参照は clap attribute（`#[arg(env = "SHIKOMI_VAULT_DIR")]`）に一任し、
//! 本モジュールでは `std::env::var` を呼ばない（真実源の二重化防止）。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/public-api.md
//! §`shikomi_cli::io::paths`、data-structures.md §env の真実源は clap のみ

use std::path::PathBuf;

use crate::error::CliError;

/// OS データディレクトリ直下の `shikomi/` を返す。
///
/// env は見ない。`dirs::data_dir()` が `None` のとき `CliError::Persistence`
/// （`CannotResolveVaultDir`）で Fail Fast。
///
/// # Errors
/// `dirs::data_dir()` が `None` を返した場合、または `PersistenceError` 包み。
pub fn resolve_os_default_vault_dir() -> Result<PathBuf, CliError> {
    dirs::data_dir()
        .ok_or_else(|| {
            CliError::Persistence(
                shikomi_infra::persistence::PersistenceError::CannotResolveVaultDir,
            )
        })
        .map(|base| base.join("shikomi"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_os_default_vault_dir_returns_shikomi_subdir_when_data_dir_available() {
        // dirs::data_dir() が Some を返す環境では Ok(PathBuf) を得る。
        // 末尾が "shikomi" であることだけ検証（環境依存の親 path は検証しない）。
        if let Ok(path) = resolve_os_default_vault_dir() {
            assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("shikomi"));
        }
    }
}
