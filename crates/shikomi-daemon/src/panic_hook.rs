//! daemon 側 panic hook（CLI と同型 fixed-message）。
//!
//! 設計根拠:
//! - docs/features/daemon-ipc/basic-design/security.md §panic hook と secret 漏洩経路の遮断
//! - docs/features/daemon-ipc/detailed-design/composition-root.md §`panic_hook::install`
//!
//! 厳守事項:
//! - `info.payload()` / `info.message()` / `info.location()` を**一切参照しない**
//! - `tracing::*!` マクロを**呼ばない**（subscriber 状態に依存しないため `eprintln!` のみ）
//! - 出力は固定文言のみ（英語、daemon は運用者向け）

use std::io::Write;

/// daemon 用 panic hook を登録する。
///
/// `run()` の最初の行で呼ばれることを想定（tokio runtime 初期化前 / その他依存 crate
/// の初期化での panic も捕捉する）。
pub fn install() {
    std::panic::set_hook(Box::new(panic_hook));
}

// MSRV 1.80 のため `PanicInfo` を使用（`PanicHookInfo` は 1.81 stable）。
// どちらの場合も `info.payload()` / `info.message()` / `info.location()` を非参照とする契約は同じ。
#[allow(deprecated)]
fn panic_hook(_info: &std::panic::PanicInfo<'_>) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "error: shikomi-daemon internal bug");
    let _ = writeln!(
        stderr,
        "hint: please report this issue to https://github.com/shikomi-dev/shikomi/issues"
    );
}
