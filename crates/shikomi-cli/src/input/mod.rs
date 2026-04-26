//! 入力 DTO + パスワード/24語/確認語句のプロンプト関数群。
//!
//! 構成:
//! - `dto`: UseCase への入力 DTO（Phase 1 既存、`input.rs` から Phase 5 で昇格）。
//!   既存の re-export (`AddInput` / `EditInput` / `ConfirmedRemoveInput`) は本 mod.rs
//!   を経由して維持され、外部 API は不変。
//! - `password`: TTY 強制 + 非 TTY 時 fail fast (C-38) のパスワード入力。
//!
//! 設計根拠:
//! - docs/features/cli-vault-commands/detailed-design/public-api.md §`shikomi_cli::input`
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §モジュール配置と責務 / §セキュリティ設計 §shell history § C-38

pub mod dto;
pub mod password;

pub use dto::{AddInput, ConfirmedRemoveInput, EditInput};
