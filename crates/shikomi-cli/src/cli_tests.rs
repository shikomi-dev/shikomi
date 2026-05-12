//! cli.rs の単体テスト（`#[cfg(test)]` モジュール）。
//!
//! cli.rs から分離した理由: ペガサス 500 行ルール（cli.rs が 539 行超過）。
//!
//! 設計根拠: docs/features/cli-vault-commands/test-design/unit.md §TC-UT-150〜152

use super::*;

#[test]
fn test_kind_arg_text_maps_to_record_kind_text() {
    assert!(matches!(RecordKind::from(KindArg::Text), RecordKind::Text));
}

#[test]
fn test_kind_arg_secret_maps_to_record_kind_secret() {
    assert!(matches!(
        RecordKind::from(KindArg::Secret),
        RecordKind::Secret
    ));
}

#[test]
fn test_cli_args_parses_list_subcommand() {
    let args = CliArgs::try_parse_from(["shikomi", "list"]).unwrap();
    assert!(matches!(args.subcommand, Subcommand::List));
}

#[test]
fn test_cli_args_parses_add_subcommand_with_kind_label_value() {
    let args = CliArgs::try_parse_from([
        "shikomi", "add", "--kind", "text", "--label", "l", "--value", "v",
    ])
    .unwrap();
    assert!(matches!(args.subcommand, Subcommand::Add(_)));
}

#[test]
fn test_cli_args_remove_alias_rm_accepted() {
    let args = CliArgs::try_parse_from([
        "shikomi",
        "rm",
        "--id",
        "01234567-0123-7000-8000-0123456789ab",
    ])
    .unwrap();
    assert!(matches!(args.subcommand, Subcommand::Remove(_)));
}

// ---------------------------------------------------------------
// Sub-F (#44): VaultSubcommand clap 派生型の最小受理確認
// 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
// §clap 派生型構造（Subcommand 拡張）
// ---------------------------------------------------------------

#[test]
fn test_cli_args_parses_vault_encrypt_with_default_output() {
    let args = CliArgs::try_parse_from(["shikomi", "vault", "encrypt"]).unwrap();
    match args.subcommand {
        Subcommand::Vault(VaultSubcommand::Encrypt(a)) => {
            assert_eq!(a.output, OutputTarget::Screen);
            assert!(!a.accept_limits);
        }
        other => panic!("expected Vault(Encrypt(_)), got {other:?}"),
    }
}

#[test]
fn test_cli_args_parses_vault_encrypt_with_accept_limits_and_braille() {
    let args = CliArgs::try_parse_from([
        "shikomi",
        "vault",
        "encrypt",
        "--accept-limits",
        "--output",
        "braille",
    ])
    .unwrap();
    match args.subcommand {
        Subcommand::Vault(VaultSubcommand::Encrypt(a)) => {
            assert!(a.accept_limits);
            assert_eq!(a.output, OutputTarget::Braille);
        }
        other => panic!("expected Vault(Encrypt(_)), got {other:?}"),
    }
}

#[test]
fn test_cli_args_parses_vault_decrypt_lock_change_password() {
    for sub in ["decrypt", "lock", "change-password"] {
        let args = CliArgs::try_parse_from(["shikomi", "vault", sub]).unwrap();
        assert!(matches!(args.subcommand, Subcommand::Vault(_)));
    }
}

#[test]
fn test_cli_args_parses_vault_unlock_recovery_flag() {
    let args = CliArgs::try_parse_from(["shikomi", "vault", "unlock", "--recovery"]).unwrap();
    match args.subcommand {
        Subcommand::Vault(VaultSubcommand::Unlock(a)) => assert!(a.recovery),
        other => panic!("expected Vault(Unlock(_)), got {other:?}"),
    }
}

#[test]
fn test_cli_args_parses_vault_rekey_with_print_output() {
    let args = CliArgs::try_parse_from(["shikomi", "vault", "rekey", "--output", "print"]).unwrap();
    match args.subcommand {
        Subcommand::Vault(VaultSubcommand::Rekey(a)) => {
            assert_eq!(a.output, OutputTarget::Print);
        }
        other => panic!("expected Vault(Rekey(_)), got {other:?}"),
    }
}

#[test]
fn test_cli_args_rejects_password_flag_on_vault_unlock() {
    // C-38 / 服部指摘: パスワードを CLI 引数として受け付けない契約。
    // `--password` は **clap 派生型に定義しない**ため不明引数として拒否される。
    let result = CliArgs::try_parse_from(["shikomi", "vault", "unlock", "--password", "x"]);
    assert!(result.is_err());
}

#[test]
fn test_cli_args_edit_kind_flag_is_unknown_arg() {
    // requirements.md REQ-CLI-003 スコープ外のため `edit --kind` は clap のエラーになる
    let result = CliArgs::try_parse_from([
        "shikomi",
        "edit",
        "--id",
        "01234567-0123-7000-8000-0123456789ab",
        "--kind",
        "text",
    ]);
    assert!(result.is_err());
}

// ---------------------------------------------------------------
// Issue #76 (#74-B): Sub-F ユニットテスト 13 件 工程3 実装
// 設計根拠: docs/features/vault-encryption/test-design/sub-f-cli-subcommands/
//          {index.md §15.5, issue-76-verification.md §15.17.1}
// ---------------------------------------------------------------

/// TC-F-U01 (REQ-S15): `VaultSubcommand` の **7 variant** が clap 派生型として
/// 構築可能であり、`vault --help` 出力に 7 サブコマンド全てが列挙され、廃止された
/// `recovery-show` が**含まれない**こと。
///
/// 検証手段: `clap::CommandFactory::command()` 経由で `vault` サブコマンド木を
/// 取り出し、子 subcommand 名集合を抽出して期待集合と比較する。`cargo run` を
/// 起動せず compile-time に決定する pure 検証で flaky を防ぐ。
///
/// 配置先: `crates/shikomi-cli/src/cli.rs::tests` (issue-76-verification.md §15.17.1
/// 推奨配置と一致)。
#[test]
fn tc_f_u01_vault_subcommand_help_lists_seven_variants_recovery_show_absent() {
    use clap::CommandFactory;

    let cmd = CliArgs::command();
    let vault = cmd
        .find_subcommand("vault")
        .expect("vault subcommand must exist");
    let names: std::collections::BTreeSet<String> = vault
        .get_subcommands()
        .map(|s| s.get_name().to_owned())
        .collect();

    let expected: std::collections::BTreeSet<String> = [
        "encrypt",
        "decrypt",
        "unlock",
        "lock",
        "change-password",
        "rekey",
        "rotate-recovery",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();

    assert_eq!(
        names, expected,
        "vault subcommand set must be exactly the 7 variants (recovery-show 廃止), got {names:?}"
    );
    assert!(
        !names.contains("recovery-show"),
        "recovery-show は廃止済 (Rev1 ペガサス致命指摘①解消)"
    );
}

/// TC-F-U11 (C-37 / EC-F9): clap 派生型に `--no-mode-banner` / `--hide-banner` が
/// **定義されていない**こと、かつ `presenter::mode_banner::display` の呼出経路が
/// `usecase::list::summaries_to_views` と `presenter::list::render_list` を介して
/// `ProtectionModeBanner` を必須引数として要求することの型レベル機械検証。
///
/// 設計書 §15.5 #11: 隠蔽不能補強。`--no-mode-banner` を渡すと clap が `unknown
/// flag` で reject + grep gate (TC-F-S02) が補完するが、本 unit test は clap parse
/// 経路のみ検証する。
///
/// 配置先: `crates/shikomi-cli/src/cli.rs::tests` (issue-76-verification.md §15.17.1
/// 「`cli.rs::tests` + grep gate」推奨配置の cli 部分)。
#[test]
fn tc_f_u11_vault_list_rejects_no_mode_banner_flag_and_render_list_requires_protection_mode() {
    // (a) clap 派生型に `--no-mode-banner` は定義されていない → unknown arg として reject。
    let result = CliArgs::try_parse_from(["shikomi", "list", "--no-mode-banner"]);
    assert!(
        result.is_err(),
        "--no-mode-banner は未定義であるべき (隠蔽フラグ非導入、C-37 構造防衛)"
    );

    // (b) `--hide-banner` も同様に未定義。
    let result2 = CliArgs::try_parse_from(["shikomi", "list", "--hide-banner"]);
    assert!(
        result2.is_err(),
        "--hide-banner は未定義であるべき (C-37 構造防衛)"
    );

    // (c) `presenter::list::render_list` シグネチャは `ProtectionModeBanner` を必須引数
    // として持つ (Option/Default 不可)。コンパイル時に関数ポインタ経由で型一致を強制し、
    // 「protection_mode を渡さない」コードパスをドリフトできない構造に閉じ込める。
    use crate::presenter::Locale;
    use crate::view::RecordView;
    use shikomi_core::ipc::ProtectionModeBanner;
    let _: fn(&[RecordView], ProtectionModeBanner, bool, Locale) -> String =
        crate::presenter::list::render_list;
}

// --- TC-UT-150~152: --no-ipc parse / TC-IT-126 ---

/// TC-UT-150 (REQ-DDM-001 / AC-DDM-01): `--no-ipc` フラグが `args.no_ipc == true` にパースされる。
#[test]
fn tc_ut_150_no_ipc_flag_parses_to_true() {
    let args = CliArgs::try_parse_from(["shikomi", "--no-ipc", "list"]).unwrap();
    assert!(args.no_ipc, "--no-ipc should set no_ipc=true");
}

/// TC-UT-151 (REQ-DDM-001 / AC-DDM-05): フラグなしで `args.no_ipc == false`（IPC 既定）。
#[test]
fn tc_ut_151_no_flag_defaults_no_ipc_to_false() {
    let args = CliArgs::try_parse_from(["shikomi", "list"]).unwrap();
    assert!(
        !args.no_ipc,
        "default no_ipc should be false (IPC is default)"
    );
}

/// TC-UT-152 (REQ-DDM-001 / AC-DDM-04): 廃止された `--ipc` フラグを渡すと clap エラー。
#[test]
fn tc_ut_152_ipc_flag_is_unknown_arg_error() {
    let result = CliArgs::try_parse_from(["shikomi", "--ipc", "list"]);
    assert!(
        result.is_err(),
        "--ipc should be rejected as unknown argument"
    );
    let err = result.unwrap_err();
    let rendered = err.to_string();
    assert!(
        rendered.contains("--ipc") || rendered.contains("ipc"),
        "error message should reference '--ipc', got: {rendered}"
    );
}
