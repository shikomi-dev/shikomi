//! Sub-E (#43) IPC V2 結合テスト — エントリポイント.
//!
//! 設計書 SSoT: docs/features/vault-encryption/test-design/sub-e-vek-cache-ipc.md
//! §14.4 Sub-E テストマトリクス + §14.6 Sub-E 結合テスト詳細
//!
//! ## 検証アーキテクチャ
//!
//! `dispatch_v2` を **in-process で直接呼出** する半ブラックボックス結合テスト。
//! `MockVaultMigration` は採用せず実 `VaultMigration` + 実 `SqliteVaultRepository`
//! + tempdir の **本物経路**で C-22〜C-29 全契約 + EC-1〜EC-10 の主要受入条件を機械検証する。
//!
//! KDF は本番値 (Argon2id `FROZEN_OWASP_2024_05`、19MiB / 2 iter) のままだと
//! 1 テスト 200-700ms 要するため、テスト専用 low-cost params で高速化する
//! (`Argon2idAdapter::new(test_params)`)。本番経路の Argon2id 強度は
//! shikomi-infra TC-D-I01 / TC-CI bench-kdf で別途担保 (Sub-D 凍結契約)。
//!
//! ## ファイル構成 (ペガサス工程5指摘対応の責務分割、2026-04-26)
//!
//! `tests/sub_e_v2_integration/` 配下に責務別 module 分割。本ファイル (entry) は
//! `mod` 宣言のみで、cargo は単一 integration test binary としてビルドする
//! (Rust の `tests/foo.rs` + `tests/foo/bar.rs` 子モジュール慣習、`#[path = ...]`
//! 経由で明示パス指定)。
//!
//! - `sub_e_v2_integration/helpers.rs`        — 共通ヘルパ (DRY 集約)
//! - `sub_e_v2_integration/unlock.rs`         — TC-E-I01
//! - `sub_e_v2_integration/backoff.rs`        — TC-E-I02 / I02b
//! - `sub_e_v2_integration/lock_lifecycle.rs` — TC-E-I04 / I05
//! - `sub_e_v2_integration/handshake.rs`      — TC-E-I07 / I09
//! - `sub_e_v2_integration/rekey_rotate.rs`   — TC-E-I06 / I08 (+ cache_relocked 経路)
//! - `sub_e_v2_integration/sanity.rs`         — TempDir lifecycle 1 件

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod common;

#[path = "sub_e_v2_integration/helpers.rs"]
mod helpers;

#[path = "sub_e_v2_integration/backoff.rs"]
mod backoff;
#[path = "sub_e_v2_integration/handshake.rs"]
mod handshake;
#[path = "sub_e_v2_integration/lock_lifecycle.rs"]
mod lock_lifecycle;
#[path = "sub_e_v2_integration/rekey_rotate.rs"]
mod rekey_rotate;
#[path = "sub_e_v2_integration/sanity.rs"]
mod sanity;
#[path = "sub_e_v2_integration/unlock.rs"]
mod unlock;
