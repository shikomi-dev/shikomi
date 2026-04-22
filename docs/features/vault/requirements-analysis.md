# 要求分析書

<!-- feature: vault / Issue #7 -->

## 人間の要求

> 開発をスタートしていい。Issueの発行から。システム上の制約で１PRごとしか内部レビュー、外部レビューをしないこと（複数まとめて作業しない）
> （Issue #4 workspace 初期化完了を受けて）次のIssueに進む。

依頼主（まこちゃん）から、Issue #4 で整った Cargo workspace 上に shikomi の実装を積み上げる流れでの着手指示。Clean Architecture の最下層から積む方針に従い、本 Issue では **`shikomi-core` crate に vault ドメイン型を定義**する。

## 背景・目的

- `docs/architecture/` で確定した vault 保護モード（平文 / 暗号化オプトイン）・鍵階層（VEK + KEK）・自動クリアなどの**方針**を、**型（Rust のデータ構造）で表現**する工程が未着手
- 後続 Issue（`shikomi-infra` SQLite 永続化、暗号化実装、IPC プロトコル）はいずれも **vault ドメイン型に依存**する。先に型を確定しないと、各 crate がそれぞれドメイン概念を再発明し、責務散在（例: `protection_mode` を各所で文字列チェックする）を招く
- 型を厳密にすることで **Fail Fast**（不正状態をコンパイル時/構築時に拒否）と **Tell, Don't Ask**（集約にメソッドを生やして問合せ分岐を減らす）を実装以前に強制できる
- ビジネス価値: 「パスワードを扱うデスクトップ OSS」として、ドメイン型層で秘密情報のリーク経路を封じる（`String` / `Vec<u8>` を直接回さない、`Debug` で中身を出さない）ことは、**後段の実装で発覚する類の欠陥を根絶する**唯一の手段

## 議論結果

Issue #4 完了後の工程1で以下を合意:

- 設計担当（セル）と涅マユリの見解: Clean Architecture の依存方向順に下から積む（`shikomi-core` → `shikomi-infra` → 実装 crate）
- 防衛線（ペテルギウス）の推薦: **「`shikomi-core` crate 骨格 + vault ドメイン型定義」を最初に**
- キャプテン決定: vault feature の新規機能として、Sub-issue 分割なしで進める
- スコープの絞り込み: 暗号化「計算」（Argon2id / AES-GCM 実行）と永続化（SQLite）は本 Issue では**型のみ**扱い、計算・I/O は `shikomi-infra` Issue に後送

## ペルソナ

本 Issue は `shikomi-core` crate の公開 API を確定するため、**エンドユーザーに直接触れない**。ただし DX の影響を受ける代表ユーザーをペルソナとして置く。エンドユーザーペルソナ（田中/山田/佐々木）は `docs/architecture/context/overview.md` §3.2 を参照。

| ペルソナ名 | 役割 | 技術レベル | 利用文脈 | 達成したいゴール |
|-----------|------|-----------|---------|----------------|
| 野木 拓海 | 後続 Issue の Rust 実装担当 | Rust 中級（`serde` / `tokio` / `rusqlite` 実務経験） | `shikomi-infra` で vault をファイルに書き出す実装を書く | `shikomi-core` の公開 API を見て「どの型を受け取り、どの型を返せばいいか」が曖昧さなく決められる |
| 志摩 凛 | OSS コントリビュータ | Rust 初級（学習中） | GitHub で Issue を拾い、`shikomi-core` の単体バグ修正を試みる | テストを書き足す際、型の不変条件が `shikomi-core::*` の public API と doc で把握できる |

## 前提条件・制約

- **Issue #4 完了済み**: Cargo workspace と 5 crate スケルトン、`deny.toml`（反転方式）、`[workspace.dependencies]` 一元化方針が整備済み
- **pure Rust / no-I/O**: 本 crate は `std::fs` / `std::net` / OS API を呼ばない。I/O を要する操作は trait シグネチャまで（実装は `shikomi-infra`）
- **セキュリティクリティカル crate 扱い**: 本 Issue で追加する `secrecy` / `zeroize` は `docs/architecture/tech-stack.md` §4.3.2 に列挙済み。`[advisories].ignore` 禁止リストに含まれる
- **`Cargo.toml` 変更は `[workspace.dependencies]` 経由**: 各 crate 側の `Cargo.toml` は `{ workspace = true }` のみ書く（§4.4）

## 機能一覧

| 機能ID | 機能名 | 概要 | 優先度 |
|--------|-------|------|--------|
| REQ-001 | ProtectionMode | vault の保護モード（`Plaintext` / `Encrypted`）を排他 2 値で表現 | 必須 |
| REQ-002 | VaultVersion | vault フォーマットバージョン（将来互換性管理） | 必須 |
| REQ-003 | VaultHeader | ヘッダ集約。モードごとに有効フィールドを型で排他（`Option<T>` を乱用しない） | 必須 |
| REQ-004 | RecordId | UUIDv7 ベースのレコード識別子 newtype | 必須 |
| REQ-005 | Record / RecordKind / RecordLabel | レコード本体・種別ヒント・ラベル型（構築時検証付き） | 必須 |
| REQ-006 | RecordPayload | 平文 / 暗号化 enum でレコード本文を排他表現 | 必須 |
| REQ-007 | Vault（集約ルート） | `VaultHeader` + `Vec<Record>` の一貫性不変条件を保持する集約 | 必須 |
| REQ-008 | SecretString / SecretBytes | `secrecy::SecretBox` ベースの秘密値ラッパ型（`Debug` 実装でリークしない） | 必須 |
| REQ-009 | DomainError | `thiserror` ベースの列挙型エラー（不変条件違反・境界値超過・nonce overflow 等） | 必須 |
| REQ-010 | NonceCounter | VEK 当たり $2^{32}$ 暗号化上限を Fail Fast で検知するカウンタ型 | 必須 |

## Sub-issue分割計画

該当なし — 理由: 単一Issueで完結。vault ドメイン型は相互に依存する小ユニットであり、バラバラの PR に割ると中途半端な状態の `shikomi-core` が develop にマージされる危険がある。

| Sub-issue名 | スコープ | 依存関係 |
|------------|---------|---------|
| — | — | — |

## 非機能要求

| 区分 | 指標 | 目標 |
|-----|------|------|
| バイナリサイズ | `shikomi-core` 単独のコンパイル後バイナリサイズ増加 | 100 KB 以下（LTO + dead-code elimination 前提） |
| ビルド時間 | `cargo build -p shikomi-core` 初回 | 30 秒以下（6 crate 依存のみ） |
| ユニットテスト実行時間 | `cargo test -p shikomi-core` | 1 秒以下（pure Rust、I/O 無し） |
| 秘密値リーク耐性 | `Debug` / `Display` / `Serialize` 全面で `SecretString` / `SecretBytes` 中身を出さない | 100%（doc テストで検証） |
| nonce 衝突耐性 | 同一 VEK での暗号化回数 | $2^{32}$ 到達で即 `DomainError::NonceOverflow`（NIST SP 800-38D §8.3 準拠） |

## 受入基準

| # | 基準 | 検証方法 |
|---|------|---------|
| 1 | 全機能 REQ-001〜REQ-010 の型が `shikomi-core` の公開 API に存在する | `cargo doc -p shikomi-core --no-deps` の出力確認 |
| 2 | 排他条件を型で保証（`Plaintext` に wrapped_VEK が生えない 等） | 詳細設計の `VaultHeader` enum 定義 + ユニットテスト |
| 3 | `RecordLabel` が空文字列・制御文字・255 文字超過を構築時に拒否 | 境界値テスト（空 / 1 文字 / 254 / 255 / 256 / 制御文字） |
| 4 | `SecretString` / `SecretBytes` が `Debug` / `Display` で中身を露出しない | doc テスト + コンパイル時型検査（`Serialize` 未実装） |
| 5 | `NonceCounter::next()` が $2^{32}$ 到達で `DomainError::NonceOverflow` を返す | ユニットテスト（上限直前・到達・超過） |
| 6 | `cargo test -p shikomi-core` が pass、行カバレッジ 80% 以上 | CI（`cargo llvm-cov` 等）で測定 |
| 7 | `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `cargo deny check` pass | CI |
| 8 | 公開 API 上に生 `String` / `Vec<u8>` を露出させない（シークレット経路のみ） | `cargo doc` の型シグネチャ目視 + grep 検査 |
| 9 | `deny.toml` §4.3.2 の暗号クリティカル crate 扱いに `secrecy` / `zeroize` が確実に登録されていること | `deny.toml` 差分確認 |

## 扱うデータと機密レベル

**本 Issue で扱うのは型定義そのもの**であり、実データは流れない。ただし、型は以下の機密レベルのデータを**今後流す経路**となる:

| データ | 機密レベル | 本 Issue での扱い |
|-------|----------|-----------------|
| マスターパスワード | 最高（失うと vault 全滅） | `SecretString` 型として定義。`Debug`/`Serialize` 非露出 |
| BIP-39 リカバリコード 24 語 | 最高（マスターパスワード同等） | `SecretString` 型として定義 |
| VEK（Vault Encryption Key）32 バイト | 最高 | `SecretBytes` 型として定義 |
| KEK_pw / KEK_recovery | 最高（使用直後 zeroize） | `SecretBytes` 型として定義、`zeroize` on drop |
| wrapped_VEK（ヘッダ保管） | 中（暗号化済み、鍵無しでは無価値） | `Vec<u8>` として公開（暗号文なのでラップ不要） |
| レコード平文（平文モード時） | 高（ユーザが選択） | `SecretString` として内包。モード切替時のみ展開可 |
| レコードラベル（すべてのモード共通） | 低〜中 | `RecordLabel`（plain `String` wrapper、検証付き） |
| UUIDv7 / created_at / updated_at | 低 | 通常の型 |

設計担当・実装担当は本 Issue 以降、型を跨いで上記のデータが扱われる**すべての経路**で `SecretString` / `SecretBytes` 以外の型に剥ぎ落とさないこと。
