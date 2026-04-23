//! vault-persistence 結合テスト — TC-I01, I02, I08, I09, I16〜I19
//! ラウンドトリップ・基本 CRUD・環境変数設定
mod helpers;
use helpers::{ENV_MUTEX, make_plaintext_vault, make_record, make_repo, plaintext_header};
use shikomi_core::{Record, RecordPayload, Vault};
use shikomi_infra::persistence::{PersistenceError, SqliteVaultRepository};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// TC-I01: 公開 API ドキュメント確認
// ---------------------------------------------------------------------------

/// TC-I01 — `cargo doc` が exit code 0 で完了し、主要型のドキュメントが生成される。
///
/// AC-01 対応。
#[test]
fn tc_i01_cargo_doc() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "/tmp/shikomi/crates/shikomi-infra".to_string());
    let status = std::process::Command::new("cargo")
        .args(["doc", "-p", "shikomi-infra", "--no-deps"])
        .current_dir(&manifest)
        .status()
        .expect("cargo を実行できませんでした");
    assert!(status.success(), "cargo doc が失敗しました: {status:?}");
}

// ---------------------------------------------------------------------------
// TC-I02: 平文 vault round-trip（レコード 5 件）
// ---------------------------------------------------------------------------

/// TC-I02 — save → load で平文 vault が完全に復元される。
///
/// AC-02 対応。
#[test]
fn tc_i02_plaintext_vault_round_trip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(5);

    repo.save(&vault).unwrap();
    let loaded = repo.load().unwrap();

    // ヘッダ確認
    assert_eq!(
        vault.header().protection_mode(),
        loaded.header().protection_mode()
    );
    assert_eq!(
        vault.header().version().value(),
        loaded.header().version().value()
    );

    // レコード件数確認
    assert_eq!(
        vault.records().len(),
        loaded.records().len(),
        "レコード件数が一致しない"
    );

    // レコード内容確認（ID をキーに照合。DB の返却順は created_at ASC, id ASC のため
    // 同一ミリ秒に複数レコードを作成すると挿入順と一致しない場合があり、
    // zip 比較ではなく HashMap で順序非依存に照合する）。
    let loaded_by_id: std::collections::HashMap<String, &Record> = loaded
        .records()
        .iter()
        .map(|r| (r.id().to_string(), r))
        .collect();

    for orig in vault.records() {
        let id_str = orig.id().to_string();
        let loaded_rec = loaded_by_id
            .get(&id_str)
            .unwrap_or_else(|| panic!("RecordId {id_str} が load 結果に見つからない"));
        assert_eq!(
            orig.label().as_str(),
            loaded_rec.label().as_str(),
            "label が一致しない (id={id_str})"
        );
        // payload は SecretString の Eq 未実装のためフォーマット比較
        if let (RecordPayload::Plaintext(a), RecordPayload::Plaintext(b)) =
            (orig.payload(), loaded_rec.payload())
        {
            assert_eq!(
                a.expose_secret(),
                b.expose_secret(),
                "plaintext_value が一致しない (id={id_str})"
            );
        } else {
            panic!("平文モードなのに暗号化ペイロードが返った (id={id_str})");
        }
    }
}

// ---------------------------------------------------------------------------
// TC-I08: UTF-8 特殊文字ラベルの round-trip
// ---------------------------------------------------------------------------

/// TC-I08 — 絵文字・CJK・アラビア文字を含むラベルが byte 完全一致で復元される。
///
/// AC-08 対応。
#[test]
fn tc_i08_utf8_label_round_trip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());

    let label_str = "🗝️秘密のキー💀شيكومي";
    let mut vault = Vault::new(plaintext_header());
    vault
        .add_record(make_record(label_str, "secret_value"))
        .unwrap();

    repo.save(&vault).unwrap();
    let loaded = repo.load().unwrap();

    assert_eq!(loaded.records().len(), 1);
    assert_eq!(
        loaded.records()[0].label().as_str(),
        label_str,
        "ラベルがバイト単位で一致しない"
    );
}

// ---------------------------------------------------------------------------
// TC-I09: SQL インジェクション禁止設計の静的 grep 確認
// ---------------------------------------------------------------------------

/// TC-I09 — ソース内に SQL 文字列連結パターンが存在しないことを確認する。
///
/// AC-09 対応。
#[test]
fn tc_i09_no_sql_injection_patterns() {
    let src_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "/tmp/shikomi/crates/shikomi-infra".to_string());
    let src_path = format!("{src_dir}/src");

    // format! マクロで SQL キーワードを組み立てているパターン
    let status1 = std::process::Command::new("grep")
        .args([
            "-rEn",
            r#"format!\s*\(.*(?:SELECT|INSERT|UPDATE|DELETE|PRAGMA)"#,
            "--include=*.rs",
            &src_path,
        ])
        .status()
        .expect("grep を実行できませんでした");
    // grep は一致なし = exit 1 が期待値
    assert!(
        !status1.success(),
        "SQL 連結パターン (format!) が検出された — SQL インジェクションリスク"
    );

    // 文字列連結で SQL を組み立てているパターン
    let status2 = std::process::Command::new("grep")
        .args([
            "-rEn",
            r#""[^"]*(?:SELECT|INSERT|UPDATE|DELETE)[^"]*"\s*\+"#,
            "--include=*.rs",
            &src_path,
        ])
        .status()
        .expect("grep を実行できませんでした");
    assert!(
        !status2.success(),
        "SQL 連結パターン (+ 演算子) が検出された — SQL インジェクションリスク"
    );
}

// ---------------------------------------------------------------------------
// TC-I16: exists() — vault 非存在
// ---------------------------------------------------------------------------

/// TC-I16 — 空の tempdir で `exists()` を呼ぶと Ok(false) が返る。
#[test]
fn tc_i16_exists_returns_false_when_no_vault() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    assert!(!repo.exists().unwrap());
}

// ---------------------------------------------------------------------------
// TC-I17: exists() — vault 存在
// ---------------------------------------------------------------------------

/// TC-I17 — save 後に `exists()` を呼ぶと Ok(true) が返る。
#[test]
fn tc_i17_exists_returns_true_after_save() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = make_plaintext_vault(1);

    repo.save(&vault).unwrap();
    assert!(repo.exists().unwrap());
}

// ---------------------------------------------------------------------------
// TC-I18: SHIKOMI_VAULT_DIR 環境変数 override
// ---------------------------------------------------------------------------

/// TC-I18 — `SHIKOMI_VAULT_DIR` で指定したディレクトリに vault.db が作成される。
///
/// `ENV_MUTEX` で直列化（`std::env::set_var` がグローバル状態を変更するため）。
#[test]
fn tc_i18_env_var_vault_dir_override() {
    let dir = TempDir::new().unwrap();
    let result = {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SHIKOMI_VAULT_DIR", dir.path().as_os_str());
        let r = (|| -> Result<(), PersistenceError> {
            let repo = SqliteVaultRepository::new()?;
            let vault = make_plaintext_vault(1);
            repo.save(&vault)?;
            Ok(())
        })();
        std::env::remove_var("SHIKOMI_VAULT_DIR");
        r
    };
    result.expect("SHIKOMI_VAULT_DIR 指定での save が失敗した");
    assert!(
        dir.path().join("vault.db").exists(),
        "指定ディレクトリに vault.db が作成されていない"
    );
}

// ---------------------------------------------------------------------------
// TC-I19: ゼロレコード vault round-trip
// ---------------------------------------------------------------------------

/// TC-I19 — レコードゼロの vault を save → load しても正常に復元される。
///
/// AC-02 の境界値ケース。
#[test]
fn tc_i19_zero_record_vault_round_trip() {
    let dir = TempDir::new().unwrap();
    let repo = make_repo(dir.path());
    let vault = Vault::new(plaintext_header());

    repo.save(&vault).unwrap();
    let loaded = repo.load().unwrap();

    assert!(loaded.records().is_empty(), "records が空でない");
    assert_eq!(
        loaded.header().protection_mode(),
        vault.header().protection_mode()
    );
}
