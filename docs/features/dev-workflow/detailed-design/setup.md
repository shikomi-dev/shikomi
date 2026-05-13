# 詳細設計書 — dev-workflow / setup スクリプトとバイナリ配布契約

<!-- 基本設計書とは別ファイル。統合禁止 -->
<!-- 配置先: docs/features/dev-workflow/detailed-design/setup.md -->
<!-- 兄弟: ./index.md, ./classes.md, ./messages.md, ./data-structures.md, ./scripts.md -->

## lefthook / gitleaks の配布経路と SHA256 検証（REQ-DW-015 詳細）

### バイナリ取得 URL のテンプレート

setup スクリプト冒頭に以下の定数を置き、アップデート時は PR で明示差分を提示する:

| 定数名 | 例 | 用途 |
|-------|-----|------|
| `LEFTHOOK_VERSION` | `1.7.18` | ピンバージョン。**設計時の契約**は「定数として設置し空なら Fail Fast」。具体値は Sub-issue C の初期実装で upstream の `checksums.txt` から転記（設計判断の先送りではなく、upstream の将来リリース版を引くための運用上の転記） |
| `LEFTHOOK_SHA256_LINUX_X86_64` | `<64 hex chars>` | `lefthook_${VERSION}_Linux_x86_64.tar.gz` の SHA256 |
| `LEFTHOOK_SHA256_LINUX_ARM64` | `<64 hex chars>` | 同 aarch64 |
| `LEFTHOOK_SHA256_MACOS_X86_64` | `<64 hex chars>` | Intel Mac |
| `LEFTHOOK_SHA256_MACOS_ARM64` | `<64 hex chars>` | Apple Silicon |
| `LEFTHOOK_SHA256_WINDOWS_X86_64` | `<64 hex chars>` | `lefthook_${VERSION}_Windows_x86_64.zip` の SHA256 |
| `GITLEAKS_VERSION` / `GITLEAKS_SHA256_*` | — | 同様のピン |

**初期値は Sub-issue C の実装 PR で確定**させる。本設計書では「定数として設置する」契約だけを凍結し、具体値は実装時の `curl -sL https://github.com/.../releases/download/v${V}/checksums.txt` を取得して転記する運用とする（upstream の公式 SHA256 を再計算せず、公式リリース成果物のチェックサムを信頼する）。

### ダウンロード → 検証 → 配置の手順

1. URL 合成: `https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/lefthook_${LEFTHOOK_VERSION}_${PLATFORM}.${EXT}`
   - `PLATFORM`: OS と arch から決定（例: `Linux_x86_64`, `Darwin_arm64`, `Windows_x86_64`）
   - `EXT`: Unix は `tar.gz`、Windows は `zip`
2. `curl -sSfL <URL> -o <tmpfile>` でダウンロード（`-f` で HTTP エラー時に Fail Fast）
3. 実測 SHA256 を取得:
   - Unix: `sha256sum <tmpfile>` の先頭 64 hex を抽出
   - Windows: `(Get-FileHash <tmpfile> -Algorithm SHA256).Hash.ToLower()`
4. ピン定数と**完全一致**（大小文字・空白含め）を検証。不一致なら:
   - 一時ファイルを削除
   - MSG-DW-012 を stderr に出して exit 非 0
5. 一致なら展開し、バイナリを `~/.cargo/bin/`（Windows は `$env:USERPROFILE\.cargo\bin\`）に移動
6. Unix のみ `chmod +x <binary>` を適用

`gitleaks` も同一手順（バージョンと SHA256 定数を別に持つ）。

### なぜ `~/.cargo/bin/` に置くか

- 既に `just` / `convco` が `cargo install` で同ディレクトリに入る。PATH 設定の追加案内が不要（DRY）
- shikomi の開発者は全員 Rust toolchain 済み → `~/.cargo/bin/` は PATH に含まれている前提
- `/usr/local/bin` に入れる案は管理者権限要求で Fail Fast 契約を損なう

### CODEOWNERS で保護する 5 パス（REQ-DW-016 詳細）

`.github/CODEOWNERS` に Sub-issue B で以下を追記:

| パス | 保護対象の理由（T5・T8 脅威対応） |
|-----|----------------------------------|
| `/lefthook.yml` | フック定義の改変で検知スキップ・任意コマンド実行を仕込める |
| `/justfile` | レシピ内のコマンド改変で CI とローカルの乖離を作れる |
| `/scripts/setup.sh` | ダウンロード URL / SHA256 ピン改変でサプライチェーン攻撃経路を作れる |
| `/scripts/setup.ps1` | 同上 |
| `/scripts/ci/` | secret 検知契約（TC-CI-012〜015）の改変で水際検知を無効化できる |

追記順は既存 CODEOWNERS の「ルート直下のガバナンスファイル」節の直後に配置（可読性優先）。
