//! TempDir lifecycle sanity (テスト harness 内 dir lifetime 確認).
//!
//! 各 sub module で `fresh_repo` 経由 `TempDir` を返す経路を使うため、
//! 構築 → drop の基本ライフタイムを 1 件だけ独立に固定する。

use tempfile::TempDir;

#[test]
fn temp_dir_lifecycle_sanity() {
    let dir: TempDir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    assert!(path.exists());
    drop(dir);
    // tempdir が存続している間は確実に dir.path() が valid
}
