//! `--output print` 経路: ハイコントラスト PDF 生成 (C-39, Sub-F #44 Phase 7)。
//!
//! 設計根拠:
//! - docs/features/vault-encryption/detailed-design/cli-subcommands.md
//!   §アクセシビリティ代替経路 `--output print` 行
//!   (ハイコントラスト PDF: 黒地白文字、最大 36pt、各語に番号付与)
//!   §セキュリティ設計 §`/tmp` 経由の中間ファイル
//!   (PDF はメモリ上で生成して stdout に直接書出、ユーザのリダイレクト先以外に
//!   ディスクヒットしない)
//!
//! 採用方針 (Phase 7):
//! - **手書き PDF 1.4** で実装。`printpdf` 等の crate を **採用しない**。
//!   24 語の番号付きテキスト表示は PDF Content Stream の数十行で完結し、
//!   依存ツリー汚染 / Cargo.lock 肥大化 / audit (deny.toml multiple-versions) を
//!   回避する設計判断 (cli-subcommands.md §セキュリティ設計 §新規依存の監査の
//!   「unsafe FFI 経路を増やさない」と同方針: 不必要な依存も増やさない)。
//! - **ハイコントラスト**: 黒背景 (`0 0 0 rg`) に白文字 (`1 1 1 rg`) で描画。
//!   フォントは PDF 標準 14 base font の `Helvetica` (PDF Reader 全互換、
//!   ファイル内に font glyph 埋込不要 = サイズ最小)。
//! - **中間ファイルなし**: `Vec<u8>` でメモリ生成 → stdout 直接書出。
//! - **umask 適用**: 呼出側 (`accessibility::umask::with_secure_umask`) の責務。
//!
//! Phase 7 スコープ外:
//! - PDF/A 準拠 (印刷アーカイブ向け国際標準) は次 minor バージョンで検討
//! - 多言語フォント埋込 (英文 BIP-39 word のみ前提なので不要)
//! - 暗号化 PDF (ユーザの `> recovery.pdf` リダイレクト先 0600 で代替)

use std::fmt::Write as _;
use std::io::Write as _;

use shikomi_core::ipc::SerializableSecretBytes;

use crate::error::CliError;

/// 24 語をハイコントラスト PDF として stdout に書出す。
///
/// # Errors
/// stdout 書出失敗時に `CliError::Persistence`。
pub fn write_to_stdout(words: &[SerializableSecretBytes]) -> Result<(), CliError> {
    let pdf = build_pdf_bytes(words);
    let mut out = std::io::stdout().lock();
    out.write_all(&pdf).map_err(|e| {
        CliError::Persistence(shikomi_infra::persistence::PersistenceError::Io {
            path: std::path::PathBuf::from("<stdout:print_pdf>"),
            source: e,
        })
    })
}

/// 24 語を PDF 1.4 バイナリにエンコードする pure 関数 (テスト容易性)。
///
/// PDF 構造:
/// 1. Header: `%PDF-1.4` + binary marker
/// 2. Catalog (object 1)
/// 3. Pages (object 2)
/// 4. Page (object 3) — A4 (595 x 842 pts)、黒背景描画 + 白文字
/// 5. Font (object 4) — Helvetica (PDF base 14 font、glyph 埋込不要)
/// 6. Content stream (object 5) — BT/ET で 24 行を描画
/// 7. xref + trailer
#[must_use]
pub fn build_pdf_bytes(words: &[SerializableSecretBytes]) -> Vec<u8> {
    let content = build_content_stream(words);
    let content_len = content.len();

    let mut pdf = String::new();
    // PDF Header: 1.4 + binary marker (4 bytes ≥ 128 で binary 認識を強制)
    pdf.push_str("%PDF-1.4\n");
    pdf.push_str("%\u{00E2}\u{00E3}\u{00CF}\u{00D3}\n"); // âãÏÓ binary sentinel

    // 各 object の byte offset 記録 (xref 用)
    let mut offsets: Vec<usize> = Vec::with_capacity(5);

    offsets.push(pdf.len());
    pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.push_str(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
         /Contents 5 0 R /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n",
    );

    offsets.push(pdf.len());
    pdf.push_str("4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");

    offsets.push(pdf.len());
    let _ = write!(pdf, "5 0 obj\n<< /Length {content_len} >>\nstream\n");
    pdf.push_str(&content);
    pdf.push_str("\nendstream\nendobj\n");

    // xref table
    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 6\n");
    pdf.push_str("0000000000 65535 f \n");
    for off in &offsets {
        let _ = writeln!(pdf, "{off:010} 00000 n ");
    }

    // trailer
    pdf.push_str("trailer\n");
    pdf.push_str("<< /Size 6 /Root 1 0 R >>\n");
    let _ = write!(pdf, "startxref\n{xref_offset}\n%%EOF\n");

    pdf.into_bytes()
}

/// PDF Content Stream を構築する (黒背景 + 白テキスト 24 行)。
///
/// PDF 描画コマンド:
/// - `q` / `Q`: graphics state save / restore
/// - `0 0 0 rg`: fill color black
/// - `0 0 595 842 re f`: 黒矩形で background (A4 全面)
/// - `1 1 1 rg`: fill color white
/// - `BT ... ET`: text block
/// - `/F1 18 Tf`: font Helvetica, size 18pt
/// - `Td`, `Tj`: text positioning + show
fn build_content_stream(words: &[SerializableSecretBytes]) -> String {
    let mut s = String::new();
    // 1. graphics state save + 黒背景描画
    s.push_str("q\n0 0 0 rg\n0 0 595 842 re\nf\n");
    // 2. 白文字フォント設定 (18pt、24 行で A4 縦に収まる行間)
    s.push_str("1 1 1 rg\nBT\n/F1 18 Tf\n22 TL\n");
    // 3. 開始位置 (上端 50pt 余白、左端 60pt)
    s.push_str("60 800 Td\n");
    // 4. 24 語を 1 行ずつ描画
    for (i, w) in words.iter().enumerate() {
        let plain = w.to_lossy_string_for_handler();
        // PDF 文字列リテラルでの () \\ エスケープ
        let escaped = pdf_text_escape(&plain);
        let line_no = i + 1;
        let _ = write!(
            s,
            "({line_no:>2}. {escaped}) Tj\nT*\n",
            line_no = line_no,
            escaped = escaped
        );
    }
    s.push_str("ET\nQ\n");
    s
}

/// PDF text string のエスケープ ( `(` / `)` / `\` のみ、ASCII 限定入力前提)。
fn pdf_text_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            // 非 ASCII 文字は `?` で置換 (BIP-39 wordlist は ASCII 英小文字のみ前提、
            // Fail Kindly で印刷停止しない)。
            ch if ch.is_ascii() && !ch.is_control() => out.push(ch),
            _ => out.push('?'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::SecretString;

    fn word(s: &str) -> SerializableSecretBytes {
        SerializableSecretBytes::from_secret_string(SecretString::from_string(s.to_owned()))
    }

    #[test]
    fn test_build_pdf_starts_with_pdf_14_header() {
        let pdf = build_pdf_bytes(&[word("alpha")]);
        let head = std::str::from_utf8(&pdf[..8]).expect("ascii header");
        assert_eq!(head, "%PDF-1.4");
    }

    #[test]
    fn test_build_pdf_ends_with_eof_marker() {
        let pdf = build_pdf_bytes(&[word("alpha")]);
        let tail = std::str::from_utf8(&pdf[pdf.len() - 6..])
            .expect("ascii tail")
            .trim();
        assert!(
            tail.ends_with("%%EOF"),
            "expected %%EOF tail, got: {tail:?}"
        );
    }

    #[test]
    fn test_build_pdf_contains_24_word_entries() {
        let words: Vec<_> = (0..24).map(|i| word(&format!("word{i}"))).collect();
        let pdf_str = String::from_utf8_lossy(&build_pdf_bytes(&words)).into_owned();
        // 各番号 (1.〜24.) が Content Stream に存在する。
        for i in 1..=24 {
            assert!(
                pdf_str.contains(&format!("{i:>2}. word")),
                "missing word entry {i}: {pdf_str}"
            );
        }
    }

    #[test]
    fn test_pdf_text_escape_handles_parens_and_backslash() {
        assert_eq!(pdf_text_escape("a(b)c\\d"), "a\\(b\\)c\\\\d");
    }

    #[test]
    fn test_pdf_text_escape_replaces_non_ascii_with_question() {
        assert_eq!(pdf_text_escape("héllo"), "h?llo");
    }

    #[test]
    fn test_build_pdf_includes_helvetica_font() {
        let pdf_str = String::from_utf8_lossy(&build_pdf_bytes(&[word("a")])).into_owned();
        assert!(pdf_str.contains("/BaseFont /Helvetica"));
    }

    #[test]
    fn test_build_pdf_high_contrast_black_bg_white_text() {
        let pdf_str = String::from_utf8_lossy(&build_pdf_bytes(&[word("a")])).into_owned();
        // 黒背景 (0 0 0 rg) + 白文字 (1 1 1 rg) の両方が Content Stream に存在。
        assert!(pdf_str.contains("0 0 0 rg"));
        assert!(pdf_str.contains("1 1 1 rg"));
    }
}
