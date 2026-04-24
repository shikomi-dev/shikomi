//! shikomi CLI バイナリエントリポイント。
//!
//! コンポジションルート本体は `shikomi_cli::run()` にある。本 main 関数は
//! プロセス起動 → `run()` 呼び出し → 終了コード返却のみを担う（3 行ラッパ）。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/composition-root.md
//! §`run()` の外側（`main.rs`）

use shikomi_cli::ExitCode;

fn main() -> ExitCode {
    shikomi_cli::run()
}
