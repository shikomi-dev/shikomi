//! `DecryptConfirmation` — `vault decrypt` 二段確認の型レベル証跡 (Sub-D 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//! §`DecryptConfirmation` (C-20)
//!
//! ## C-20 型レベル強制契約
//!
//! - `_private: ()` フィールドが非 `pub` のため**外部 crate から直接構築禁止**。
//! - `DecryptConfirmation::confirm()` のみが `Self` を返す pub API。
//! - `VaultMigration::decrypt_vault(.., _confirm: DecryptConfirmation)` の引数で
//!   存在を強制 → `--force` フラグでも省略不可 (Sub-F CLI 経路で型シグネチャに支配される)。

use core::fmt;

/// `vault decrypt` 二段確認の型レベル証跡。
///
/// 設計書 §`DecryptConfirmation`:
/// - 外部 crate からの直接構築禁止 (`_private: ()` 非可視性)。
/// - `confirm()` のみが pub コンストラクタ (Sub-F CLI が「DECRYPT」キーワード入力 +
///   パスワード再入力の二段確認通過後に呼ぶ)。
/// - `VaultMigration::decrypt_vault` の引数として要求される。
pub struct DecryptConfirmation {
    /// 非 `pub` フィールド。外部 crate からの直接構築禁止 (C-20 構造的封鎖)。
    _private: (),
}

impl DecryptConfirmation {
    /// 二段確認通過後に Sub-F CLI / GUI が呼ぶコンストラクタ。
    ///
    /// ## 呼出側責務 (Sub-F CLI / GUI)
    ///
    /// 1. ユーザに「DECRYPT」大文字英字を**手入力**させる (UI で paste 抑制)。
    /// 2. パスワード再入力させる (`subtle::ConstantTimeEq` で比較)。
    /// 3. 両方通過後に本関数を呼んで `DecryptConfirmation` を取得。
    /// 4. `VaultMigration::decrypt_vault(_, confirm)` の引数として渡す。
    ///
    /// 本関数自体は引数を取らない。確認ロジックは Sub-F CLI/GUI 層が担当し、
    /// 通過した事実だけを型レベル証跡として `DecryptConfirmation` に閉じ込める。
    /// (二段確認の具体ロジックを shikomi-infra に持ち込まない、Clean Arch 維持)
    #[must_use]
    pub fn confirm() -> Self {
        Self { _private: () }
    }
}

impl fmt::Debug for DecryptConfirmation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DecryptConfirmation(confirmed)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-D-U08 simplified: `confirm()` で `DecryptConfirmation` を構築できる。
    /// 二段確認の具体ロジック (yes_keyword / password_reentry) は Sub-F CLI 層実装、
    /// shikomi-infra 側は通過証跡型のみ提供する責務分離。
    #[test]
    fn confirm_constructs_decrypt_confirmation() {
        let _ = DecryptConfirmation::confirm();
    }

    #[test]
    fn debug_returns_fixed_marker() {
        let c = DecryptConfirmation::confirm();
        assert_eq!(format!("{c:?}"), "DecryptConfirmation(confirmed)");
    }
}

// ---------------------------------------------------------------------------
// 型レベル強制 compile_fail doctest (C-20 / TC-D-U11)
// ---------------------------------------------------------------------------

/// TC-D-U11: 外部 crate から `DecryptConfirmation { _private: () }` 直接構築は compile_fail。
///
/// ```compile_fail
/// use shikomi_infra::persistence::vault_migration::DecryptConfirmation;
/// // _private フィールドは非 pub のため外部から構築できない (E0451)。
/// let _ = DecryptConfirmation { _private: () };
/// ```
#[cfg(doctest)]
struct _CompileFailGuard;
