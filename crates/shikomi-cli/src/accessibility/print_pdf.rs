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
use zeroize::Zeroizing;

use crate::error::CliError;

/// 24 語をハイコントラスト PDF として stdout に書出す。
///
/// 工程5 服部指摘 (BLOCKER 3) 解消: PDF バイト列は `Zeroizing<Vec<u8>>` で構築し、
/// drop で自動 zeroize される。`SerializableSecretBytes` 中の 24 語は `String`
/// 経由を一切せず `expose_secret() -> &[u8]` から直接 byte を消費する経路に統一。
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
    // pdf は drop で zeroize (Zeroizing<Vec<u8>>)
}

/// 24 語を PDF 1.4 バイナリにエンコードする pure 関数 (テスト容易性)。
///
/// 戻り値は `Zeroizing<Vec<u8>>` で drop 時に自動 zeroize される。
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
pub fn build_pdf_bytes(words: &[SerializableSecretBytes]) -> Zeroizing<Vec<u8>> {
    let content = build_content_stream(words);
    let content_len = content.len();

    let mut pdf: Vec<u8> = Vec::new();
    // PDF Header: 1.4 + binary marker (4 bytes ≥ 128 で binary 認識を強制)
    pdf.extend_from_slice(b"%PDF-1.4\n");
    pdf.extend_from_slice(&[b'%', 0xE2, 0xE3, 0xCF, 0xD3, b'\n']); // binary sentinel

    // 各 object の byte offset 記録 (xref 用)
    let mut offsets: Vec<usize> = Vec::with_capacity(5);

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 5 0 R /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n",
    );

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );

    offsets.push(pdf.len());
    let header = format!("5 0 obj\n<< /Length {content_len} >>\nstream\n");
    pdf.extend_from_slice(header.as_bytes());
    pdf.extend_from_slice(&content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // xref table
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        let line = format!("{off:010} 00000 n \n");
        pdf.extend_from_slice(line.as_bytes());
    }

    // trailer
    pdf.extend_from_slice(b"trailer\n");
    pdf.extend_from_slice(b"<< /Size 6 /Root 1 0 R >>\n");
    let trailer = format!("startxref\n{xref_offset}\n%%EOF\n");
    pdf.extend_from_slice(trailer.as_bytes());

    Zeroizing::new(pdf)
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
/// PDF Content Stream を `Zeroizing<Vec<u8>>` で構築する。
///
/// 工程5 BLOCKER 3 解消: `SerializableSecretBytes::expose_secret() -> &[u8]` を
/// 直接消費し、中間 `String` を作らない。byte 単位で PDF text escape を実施。
fn build_content_stream(words: &[SerializableSecretBytes]) -> Zeroizing<Vec<u8>> {
    let mut s: Vec<u8> = Vec::new();
    // 1. graphics state save + 黒背景描画
    s.extend_from_slice(b"q\n0 0 0 rg\n0 0 595 842 re\nf\n");
    // 2. 白文字フォント設定 (18pt、24 行で A4 縦に収まる行間)
    s.extend_from_slice(b"1 1 1 rg\nBT\n/F1 18 Tf\n22 TL\n");
    // 3. 開始位置 (上端 50pt 余白、左端 60pt)
    s.extend_from_slice(b"60 800 Td\n");
    // 4. 24 語を 1 行ずつ描画 (expose_secret 経由で `&[u8]` 直接消費)
    for (i, w) in words.iter().enumerate() {
        let line_no = i + 1;
        let prefix = format!("({line_no:>2}. ");
        s.extend_from_slice(prefix.as_bytes());
        // 平文 byte をそのまま escape して PDF Content Stream に書く。
        // `to_lossy_string_for_handler()` 経路 (heap String) は採用しない。
        let bytes = w.inner().expose_secret();
        push_pdf_text_escaped(&mut s, bytes);
        s.extend_from_slice(b") Tj\nT*\n");
    }
    s.extend_from_slice(b"ET\nQ\n");
    Zeroizing::new(s)
}

/// PDF text string のエスケープを `Vec<u8>` に in-place で書込む。
///
/// `(` / `)` / `\` (PDF 予約文字) のみエスケープ。非 ASCII / 制御文字は `?` 置換
/// (BIP-39 wordlist は ASCII 英小文字のみ前提、Fail Kindly で印刷停止しない)。
fn push_pdf_text_escaped(out: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            ch if ch.is_ascii() && (b >= 0x20) && (b != 0x7F) => out.push(ch),
            _ => out.push(b'?'),
        }
    }
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
    fn test_push_pdf_text_escaped_handles_parens_and_backslash() {
        let mut out = Vec::new();
        push_pdf_text_escaped(&mut out, b"a(b)c\\d");
        assert_eq!(out, b"a\\(b\\)c\\\\d");
    }

    #[test]
    fn test_push_pdf_text_escaped_replaces_non_ascii_with_question() {
        let mut out = Vec::new();
        // "héllo" の é は 0xC3 0xA9 (UTF-8 2 byte)、両 byte が `?` 置換 = 6 文字
        push_pdf_text_escaped(&mut out, "héllo".as_bytes());
        assert_eq!(out, b"h??llo");
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
