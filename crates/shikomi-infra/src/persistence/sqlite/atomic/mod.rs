//! アトミック書き込みユーティリティ。
//!
//! Phase 8 リファクタ（Issue #73）: `AtomicWriter` ZST + 静的メソッド連鎖から
//! セッション型 `AtomicWriteSession { conn, new_path }` に移行。
//! `finalize(self, retry_policy)` の所有権消費でクローズ順序契約を型レベルで強制。
//!
//! - **`AtomicWriteSession`**: `.new` 書込から `conn.close()` + fsync + rename まで一連で完結。
//! - **`AtomicWriter`** (ZST): `detect_orphan` / `cleanup_new` の名前空間として残存。
//! - **`RetryPolicy`** trait: Win rename retry の振る舞いをテスト注入可能に抽象化。
//!
//! Bug-G-001 反映（2026-04-27）: Win CI ランナーで Defender / Search Indexer が
//! ハンドルを `drop` 後も `~250ms+` 保持し続けるため、retry を指数バックオフへ拡張
//! （`50ms × 2^(n-1)` ± `25ms` jitter × 最大 5 回、最悪 ~1675ms / 平均 ~1550ms）。
//!
//! Windows DACL 適用順序確定（Phase 8、`./classes.md` §3.3）: rename 前に `.new` に
//! `ensure_file` を適用し、`MoveFileExW` がソース SD を保持することで `vault.db` に引継。

mod drop_guard;
mod session;
mod writer;

pub(crate) use session::AtomicWriteSession;
pub(crate) use writer::AtomicWriter;

use std::time::Duration;

// -------------------------------------------------------------------
// 内部定数
// -------------------------------------------------------------------

/// `SQLite` サイドカーファイル名のサフィックス。
pub(super) const SQLITE_SIDECAR_SUFFIXES: &[&str] = &["-journal", "-wal", "-shm"];

// -------------------------------------------------------------------
// RetryPolicy trait
// -------------------------------------------------------------------

/// rename retry の振る舞いを抽象化する trait（Phase 8、Issue #73）。
///
/// `cfg(windows)` 限定 rename retry で使用する。`NoSleepRetryPolicy` を差し込むことで
/// テスト時の実際の sleep を排除し CI を高速化する（`./classes.md` §3.4 参照）。
///
/// Unix では `finalize` 内で `should_retry` / `sleep_duration` は一切呼ばれない
/// （Unix の rename は即 fail fast、retry なし）。
///
/// `cfg(windows)` 限定ロジックで使用するため非 Windows ビルドでは dead_code になるが意図的。
#[allow(dead_code)]
pub(crate) trait RetryPolicy {
    /// 最大 retry 回数。超過したら `AtomicWriteFailed { stage: Rename }` で return。
    fn max_attempts(&self) -> u32;

    /// `attempt` 番目（1-indexed）の retry 前 sleep 量を返す。jitter を内部で生成してよい。
    fn sleep_duration(&self, attempt: u32) -> Duration;

    /// OS エラーコードが一過性エラーか否かを判定する。
    ///
    /// `cfg(windows)`:
    /// - `ERROR_ACCESS_DENIED (5)` / `ERROR_SHARING_VIOLATION (32)` /
    ///   `ERROR_LOCK_VIOLATION (33)` → `true`、それ以外 → `false`
    ///
    /// `cfg(not(windows))`: 常に `false`。
    fn should_retry(&self, raw_os_error: i32) -> bool;
}

// -------------------------------------------------------------------
// ExponentialBackoffRetryPolicy
// -------------------------------------------------------------------

/// 指数バックオフ retry の production default 実装。
///
/// `max_attempts = 5`、`base_ms = 50`、`jitter = ±25ms`（`OsRng` 一様乱数）。
///
/// | attempt | 中央値 | range       | 累積中央値 |
/// |---------|--------|-------------|----------|
/// | 1       | 50ms   | 25〜75ms   | 50ms     |
/// | 2       | 100ms  | 75〜125ms  | 150ms    |
/// | 3       | 200ms  | 175〜225ms | 350ms    |
/// | 4       | 400ms  | 375〜425ms | 750ms    |
/// | 5       | 800ms  | 775〜825ms | 1550ms   |
///
/// 最悪 ~1675ms / 平均 ~1550ms。SSoT:
/// `docs/features/vault-persistence/basic-design/security.md` §jitter。
pub(crate) struct ExponentialBackoffRetryPolicy;

impl Default for ExponentialBackoffRetryPolicy {
    fn default() -> Self {
        Self
    }
}

impl RetryPolicy for ExponentialBackoffRetryPolicy {
    fn max_attempts(&self) -> u32 {
        5
    }

    fn sleep_duration(&self, attempt: u32) -> Duration {
        #[cfg(windows)]
        {
            use rand_core::{OsRng, RngCore};

            const BASE_MS: u64 = 50;
            const JITTER_HALF_RANGE_MS: u64 = 25;
            // 0..=50 を一様抽選後 -25 シフトで [-25, +25]
            // HALF_RANGE_MS ≤ 127 の範囲で u8 へのキャストは安全
            const JITTER_RANGE: u8 = (JITTER_HALF_RANGE_MS * 2 + 1) as u8;

            let mut buf = [0u8; 1];
            OsRng.fill_bytes(&mut buf);
            let jitter_pos = u64::from(buf[0] % JITTER_RANGE);
            // attempt は 1..=max_attempts(=5) なので左シフト overflow なし
            let multiplier: u64 = 1u64 << (attempt.saturating_sub(1));
            let center_ms = BASE_MS.saturating_mul(multiplier);
            let delay_ms = center_ms + jitter_pos - JITTER_HALF_RANGE_MS;
            Duration::from_millis(delay_ms)
        }
        #[cfg(not(windows))]
        {
            let _ = attempt;
            Duration::ZERO
        }
    }

    fn should_retry(&self, raw_os_error: i32) -> bool {
        #[cfg(windows)]
        {
            matches!(raw_os_error, 5 | 32 | 33)
        }
        #[cfg(not(windows))]
        {
            let _ = raw_os_error;
            false
        }
    }
}

// -------------------------------------------------------------------
// NoSleepRetryPolicy（テスト専用）
// -------------------------------------------------------------------

/// テスト専用 `RetryPolicy`（sleep なし・CI 高速化）。
///
/// `sleep_duration` は常に `Duration::ZERO`。`should_retry` は
/// `ExponentialBackoffRetryPolicy` と同じ判定ロジックを使用。
/// TC-I29 / TC-I29-B の retry 発火テストで実際の sleep を排除する（`./classes.md` §3.4）。
#[cfg(test)]
pub(crate) struct NoSleepRetryPolicy {
    pub(crate) max_attempts: u32,
}

#[cfg(test)]
impl RetryPolicy for NoSleepRetryPolicy {
    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    fn sleep_duration(&self, _attempt: u32) -> Duration {
        Duration::ZERO
    }

    fn should_retry(&self, raw_os_error: i32) -> bool {
        #[cfg(windows)]
        {
            matches!(raw_os_error, 5 | 32 | 33)
        }
        #[cfg(not(windows))]
        {
            let _ = raw_os_error;
            false
        }
    }
}

// ---------------------------------------------------------------------------
// 内部テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::{
        Record, RecordId, RecordKind, RecordLabel, RecordPayload, SecretString, Vault, VaultHeader,
        VaultVersion,
    };
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn plaintext_vault(label: &str, value: &str) -> Vault {
        let header =
            VaultHeader::new_plaintext(VaultVersion::CURRENT, OffsetDateTime::now_utc()).unwrap();
        let mut vault = Vault::new(header);
        let record = Record::new(
            RecordId::new(Uuid::now_v7()).unwrap(),
            RecordKind::Secret,
            RecordLabel::try_new(label.to_string()).unwrap(),
            RecordPayload::Plaintext(SecretString::from_string(value.to_string())),
            OffsetDateTime::now_utc(),
        );
        vault.add_record(record).unwrap();
        vault
    }

    /// TC-I06 — `write_new_only` フックで .new のみ書き込み→load が `OrphanNewFile` を返す。
    ///
    /// AC-06 対応。`write_new_only` は `finalize` を呼ばないため .new が残り、
    /// vault.db の内容は初期 vault のままになる。
    #[test]
    fn tc_i06_write_new_only_hook_orphan() {
        let dir = TempDir::new().unwrap();
        let paths = crate::persistence::paths::VaultPaths::new_unchecked(dir.path().to_path_buf());

        // ディレクトリのパーミッションを設定
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        // 初期 vault を save（vault.db が存在する状態にする）
        let initial_vault = plaintext_vault("initial", "initial-value");
        let session = AtomicWriteSession::new(&paths, &initial_vault).unwrap();
        session.finalize(&ExponentialBackoffRetryPolicy).unwrap();

        // vault.db のバイト列を記録
        let db_bytes_before = std::fs::read(paths.vault_db()).unwrap();

        // write_new_only で別内容の .new のみ作成（rename しない）
        let new_vault = plaintext_vault("updated", "updated-value");
        AtomicWriter::write_new_only(&paths, &new_vault).unwrap();

        // .new ファイルが存在することを確認
        assert!(
            paths.vault_db_new().exists(),
            ".new ファイルが作成されていない"
        );

        // .new が残存している状態での load は OrphanNewFile になる
        let orphan_result = AtomicWriter::detect_orphan(paths.vault_db_new());
        assert!(
            matches!(orphan_result, Err(crate::persistence::error::PersistenceError::OrphanNewFile { .. })),
            "OrphanNewFile を期待したが {orphan_result:?}"
        );

        // vault.db の内容が初期 vault のまま（.new の内容が反映されていない）
        let db_bytes_after = std::fs::read(paths.vault_db()).unwrap();
        assert_eq!(
            db_bytes_before, db_bytes_after,
            "vault.db の内容が変わっている（.new がリネームされてしまった）"
        );
    }

    // -------------------------------------------------------------------
    // Phase 8 (Issue #73) — AtomicWriteSession / RetryPolicy 追加テスト群
    // -------------------------------------------------------------------

    /// TC-U-Drop — `finalize` を呼ばずに drop された場合、`Drop` impl が
    /// `AtomicWriter::cleanup_new` を best-effort 呼出して `.new` を削除する。
    ///
    /// `new_path` が `Some` のまま drop される経路（panic / early-return 等）で
    /// `.new` の残存を防ぐ Drop safety 設計を確認する。
    /// 対応設計書: `./classes.md` §3.1 Drop safety
    #[test]
    fn tc_u_drop_without_finalize_removes_new_file() {
        let dir = TempDir::new().unwrap();
        let paths = crate::persistence::paths::VaultPaths::new_unchecked(dir.path().to_path_buf());
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        let vault = plaintext_vault("drop-guard", "test-value");
        let session = AtomicWriteSession::new(&paths, &vault).unwrap();

        assert!(
            paths.vault_db_new().exists(),
            "AtomicWriteSession::new 後に .new ファイルが存在しない"
        );

        // finalize を呼ばずに drop → Drop impl が cleanup_new を呼ぶ
        drop(session);

        assert!(
            !paths.vault_db_new().exists(),
            "drop 後も .new が残存している — Drop guard が AtomicWriter::cleanup_new を呼んでいない"
        );
    }

    /// TC-U-NoSleep-1 — `NoSleepRetryPolicy::sleep_duration` は常に `Duration::ZERO` を返す。
    ///
    /// CI 高速化のための sleep-free 設計が正しく実装されていることを確認。
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[test]
    fn tc_u_no_sleep_retry_policy_sleep_is_zero() {
        use std::time::Duration;
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        for attempt in 1..=5u32 {
            assert_eq!(
                policy.sleep_duration(attempt),
                Duration::ZERO,
                "attempt {attempt}: sleep_duration が Duration::ZERO でない"
            );
        }
    }

    /// TC-U-NoSleep-2 — `NoSleepRetryPolicy::max_attempts` は構築時に指定した値を返す。
    ///
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[test]
    fn tc_u_no_sleep_retry_policy_max_attempts() {
        let policy3 = NoSleepRetryPolicy { max_attempts: 3 };
        assert_eq!(policy3.max_attempts(), 3);
        let policy5 = NoSleepRetryPolicy { max_attempts: 5 };
        assert_eq!(policy5.max_attempts(), 5);
    }

    /// TC-U-NoSleep-3 (non-Windows) — 非 Windows では `should_retry` は常に `false`。
    ///
    /// Unix は rename が即 fail fast（retry なし）のため、エラーコードに関係なく
    /// retry しないことを確認する。
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[cfg(not(windows))]
    #[test]
    fn tc_u_no_sleep_retry_policy_should_not_retry_non_windows() {
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        // Unix では Windows エラーコード相当でも retry しない
        assert!(!policy.should_retry(5)); // ERROR_ACCESS_DENIED 相当
        assert!(!policy.should_retry(32)); // ERROR_SHARING_VIOLATION 相当
        assert!(!policy.should_retry(33)); // ERROR_LOCK_VIOLATION 相当
        assert!(!policy.should_retry(0));
        assert!(!policy.should_retry(2));
    }

    /// TC-U-NoSleep-3 (Windows) — Windows では `should_retry(5/32/33)` は `true`、
    /// それ以外は `false`。
    ///
    /// `ERROR_ACCESS_DENIED (5)` / `ERROR_SHARING_VIOLATION (32)` /
    /// `ERROR_LOCK_VIOLATION (33)` が retry 対象として正しく判定されることを確認する。
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy`
    #[cfg(windows)]
    #[test]
    fn tc_u_no_sleep_retry_policy_should_retry_windows() {
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        // retry 対象（一過性 file lock エラー）
        assert!(policy.should_retry(5)); // ERROR_ACCESS_DENIED
        assert!(policy.should_retry(32)); // ERROR_SHARING_VIOLATION
        assert!(policy.should_retry(33)); // ERROR_LOCK_VIOLATION
                                          // retry 非対象
        assert!(!policy.should_retry(0));
        assert!(!policy.should_retry(2)); // ERROR_FILE_NOT_FOUND
        assert!(!policy.should_retry(6)); // ERROR_INVALID_HANDLE
    }

    /// TC-U-FinalizeFail (Unix) — `finalize` が FsyncTemp ステップで失敗した場合に
    /// `.new` ファイルが best-effort 削除される。
    ///
    /// `finalize` は冒頭で `new_path.take()` により Drop への cleanup 委譲を切り离し、
    /// 各失敗ステップで `AtomicWriter::cleanup_new` を明示呼出する設計を確認する。
    ///
    /// 注入方法: `.new` を chmod 0o400（書き込み不可）にして
    /// `FsyncTemp` の `.write(true).open()` を `PermissionDenied` で失敗させる。
    /// 親ディレクトリは 0o700 のまま維持するため `cleanup_new` の `remove_file` は成功する。
    ///
    /// 対応設計書: `./classes.md` §3.2 finalize § cleanup_new 明示呼出
    #[cfg(unix)]
    #[test]
    fn tc_u_finalize_failure_cleans_new_on_fsync_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let paths = crate::persistence::paths::VaultPaths::new_unchecked(dir.path().to_path_buf());
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        let vault = plaintext_vault("finalize-fail", "test-value");
        let session = AtomicWriteSession::new(&paths, &vault).unwrap();
        assert!(paths.vault_db_new().exists(), ".new が作成されていない");

        // .new を書き込み不可（0o400）に変更 → FsyncTemp が open(.write(true)) で失敗。
        // 親ディレクトリは 0o700 のまま → cleanup_new の remove_file は成功する。
        std::fs::set_permissions(paths.vault_db_new(), std::fs::Permissions::from_mode(0o400))
            .unwrap();

        let result = session.finalize(&ExponentialBackoffRetryPolicy);

        // FsyncTemp で AtomicWriteFailed が返ること
        assert!(
            matches!(
                result,
                Err(crate::persistence::error::PersistenceError::AtomicWriteFailed {
                    stage: crate::persistence::error::AtomicWriteStage::FsyncTemp,
                    ..
                })
            ),
            "AtomicWriteFailed {{ stage: FsyncTemp }} を期待したが: {:?}",
            result.err()
        );

        // finalize 失敗後でも .new が cleanup されている
        assert!(
            !paths.vault_db_new().exists(),
            ".new が残存している — finalize 失敗時の明示的 cleanup_new が機能していない"
        );
    }

    /// TC-U-WinNoSleep (Windows) — `NoSleepRetryPolicy` を使うと retry が即座に全敗し、
    /// `AtomicWriteFailed { stage: Rename }` が返り `.new` が cleanup される。
    ///
    /// `NoSleepRetryPolicy` が sleep 0ms のため、`vault.db` を長時間 `FILE_SHARE_NONE`
    /// で保持している間に finalize を呼ぶと 5 回の retry が一瞬で全て失敗する。これにより:
    ///
    /// 1. retry ループが実際に発火することを確認
    /// 2. 全敗時に `.new` が cleanup されることを確認
    ///
    /// 対応設計書: `./classes.md` §3.4 `NoSleepRetryPolicy` / `./classes.md` §3.2 finalize
    /// AC-19 (Issue #65 retry 補強) 対応。
    #[cfg(windows)]
    #[test]
    fn tc_u_windows_no_sleep_retry_exhausts_on_held_file() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        let dir = TempDir::new().unwrap();
        let paths = crate::persistence::paths::VaultPaths::new_unchecked(dir.path().to_path_buf());
        crate::persistence::permission::PermissionGuard::ensure_dir(dir.path()).unwrap();

        // 初期 vault.db を作成
        let vault1 = plaintext_vault("v1", "initial");
        let session1 = AtomicWriteSession::new(&paths, &vault1).unwrap();
        session1
            .finalize(&ExponentialBackoffRetryPolicy)
            .expect("初期 vault 作成失敗");

        // 更新用 session を新規作成
        let vault2 = plaintext_vault("v2", "updated");
        let session2 = AtomicWriteSession::new(&paths, &vault2).unwrap();
        assert!(
            paths.vault_db_new().exists(),
            "session2 作成後 .new が存在しない"
        );

        // 補助スレッドが vault.db を FILE_SHARE_NONE で保持する。
        // NoSleepRetryPolicy は sleep 0ms のため 5 回の即時 retry が完了するより長く保持する。
        const FILE_SHARE_NONE: u32 = 0;
        const HOLD_MS: u64 = 500; // 5 回即時 retry (~数十μs) より十分長い
        let vault_db = paths.vault_db().to_path_buf();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let handle = thread::spawn(move || {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_NONE)
                .open(&vault_db)
                .expect("vault.db の排他オープン失敗");
            ready_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(HOLD_MS));
            drop(f);
        });
        ready_rx.recv().unwrap();

        // NoSleepRetryPolicy で finalize → 5 回即時 retry が全て失敗
        let policy = NoSleepRetryPolicy { max_attempts: 5 };
        let result = session2.finalize(&policy);
        handle.join().unwrap();

        // AtomicWriteFailed { stage: Rename } が返る（retry 全敗 + fail fast）
        assert!(
            matches!(
                result,
                Err(crate::persistence::error::PersistenceError::AtomicWriteFailed {
                    stage: crate::persistence::error::AtomicWriteStage::Rename,
                    ..
                })
            ),
            "AtomicWriteFailed {{ stage: Rename }} を期待したが: {:?}",
            result.err()
        );

        // .new が cleanup されている（finalize 全敗時の明示的 cleanup_new）
        assert!(
            !paths.vault_db_new().exists(),
            ".new が残存している — retry 全敗時の cleanup_new が機能していない"
        );
    }

    // -------------------------------------------------------------------
    // TC-I29-D (unit) — Win retry 中 TOCTOU 再検証ユニットテスト群
    // -------------------------------------------------------------------
    //
    // `AtomicWriteSession::reverify_no_reparse_point` は `cfg(windows)` の
    // rename retry ループ内で各 attempt の sleep 直後に呼ばれ、retry 窓中の
    // symlink / NTFS reparse point 差替えを fail fast する。
    // 基本設計 `security.md §atomic write の二次防衛線 §Win retry 中 TOCTOU` 対応。
    //
    // 整合する受入基準: AC-19 (Issue #65 retry 補強の二次防衛線、TOCTOU 側)。

    /// TC-I29-D-1: 通常ファイル → Ok。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d1_reverify_returns_ok_for_regular_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("regular.bin");
        std::fs::write(&f, b"x").unwrap();
        AtomicWriteSession::reverify_no_reparse_point(&f)
            .expect("通常ファイルで reverify が誤判定 (Ok を期待)");
    }

    /// TC-I29-D-2: 未存在パス (vault.db 初回作成時の final_path) → Ok。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d2_reverify_returns_ok_for_missing_path() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("not_yet.bin");
        AtomicWriteSession::reverify_no_reparse_point(&f)
            .expect("未存在パスで reverify が誤判定 (Ok を期待 — 初回 save の final_path 経路)");
    }

    /// TC-I29-D-3: ディレクトリ junction (`mklink /J`) → SymlinkNotAllowed。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d3_reverify_detects_directory_junction() {
        use crate::persistence::error::VaultDirReason;

        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target_dir");
        std::fs::create_dir(&target).unwrap();
        let junction = dir.path().join("junction_dir");

        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .status()
            .expect("cmd /C mklink /J 実行不能");

        if !status.success() {
            eprintln!(
                "skipping tc_i29_d3: mklink /J が失敗した (権限不足の可能性、exit={:?})",
                status.code()
            );
            return;
        }

        let result = AtomicWriteSession::reverify_no_reparse_point(&junction);
        match result {
            Err(crate::persistence::error::PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::SymlinkNotAllowed,
                ..
            }) => {}
            other => panic!(
                "junction で SymlinkNotAllowed を期待したが {:?}",
                other.err()
            ),
        }
    }

    /// TC-I29-D-4: ディレクトリ symlink → SymlinkNotAllowed。
    #[cfg(windows)]
    #[test]
    fn tc_i29_d4_reverify_detects_directory_symlink() {
        use crate::persistence::error::VaultDirReason;
        use std::os::windows::fs::symlink_dir;

        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target_dir");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("symlink_dir");

        if symlink_dir(&target, &link).is_err() {
            eprintln!("skipping tc_i29_d4: dir symlink 作成権限が無い (Developer Mode 無効)");
            return;
        }

        let result = AtomicWriteSession::reverify_no_reparse_point(&link);
        match result {
            Err(crate::persistence::error::PersistenceError::InvalidVaultDir {
                reason: VaultDirReason::SymlinkNotAllowed,
                ..
            }) => {}
            other => panic!(
                "dir symlink で SymlinkNotAllowed を期待したが {:?}",
                other.err()
            ),
        }
    }
}
