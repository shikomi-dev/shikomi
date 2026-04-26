//! UseCase 層。ドメイン操作の orchestration のみを担い、I/O・TTY・clap の知識を持たない。
//!
//! 各 UseCase は `&dyn VaultRepository` / 入力 DTO / `now: OffsetDateTime` を引数に取る
//! pure に近い関数（実 Repository は I/O を持つが、UseCase 自身は副作用を持たない）。
//! これにより UseCase 単位で固定時刻・モック Repository を使った結合テストが書ける。
//!
//! 設計根拠: docs/features/cli-vault-commands/basic-design/index.md
//! §モジュール設計方針

pub mod add;
pub mod edit;
pub mod list;
pub mod remove;
pub mod vault;
