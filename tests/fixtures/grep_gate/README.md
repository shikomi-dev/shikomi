# grep gate 回帰防止 fixture (TC-CI-026 サブケース a〜e)

`scripts/ci/lib/audit_unsafe_blocks.sh` の `audit_unsafe_blocks` 関数が `unsafe` ブロック検出契約（TC-CI-019 / TC-CI-026 共通）を満たすことを検証するための最小再現 fixture。

## 設計根拠

- `docs/features/dev-workflow/detailed-design/scripts.md` §`audit-secret-paths.sh` の `unsafe` ブロック検出契約
- `docs/features/dev-workflow/test-design.md` §TC-CI-026 サブケース

## サブケース構成

| サブケース | fixture path | 期待結果 | 検証目的 |
|----------|--------------|---------|---------|
| **a** | `case_a/realy_unsafe.rs` | FAIL（exit 1、stderr に file:line:content） | コメント行除外パイプを挟んでも実 `unsafe` ブロックは正しくヒット |
| **b** | `case_b/doc_comment_only.rs` | PASS（exit 0） | doc コメント `/// unsafe { ... }` を grep が誤検出しない（PR #82 実害再現） |
| **c** | `case_c/comment_variants.rs` | PASS（exit 0） | `//` / `///` / `//!` / 先頭空白 / 空白なしの 5 パターン全てを除外 |
| **d** | `case_d/inline_comment.rs` | FAIL（exit 1） | 行内コメント `unsafe { ... } // SAFETY: ...` の実コードは検出継続 |
| **e** | `case_e/io/windows_sid.rs` + `case_e/hardening/core_dump.rs` | PASS（exit 0） | 許可リストに登録された 2 ファイルは検査結果から除外 |

## 実行方法

```bash
bash tests/unit/test_audit_secret_paths_grep_gate.sh
```

## Issue 連動

Issue #85 — Audit grep gate がコメント文字列の `unsafe` を誤検出する（PR #82 / #84 で `--admin` 連発を誘発した実害の構造的終止符）。
