//! `RecoveryDisclosure` / `RecoveryWords` — 24 語初回 1 度表示の型レベル強制 (Sub-D 新規)。
//!
//! 設計書: `docs/features/vault-encryption/detailed-design/repository-and-migration.md`
//! §`RecoveryDisclosure`
//!
//! ## REQ-S13 型レベル強制契約
//!
//! 1. `disclose(self)` は `self` を消費 → 2 度呼出は compile_fail (C-19)。
//! 2. `RecoveryWords` は `Display` / `Serialize` 未実装 → 永続化禁止 (DC-6)。
//! 3. `Drop` 連鎖で内包 `RecoveryMnemonic` の `SecretBox<Zeroizing<...>>` が zeroize。
//! 4. `drop_without_disclose(self)` でクラッシュ・キャンセル経路の Fail Secure 提供。

use core::fmt;
use shikomi_core::crypto::RecoveryMnemonic;
use time::OffsetDateTime;

// -------------------------------------------------------------------
// RecoveryWords (24 語表示用 newtype)
// -------------------------------------------------------------------

/// 24 語リカバリ・ワード (CLI/GUI 表示専用)。
///
/// `RecoveryDisclosure::disclose` の戻り値として **1 度だけ**取得され、表示後即 Drop で
/// `String` の zeroize が連鎖発火する (`zeroize::Zeroizing` 相当)。
///
/// # Forbidden traits (DC-6)
///
/// - `Display` 未実装: 誤 `println!("{}", words)` を構造禁止。
/// - `serde::Serialize` 未実装: 誤永続化を構造禁止。
/// - `Clone` 未実装: 表示後の複製による滞留時間延長を構造禁止。
/// - `Debug` は `[REDACTED RECOVERY WORDS (24)]` 固定 (秘匿)。
///
/// ## 表示ルール
///
/// `iter()` で `(index, &str)` の番号付きイテレータを取得し、CLI/GUI が
/// 「1: word1, 2: word2, ...」形式で表示する。表示完了後即 Drop。
pub struct RecoveryWords {
    words: [String; 24],
}

impl RecoveryWords {
    /// `RecoveryDisclosure::disclose` 専用 `pub(super)` コンストラクタ。
    ///
    /// 同一モジュール (`vault_migration`) 内の `RecoveryDisclosure::disclose` のみが呼出可能。
    pub(super) fn new(words: [String; 24]) -> Self {
        Self { words }
    }

    /// 番号付きイテレータ `(1..=24, &str)` を返す (CLI/GUI 表示用)。
    pub fn iter(&self) -> impl Iterator<Item = (usize, &str)> {
        self.words
            .iter()
            .enumerate()
            .map(|(i, w)| (i + 1, w.as_str()))
    }

    /// 24 語の生スライスへの参照 (テスト・ユニット検証用)。
    /// 本番 CLI/GUI からは `iter()` 経由で利用すること。
    #[must_use]
    pub fn as_slice(&self) -> &[String; 24] {
        &self.words
    }
}

impl fmt::Debug for RecoveryWords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED RECOVERY WORDS (24)]")
    }
}

impl Drop for RecoveryWords {
    fn drop(&mut self) {
        // 各 String の中身を zeroize (zeroize crate 経由)。
        // String 自体は Drop で free されるが、ヒープ内容を 0 上書きするため
        // 一旦 mem::take して空文字列に置換 → 元の中身を zeroize する。
        use zeroize::Zeroize;
        for w in &mut self.words {
            w.zeroize();
        }
    }
}

// -------------------------------------------------------------------
// RecoveryDisclosure (24 語初回 1 度表示の型レベル強制)
// -------------------------------------------------------------------

/// 24 語初回 1 度表示の型レベル強制ラッパ (REQ-S13 / C-19)。
///
/// `disclose(self)` は `self` を消費するため Rust の所有権ルールで 2 度呼出が compile_fail。
/// `Drop` 連鎖で内包する `RecoveryMnemonic` の zeroize が発火する (Sub-A C-1 維持)。
///
/// `Serialize` / `Display` 未実装で永続化・表示経路を型レベル封鎖。
pub struct RecoveryDisclosure {
    /// 内包する 24 語ニーモニック。
    mnemonic: RecoveryMnemonic,
    /// 監査ログ用構築時刻 (秘密でない)。
    displayed_at: OffsetDateTime,
}

impl RecoveryDisclosure {
    /// `vault_migration` モジュール内部からのみ構築可能。
    /// `VaultMigration::encrypt_vault` の戻り値として 1 回だけ生成される。
    pub(super) fn new(mnemonic: RecoveryMnemonic, displayed_at: OffsetDateTime) -> Self {
        Self {
            mnemonic,
            displayed_at,
        }
    }

    /// 24 語を `RecoveryWords` として取り出す (所有権消費)。
    ///
    /// `self` を move するため、本メソッドは型レベルで 2 度呼べない (C-19)。
    /// `RecoveryMnemonic::expose_words()` で `&[String; 24]` を取得 → コピーして
    /// `RecoveryWords` を構築 → 元の `RecoveryMnemonic` は scope 抜けで Drop & zeroize。
    #[must_use]
    pub fn disclose(self) -> RecoveryWords {
        let words_ref = self.mnemonic.expose_words();
        // `[String; 24]` を `clone` してコピーを `RecoveryWords` に渡す。
        // 元 `RecoveryMnemonic` は `self` の Drop で zeroize される。
        let words: [String; 24] = std::array::from_fn(|i| words_ref[i].clone());
        RecoveryWords::new(words)
    }

    /// クラッシュ・キャンセル経路で「ユーザに見せずに即破棄」する Fail Secure 経路。
    ///
    /// `self` を消費するため `disclose` と同様 2 度呼べない。
    /// `RecoveryMnemonic` の Drop 連鎖で zeroize が発火する。
    pub fn drop_without_disclose(self) {
        // self の所有権を取り、本関数の scope 抜けで Drop が発火する。
        drop(self);
    }

    /// 監査ログ用構築時刻 (秘密でない、CLI/GUI が表示メッセージに使う場合あり)。
    #[must_use]
    pub fn displayed_at(&self) -> OffsetDateTime {
        self.displayed_at
    }
}

impl fmt::Debug for RecoveryDisclosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED RECOVERY DISCLOSURE]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::crypto::RecoveryMnemonic;

    fn dummy_mnemonic() -> RecoveryMnemonic {
        let words: [String; 24] = std::array::from_fn(|i| format!("word{i:02}"));
        RecoveryMnemonic::from_words(words)
    }

    #[test]
    fn disclose_yields_recovery_words_with_24_entries() {
        let m = dummy_mnemonic();
        let d = RecoveryDisclosure::new(m, OffsetDateTime::UNIX_EPOCH);
        let words = d.disclose();
        assert_eq!(words.as_slice().len(), 24);
        assert_eq!(words.as_slice()[0], "word00");
        assert_eq!(words.as_slice()[23], "word23");
    }

    #[test]
    fn disclose_iter_returns_numbered_entries() {
        let m = dummy_mnemonic();
        let d = RecoveryDisclosure::new(m, OffsetDateTime::UNIX_EPOCH);
        let words = d.disclose();
        let collected: Vec<(usize, String)> =
            words.iter().map(|(i, w)| (i, w.to_string())).collect();
        assert_eq!(collected.len(), 24);
        assert_eq!(collected[0].0, 1);
        assert_eq!(collected[0].1, "word00");
        assert_eq!(collected[23].0, 24);
    }

    #[test]
    fn recovery_words_debug_returns_redacted_marker() {
        let m = dummy_mnemonic();
        let d = RecoveryDisclosure::new(m, OffsetDateTime::UNIX_EPOCH);
        let words = d.disclose();
        let s = format!("{words:?}");
        assert_eq!(s, "[REDACTED RECOVERY WORDS (24)]");
        assert!(!s.contains("word00"));
    }

    #[test]
    fn recovery_disclosure_debug_returns_redacted_marker() {
        let m = dummy_mnemonic();
        let d = RecoveryDisclosure::new(m, OffsetDateTime::UNIX_EPOCH);
        let s = format!("{d:?}");
        assert_eq!(s, "[REDACTED RECOVERY DISCLOSURE]");
    }

    /// TC-D-U05: drop_without_disclose で内部 RecoveryMnemonic の Drop 連鎖発火。
    /// メモリパターン直接観測は OS/Allocator 依存のため、API として呼び出せて
    /// panic しないこと + `self` 消費で 2 度呼べないことを確認する。
    #[test]
    fn drop_without_disclose_consumes_self_without_panic() {
        let m = dummy_mnemonic();
        let d = RecoveryDisclosure::new(m, OffsetDateTime::UNIX_EPOCH);
        d.drop_without_disclose();
        // d はここで使えない (move 後)。compile_fail を期待する経路は doctest で別途。
    }

    #[test]
    fn displayed_at_returns_construction_time() {
        let m = dummy_mnemonic();
        let now = OffsetDateTime::UNIX_EPOCH;
        let d = RecoveryDisclosure::new(m, now);
        assert_eq!(d.displayed_at(), now);
    }
}

// ---------------------------------------------------------------------------
// 型レベル強制 compile_fail doctests (C-19 / DC-6)
// ---------------------------------------------------------------------------

/// C-19: `disclose(self)` 所有権消費後、move 後再使用は compile_fail。
///
/// ```compile_fail
/// use shikomi_infra::persistence::vault_migration::RecoveryDisclosure;
/// // RecoveryDisclosure::new は pub(super) のため外部 crate から構築不可だが、
/// // 仮に構築できたとしても disclose 後の再利用は compile_fail。
/// // ここでは型シグネチャの所有権消費を示す疑似コードとして記述。
/// fn _check(d: RecoveryDisclosure) {
///     let _ = d.disclose();
///     let _ = d.disclose(); // E0382 use of moved value
/// }
/// ```
///
/// DC-6: `RecoveryWords` への `Display` 実装は compile_fail (型外部からは impl 不能)。
///
/// ```compile_fail
/// use shikomi_infra::persistence::vault_migration::RecoveryWords;
/// // 外部 crate から impl すると orphan rule 違反 (E0117)。
/// impl std::fmt::Display for RecoveryWords {
///     fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }
/// }
/// ```
#[cfg(doctest)]
struct _CompileFailGuard;
