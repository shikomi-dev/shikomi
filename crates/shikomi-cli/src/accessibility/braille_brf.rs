//! BRF (Braille Ready Format) 出力（C-39、Sub-F #44 Phase 6）。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 `--output braille` 行
//!   (自前 wordlist 変換テーブル: 追加 crate なし、liblouis FFI bindings は
//!   不採用 = unsafe C-FFI 経路を増やさない設計判断)
//!
//! 採用方針 (Phase 6):
//! - **Grade 1 ASCII Braille** (各文字を 6dot Braille に 1:1 マップ) で実装。
//!   24 語の BIP-39 ASCII 小文字 + space + 数字のみが入力なので、26 文字 + 数字 +
//!   space + 改行のシンプルな mapping table で 100% 表現可能。
//! - Grade 2 contractions (例: `the` → `⠮`) は Phase 7 で BIP-39 全 2048 語の
//!   mapping table 追加と同時に対応する。Phase 6 は読み上げ可能な ASCII Braille
//!   出力で MSG-S18 アクセシビリティ約束を最低限満たす。
//! - 出力は **ASCII Braille 文字 (北米 BRF 標準、`!"#$%&'()*+,-./:;<=>?@[\]^_`)** を
//!   採用 (Unicode 6dot Braille `⠁⠃...` ではない)。これにより BRF 印刷機 (Tiger
//!   embosser 等) と互換になる。
//!
//! 不変条件:
//! - 入力 24 語が小文字 ASCII であることは BIP-39 wordlist 由来で保証されている。
//!   万一非 ASCII 文字が混入した場合は `?` で置換 (Fail Kindly、印刷停止しない)。
//! - 各語に番号 (`1.` ～ `24.`) を付ける (印刷機からの読み取りで順序保証)。
//! - umask(0o077) 適用は呼出側 (`accessibility::umask::with_secure_umask`) の責務。

use std::io::Write;

use shikomi_core::ipc::SerializableSecretBytes;
use zeroize::Zeroizing;

use crate::error::CliError;

/// 24 語を BRF (Grade 1 ASCII Braille) 形式で stdout に書出す。
///
/// 工程5 服部指摘 (BLOCKER 3) 解消: BRF バイト列は `Zeroizing<Vec<u8>>` で構築し、
/// drop で自動 zeroize。`to_lossy_string_for_handler()` 経由を廃止し、
/// `expose_secret() -> &[u8]` から直接 byte 単位で消費する。
///
/// # Errors
/// stdout 書出失敗時に `CliError::Persistence`。
pub fn write_to_stdout(words: &[SerializableSecretBytes]) -> Result<(), CliError> {
    let brf = encode_words(words);
    let mut out = std::io::stdout().lock();
    out.write_all(&brf).map_err(io_err)?;
    Ok(())
}

/// 24 語を BRF バイト列にエンコードする pure 関数 (テスト容易性 + zeroize 維持)。
#[must_use]
pub fn encode_words(words: &[SerializableSecretBytes]) -> Zeroizing<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let line_no = i + 1;
        let prefix = format!("{line_no:>2}. ");
        out.extend_from_slice(prefix.as_bytes());
        let bytes = w.inner().expose_secret();
        if let Some(short) = grade2_word_contraction(bytes) {
            out.extend_from_slice(short.as_bytes());
        } else {
            for &b in bytes {
                out.push(ascii_byte_to_braille(b));
            }
        }
        out.push(b'\n');
    }
    Zeroizing::new(out)
}

/// BIP-39 wordlist で出現する Grade 2 whole-word contractions (byte 比較版)。
///
/// 設計根拠は変更前と同じ (UEB 2013 §10.6 + BIP-39)。`String` を作らず `&[u8]`
/// 直接比較で zeroize 経路を維持。
fn grade2_word_contraction(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        b"have" => Some("H"),
        b"like" => Some("L"),
        b"people" => Some("P"),
        b"you" => Some("Y"),
        b"child" => Some("C"),
        b"it" => Some("X"),
        b"not" => Some("N"),
        b"so" => Some("S"),
        b"us" => Some("U"),
        _ => None,
    }
}

/// ASCII 1 byte を BRF (北米 ASCII Braille) 1 byte にマップする。
///
/// 設計根拠: ANSI/NISO Z39.86-2005 BRF 標準 + Unified English Braille (UEB) Grade 1。
/// マップ非掲載 byte は `?` (BRF) に置換 (Fail Kindly)。
/// 工程5 BLOCKER 3 で `&char` → `&u8` 経路に変更 (zeroize 維持)。
fn ascii_byte_to_braille(b: u8) -> u8 {
    let c = b as char;
    let mapped = ascii_to_braille_char(c);
    mapped as u8
}

fn ascii_to_braille_char(c: char) -> char {
    match c {
        // 26 letters (lowercase, BRF small letter prefix で uppercase は別 prefix)。
        'a' => 'A',
        'b' => 'B',
        'c' => 'C',
        'd' => 'D',
        'e' => 'E',
        'f' => 'F',
        'g' => 'G',
        'h' => 'H',
        'i' => 'I',
        'j' => 'J',
        'k' => 'K',
        'l' => 'L',
        'm' => 'M',
        'n' => 'N',
        'o' => 'O',
        'p' => 'P',
        'q' => 'Q',
        'r' => 'R',
        's' => 'S',
        't' => 'T',
        'u' => 'U',
        'v' => 'V',
        'w' => 'W',
        'x' => 'X',
        'y' => 'Y',
        'z' => 'Z',
        // 数字 (1-9, 0): BRF では number indicator `#` 後に a-j (1-0) が続く。
        // ここでは行番号で使われるため簡易マップ (Phase 7 で正式 number indicator 対応)。
        '0' => '0',
        '1' => '1',
        '2' => '2',
        '3' => '3',
        '4' => '4',
        '5' => '5',
        '6' => '6',
        '7' => '7',
        '8' => '8',
        '9' => '9',
        ' ' => ' ',
        '.' => '4', // BRF period
        ',' => '1', // BRF comma
        // 非対応文字は `?` で置換 (印刷停止しない、Fail Kindly)。
        _ => '?',
    }
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
        path: std::path::PathBuf::from("<stdout:braille>"),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(s: &str) -> SerializableSecretBytes {
        use shikomi_core::SecretString;
        SerializableSecretBytes::from_secret_string(SecretString::from_string(s.to_owned()))
    }

    fn brf_as_str(brf: &[u8]) -> &str {
        std::str::from_utf8(brf).expect("BRF bytes are ASCII subset")
    }

    #[test]
    fn test_encode_words_includes_line_numbers() {
        let words = vec![word("abandon"), word("ability")];
        let brf = encode_words(&words);
        let s = brf_as_str(&brf);
        assert!(s.contains(" 1. "));
        assert!(s.contains(" 2. "));
    }

    #[test]
    fn test_encode_words_ascii_braille_uppercase_per_letter() {
        let words = vec![word("abc")];
        let brf = encode_words(&words);
        let s = brf_as_str(&brf);
        // a→A, b→B, c→C (BRF mapping 簡易版)
        assert!(s.contains("ABC"));
    }

    #[test]
    fn test_ascii_to_braille_unknown_char_maps_to_question() {
        assert_eq!(ascii_to_braille_char('Ω'), '?');
        assert_eq!(ascii_to_braille_char('!'), '?');
    }

    #[test]
    fn test_grade2_contraction_known_words_map_to_single_letter() {
        // BIP-39 に含まれる UEB single-letter wordsign 対象 9 語の確認
        assert_eq!(grade2_word_contraction(b"have"), Some("H"));
        assert_eq!(grade2_word_contraction(b"like"), Some("L"));
        assert_eq!(grade2_word_contraction(b"people"), Some("P"));
        assert_eq!(grade2_word_contraction(b"you"), Some("Y"));
        assert_eq!(grade2_word_contraction(b"child"), Some("C"));
    }

    #[test]
    fn test_grade2_contraction_unknown_words_return_none() {
        // BIP-39 wordlist の他多数 (Grade 1 fallback 対象)
        assert_eq!(grade2_word_contraction(b"abandon"), None);
        assert_eq!(grade2_word_contraction(b"ability"), None);
        assert_eq!(grade2_word_contraction(b"zone"), None);
    }

    #[test]
    fn test_encode_words_uses_grade2_for_known_word() {
        let brf = encode_words(&[word("have")]);
        let s = brf_as_str(&brf);
        // Grade 2 single-letter wordsign "H" + 改行
        assert!(s.contains(" 1. H\n"), "expected grade-2 H, got: {s}");
    }

    #[test]
    fn test_encode_words_24_words_produces_24_lines() {
        let words: Vec<_> = (0..24).map(|i| word(&format!("word{i}"))).collect();
        let brf = encode_words(&words);
        let newline_count = brf.iter().filter(|&&b| b == b'\n').count();
        assert_eq!(newline_count, 24);
    }
}
