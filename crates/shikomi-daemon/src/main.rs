//! `shikomi-daemon` バイナリエントリ。
//!
//! 設計根拠: docs/features/daemon-ipc/detailed-design/composition-root.md §`main.rs`

use std::process::ExitCode;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    shikomi_daemon::run().await
}
