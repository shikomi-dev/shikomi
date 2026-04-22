//! 静的確認テスト: deny.toml の暗号クリティカル crate 登録確認（TC-I07）
//! REQ-009 / AC-09
//!
//! deny.toml の [advisories] コメントに secrecy / zeroize が明記され、
//! かつ ignore = [] リストに含まれていないことを検証する。

use std::fs;

/// TC-I07: deny.toml に secrecy / zeroize がコメントに明記されている
#[test]
fn test_deny_toml_lists_secrecy_and_zeroize_as_crypto_critical_in_comments() {
    // deny.toml はリポジトリルートに存在する
    // integration test は crates/shikomi-core/ から実行されるため、
    // ../../deny.toml が相対パス
    let deny_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deny.toml");
    let content = fs::read_to_string(deny_path).expect("deny.toml should exist at repository root");

    // secrecy / zeroize がコメント行に登場することを確認
    let has_secrecy_in_comment = content
        .lines()
        .any(|line| line.trim_start().starts_with('#') && line.contains("secrecy"));
    let has_zeroize_in_comment = content
        .lines()
        .any(|line| line.trim_start().starts_with('#') && line.contains("zeroize"));

    assert!(
        has_secrecy_in_comment,
        "deny.toml must mention 'secrecy' in a comment line (crypto-critical crate prohibition)"
    );
    assert!(
        has_zeroize_in_comment,
        "deny.toml must mention 'zeroize' in a comment line (crypto-critical crate prohibition)"
    );
}

/// TC-I07: deny.toml の ignore リストに secrecy / zeroize が含まれていない
#[test]
fn test_deny_toml_ignore_list_does_not_contain_secrecy_or_zeroize() {
    let deny_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deny.toml");
    let content = fs::read_to_string(deny_path).expect("deny.toml should exist at repository root");

    // ignore = [...] セクション内に secrecy / zeroize が含まれないことを確認
    // （コメント行を除いた実際の ignore 値部分のみチェック）
    let in_ignore_block = content
        .lines()
        .skip_while(|line| !line.contains("ignore = ["))
        .take_while(|line| !line.contains(']') || line.contains("ignore = ["))
        .filter(|line| !line.trim_start().starts_with('#'))
        .any(|line| line.contains("secrecy") || line.contains("zeroize"));

    assert!(
        !in_ignore_block,
        "deny.toml ignore list must NOT contain secrecy or zeroize advisory IDs"
    );
}
