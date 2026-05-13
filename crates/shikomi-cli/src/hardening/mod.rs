//! プロセス起動時のセキュリティハードニング群（Sub-F #44 Phase 5）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §セキュリティ設計 §不変条件・契約 C-41
//! - docs/features/vault-encryption/basic-design/security.md §脅威 L1 §core dump
//!
//! 構成:
//! - `core_dump`: OS 別の core dump 抑制呼出（C-41）。
//!
//! 各 hardening は **`shikomi_cli::run()` の最初**で呼び出すこと。プロセス起動の
//! 最早期で適用することで、後続の vault unlock / decrypt 等で VEK が常駐する前に
//! 防衛が成立する。

pub mod core_dump;
