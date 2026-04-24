# System Context — Threat Model & OWASP（shikomi）

> **本書の位置づけ**: `docs/architecture/context/` 配下の**脅威モデル / OWASP 編**。システム概要・ペルソナは `overview.md`、プロセスモデル / IPC / vault 保護モードは `process-model.md`、課題 / スコープ / 非機能要件は `nfr.md` を参照。

## 7. 脅威モデル（STRIDE ベース）

**前提**: 下表は **vault 保護モードごと**に脅威と対策を分けて扱う。平文モード（デフォルト）と暗号化モード（オプトイン）では **T / I の攻撃面が大きく異なる**ため、利用者がモード選択時にトレードオフを正しく理解できるよう明示する。

| 脅威カテゴリ | 具体 | 平文モード（デフォルト）の対策 | 暗号化モード（オプトイン）の対策 |
|------------|-----|--------------------------|--------------------------|
| **S**poofing | 他プロセスが shikomi を名乗りホットキーを横取り／同ユーザ内の別プロセスが IPC ソケットへ偽装接続 | OS 署名（Developer ID / EV 証明書）、Wayland は Portal 同意ダイアログ、IPC は `process-model.md` §4.2 の UDS `0700` + ピア UID 検証（Issue #26 で実装）。**セッショントークンによる二重防御（同ユーザ内悪性プロセス対策）は後続 Issue で追加予定**——`IpcProtocolVersion::V2` 追加と同時に `Handshake { client_version, session_token }` へ非破壊拡張する | 同左（共通） |
| **T**ampering | vault ファイルの改竄（同ユーザ他プロセスによる書換え・差替え）・途中書込でのファイル破損 | OS パーミッション + atomic write（§7.1）。**同ユーザ内の他プロセスによる改竄は OS では阻止できず、平文モードでは暗号学的な改竄検出も行えない**（§7.0 参照）。自プロセスの部分書込のみ atomic write で防御 | AEAD（AES-256-GCM）認証タグ検証で他プロセスによる改竄を暗号学的に検出、改竄時は `fail fast` |
| **R**epudiation | 対象外（単独ローカルアプリ） | 該当なし — 理由: 外部へ操作ログを送出しない | 同左 |
| **I**nformation Disclosure | 平文パスワードのクリップボード流出、履歴保持、Cloud Clipboard 同期、スワップ経由ディスク書出、IPC 通信盗聴、**vault ファイル直接読取** | **vault は平文のため、OS パーミッション突破と同時に全レコードが漏洩**（脅威が現実化する具体例は §7.0）。クリップボード系対策は共通（自動クリア、sensitive hint）。IPC は UDS `0700` 適用 | vault は AES-256-GCM で保護され、OS パーミッション突破だけでは平文を得られない。加えて `secrecy` + `zeroize` + best-effort `mlock`/`VirtualLock` でメモリ上も最小時間保護 |
| **D**enial of Service | ホットキー登録衝突、daemon 多重起動、vault 破損 | 起動時検出 → ユーザに再割当を促す（fail fast）、**IPC エンドポイント先取り（UDS: `flock` + unlink-before-bind の race-safe 併用、Windows: Named Pipe `FILE_FLAG_FIRST_PIPE_INSTANCE`）による単一インスタンス保証**（`process-model.md` §4.1 ルール2、Issue #26 で実装）、atomic write による部分書込防止 | 同左 |
| **E**levation of Privilege | 管理者権限を要求しない設計 | 通常ユーザ権限で動作、setuid 等は使用しない | 同左 |

### 7.0 平文モードの残存リスク（ユーザ自己責任として明記）

**平文モードはデフォルトだが、以下のリスクをユーザが受容していることが前提**。アプリは初回起動時のウェルカム画面・`shikomi vault --help`・`SECURITY.md` で**平文であることとリスク**を明示する（「インストール時に何も言われなかったから安全」という誤認を防ぐ責務）。

| リスク | 具体例 | 推奨対処 |
|-------|-------|---------|
| **同ユーザ内の他プロセス**が vault ファイルを**読取** | OS パーミッションは同ユーザ別プロセスには無力。マルウェア・誤って実行した怪しいスクリプトが `~/.config/shikomi/vault.db` を直接読む | 暗号化モードに切替（`shikomi vault encrypt`）、または不審プロセスを実行しない運用 |
| **同ユーザ内の他プロセス**が vault ファイルを**書換え／差替え** | 悪意あるスクリプトが平文レコードを改竄（例: 既存レコードの値を攻撃者のサーバに書き換え、ユーザがホットキーで投入＝認証情報漏洩）、または vault ファイル自体を差し替えて偽レコードを注入。**atomic write は自プロセスの部分書込を防ぐのみで、外部プロセスの改竄は阻止しない**。平文モードには AEAD 認証タグがないため改竄を**暗号学的に検出できない** | 暗号化モードに切替（AES-256-GCM の GMAC タグで改竄検出）、ファイル整合性監視ツール（`tripwire` / `aide` 等）併用、不審プロセスを実行しない運用 |
| **別ユーザ・root**が vault ファイルを読取／書換え | 共有端末、侵害されたマシン | 共有端末では使わない、暗号化モード必須 |
| **ディスク暗号化なし**での物理盗難 | ノートパソコン紛失、廃棄時のディスク引き抜き | OS 標準のディスク暗号化（BitLocker / FileVault / LUKS）を有効化、かつ shikomi 暗号化モードも併用 |
| **バックアップ媒体への漏洩** | クラウド同期ツール（Dropbox / iCloud 等）が `~/.config/shikomi/` を同期、バックアップ先で復号可能 | バックアップ対象から vault ディレクトリを除外、または暗号化モードで export のみ外部へ |

**UI/UX 要件**:
- 初回起動ウェルカム画面で「現在は平文モードです。機密情報（パスワード等）を保存する場合は暗号化モードを推奨します」をデフォルトで表示（スキップ可能だが、隠さない）
- `shikomi list` の出力ヘッダに **`[plaintext]`** / **`[encrypted]`** を表示、現在の保護モードを常に可視化（Fail Visible）
- README の "Security" セクション冒頭に **「default is plaintext; opt-in encryption for sensitive data」**を明記

### 7.1 vault の atomic write

- 書込先は `vault.db`（SQLite）本体ではなく `vault.db.new`（同一ディレクトリ、パーミッション `0600`）に書き、`fsync(2)` / `FlushFileBuffers` → `rename(2)` / `ReplaceFileW` で差替
- `rename` は POSIX の atomicity を利用。Windows は `ReplaceFileW` で旧ファイルを保持したままメタデータ差替（同一ボリューム内限定）
- 失敗時は `.new` を削除（部分書込を残さない）。起動時に `.new` の残存を検出したら破損扱い → リカバリ UI へ誘導
- SQLite 利用時は `PRAGMA journal_mode=WAL` ではなく `DELETE` を採用（WAL はチェックポイント不整合を生みやすく、export バックアップが取りにくい）
- **両モード共通**: atomic write はモードに依存しない。平文モードでも部分書込はユーザデータ破損に直結するため必須

### 7.2 クリップボード自動クリア既定 30 秒の根拠

- **1Password**: 90 秒（`support.1password.com/copy-passwords/`）
- **KeePassXC**: 既定 10 秒（変更可）
- **Bitwarden**: 既定 "Never"（セキュリティ観点で問題あり、ユーザ設定前提）

shikomi は「ホットキーで投入 → ユーザが即貼付」が主ユースケース。10 秒は慌ただしい業務で短すぎ、90 秒は残存リスクが大きい。間を取って **30 秒** を既定とし、ユーザ設定で `5〜300` 秒の範囲を許可する。**クリア時は「書き込み時点の値と一致する場合のみ消す」ロジック**（KeePassXC 方式）で上書き誤消去を回避する。

#### 7.2.1 クリア時のユーザーフィードバック方針（アーキテクチャ決定）

**決定**: **カウントダウン表示 + クリア完了通知の二段構え**。単純な「消えて終わり」は UX 盲点（ペルソナ田中が「なぜ突然 Ctrl+V できなくなったのか」を理解できない）。ただし**通知の内容に秘密は含めない**。

| 段階 | タイミング | 表示 | 技術 |
|-----|---------|------|------|
| 投入直後 | ホットキー押下 → クリップボード書込完了時 | トレイアイコンを「シークレット投入中」色（例: オレンジ）に変更、ツールチップに残秒数を表示 | OS トレイ API（Tauri tray icon）。**内容は一切表示しない**、残秒数のみ |
| クリア中 | カウントダウン進行中 | トレイアイコンのツールチップを毎秒更新、最後の 5 秒は点滅 | 同上 |
| クリア完了 | クリアタイマー満了時 | OS 標準の非ブロッキング通知（macOS `UNUserNotificationCenter` / Windows `ToastNotification` / Linux `org.freedesktop.Notifications`）で「クリップボードをクリアしました」とのみ表示 | Tauri `tauri-plugin-notification`。**内容・入力元レコード名は含めない**（Alt+Tab 先の画面共有で漏洩する事故防止） |
| 上書き検出時 | 他アプリが同時期にクリップボードを書換 → shikomi が上書きを検出 | 通知せず静かに監視停止（誤消去防止） | — |

**ユーザ設定**:
- 全通知の**完全オフ**も許可（`shikomi settings --notifications=off`）。画面共有の多い利用者向け
- トレイアイコン非表示モード（完全な「見えない常駐」）も許可

**NG パターン（設計上拒否）**:
- モーダルダイアログでの通知（UX 破壊、Alt+Tab 前提の業務で操作を妨げる）
- 通知メッセージにレコード名やプレビュー文字を含める（画面共有・肩越し閲覧で情報漏洩）
- サウンド通知（デフォルト off、オプトイン）

## 8. OWASP Top 10 対応表（2021 版・デスクトップアプリ適用）

OWASP Top 10 はもともと Web アプリ向けだが、サーバを持たないデスクトップアプリでも**多くが該当する**（認可・ログ・暗号失敗・依存コンポーネント等）。設計上の取扱を以下に明示する。

| カテゴリ | 該当性 | shikomi での扱い |
|---------|-------|-----------------|
| **A01: Broken Access Control** | 該当（ローカル多重プロセス・多重ユーザ） | IPC の UID 検証＋ソケット `0700`（`process-model.md` §4.2）。vault ファイルパーミッション `0600`、ディレクトリ `0700`。Windows は同等の ACL を SDDL で設定 |
| **A02: Cryptographic Failures** | **暗号化モード（オプトイン）で該当** | 暗号化モード有効時のみ AEAD（AES-256-GCM）＋ Argon2id（OWASP 推奨 `m=19456, t=2, p=1`）を適用。nonce は CSPRNG から毎回 96bit 生成、vault 内に per-record 記録（`../tech-stack.md` §2.4 参照）。VEK は `secrecy` + `zeroize`。MAC（GMAC タグ）で改竄検知。**平文モードは暗号保護を行わないことを明示**（§7.0 のユーザ自己責任リスク表を参照）。デフォルト平文を選ぶ場合、A02 は「暗号を使わない設計判断」として該当外だが、代わりに §7.0 の脅威表を受容する |
| **A03: Injection** | 該当（SQL / コマンド引数） | SQLite 操作は `rusqlite` の parameter binding のみ使用し生 SQL 連結禁止。CLI → daemon IPC は MessagePack 型付きスキーマ、文字列として shell に渡す経路なし |
| **A04: Insecure Design** | 該当 | 本ドキュメント全体で扱う（プロセスモデル・Threat Model・Fail Secure 方針）。**デフォルト平文を Insecure Design と誤解されないよう**、§7.0 でリスク提示と UI 可視化（`[plaintext]` 表示）を強制する設計とした。「知らされず平文だった」事故を防ぐ |
| **A05: Security Misconfiguration** | 該当 | 既定値を安全側に（自動クリア 30 秒、アイドルタイムアウト 15 分、テレメトリ off、キーチェーン連携 off）。**vault 保護モードはデフォルト平文**だが、それを**必ず可視化する**ことで「設定ミスでオフのまま」を回避。デバッグビルドは別バイナリでリリースチャネルに混入しない |
| **A06: Vulnerable and Outdated Components** | 該当 | `cargo-deny` + `cargo-audit` + Dependabot（`../dev.md` §5）。`Cargo.lock` をコミットし lock 書換え監査。SBOM（CycloneDX）をリリースに添付 |
| **A07: Identification and Authentication Failures** | **暗号化モードで該当** | 暗号化モード時のみマスターパスワード認証を行うため、該当は暗号化モードに限定。Argon2id で総当たり耐性、連続失敗 5 回で **非同期タイマー（`tokio::time::sleep`）による指数バックオフを該当 IPC リクエストにのみ適用**（プロセス全体を blocking sleep させない＝ホットキー購読を継続、`../tech-stack.md` §2.4 参照）。IPC 認証（UID 検証は Issue #26 で実装、**セッショントークンは後続 Issue で追加予定**）は両モード共通。リカバリコード：BIP-39 24 語、1 度だけ表示、再発行不可（暗号化モード時のみ） |
| **A08: Software and Data Integrity Failures** | 該当 | コード署名（Win: Authenticode、Mac: Developer ID + Notarization、Linux: GPG + minisign）。更新時は `tauri-plugin-updater` の minisign 署名検証、検証失敗で更新中断 |
| **A09: Security Logging and Monitoring Failures** | 該当（ただしテレメトリ送信なし方針） | ローカルログのみ、`tracing` で構造化。シークレットは `secrecy` の `Debug` マスクで自動秘匿。ログファイルはローテート（サイズ・日数）、`0600` 権限。プライバシ懸念のため操作ログはユーザが明示的に opt-in した場合のみ詳細化 |
| **A10: Server-Side Request Forgery** | 該当なし — サーバサイドリクエストを行わない | 該当なし。更新チェック（`tauri-plugin-updater`）のみ固定ホストへアクセス。任意 URL への HTTP 発行 API は提供しない |

### 8.1 残存リスク（受容する）

- **OS が侵害された場合**: プロセスメモリ読取・kernel keylogger・LD_PRELOAD 等にはアプリ側で防御不能。README / SECURITY.md に明記
- **サスペンド／ハイバネーション**: `mlock(2)` man-page 記載の通り、RAM 全体がスワップへ書き出される。メモリロックは best-effort
- **macOS Secure Event Input**: パスワード入力欄がフォーカスされている間、貼付後のキー入力も含め他プロセスからの注入はブロックされる。機能仕様として明示（仕様不具合ではない）

### 8.2 参考一次情報

- OWASP Secrets Management Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html
- OWASP Password Storage Cheat Sheet（Argon2id 推奨パラメータ）: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
- KeePassXC Clipboard 実装（sensitive hint の OS 別 MIME）: https://github.com/keepassxreboot/keepassxc/blob/develop/src/gui/Clipboard.cpp
- KDE `x-kde-passwordManagerHint` 由来: https://phabricator.kde.org/D12539
- 1Password 90 秒自動クリア既定: https://support.1password.com/copy-passwords/
- Wayland セキュリティモデル: https://wayland.freedesktop.org/architecture.html
- `mlock(2)` とサスペンド制約: https://man7.org/linux/man-pages/man2/mlock.2.html
- Apple Technote TN2150（Secure Event Input）: https://developer.apple.com/library/archive/technotes/tn2150/_index.html
