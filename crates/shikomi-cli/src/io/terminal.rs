//! TTY 判定 / 通常 readline / 非エコー入力。
//!
//! テスト差し替えのための trait 化は行わず（YAGNI）、関数ベースで薄く wrap する。
//! 返却する秘密値は即座に `SecretString` で包み、平文 String が返値に漏れないようにする。

use std::io::{self, BufRead, Write};

use is_terminal::IsTerminal;
use shikomi_core::SecretString;

/// stdin が TTY 接続かどうかを返す。
#[must_use]
pub fn is_stdin_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// プロンプトを stdout に出し、stdin から 1 行読む。末尾の `\n` / `\r\n` を trim。
///
/// # Errors
/// stdin / stdout の IO エラーを透過する。
pub fn read_line(prompt: &str) -> io::Result<String> {
    if !prompt.is_empty() {
        let mut out = io::stdout().lock();
        out.write_all(prompt.as_bytes())?;
        out.flush()?;
    }
    let stdin = io::stdin();
    let mut buf = String::new();
    stdin.lock().read_line(&mut buf)?;
    // 末尾改行の除去
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
}

/// 非エコー入力で Secret 値を得る（Unix: `termios` / Windows: `SetConsoleMode`）。
///
/// TTY でない場合 `rpassword` は通常 readline にフォールバックする。返り値は即座に
/// `SecretString` に包んで返す。中間 String はスタックに短期間存在するが、`rpassword`
/// の内部実装上 zeroize は保証しない（`docs/.../basic-design/security.md §依存 crate`
/// の §1 参照）。
///
/// # Errors
/// `rpassword::prompt_password` が返した IO エラーを透過する。
pub fn read_password(prompt: &str) -> io::Result<SecretString> {
    let raw = rpassword::prompt_password(prompt)?;
    Ok(SecretString::from_string(raw))
}
