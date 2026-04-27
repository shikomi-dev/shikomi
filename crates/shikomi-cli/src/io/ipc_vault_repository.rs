//! `IpcVaultRepository` — daemon 経由の vault 操作を提供する narrow IPC クライアント。
//!
//! ## スコープ（案 D 採用）
//!
//! `VaultRepository` trait は **実装しない**。代わりに `list_summaries` /
//! `add_record` / `edit_record` / `remove_record` の 4 専用メソッドを公開し、
//! `crate::lib::run` が `enum RepositoryHandle { Sqlite, Ipc }` の `match` で
//! 経路ディスパッチする（Composition over Inheritance）。
//!
//! 当初の trait full impl 案 C は以下の致命的問題を抱えていたため棄却済み（PR #29
//! レビュー、Phase 1.5 で構造的撤去）:
//!
//! - `load()` が daemon 由来 summary を `RecordPayload::Plaintext("")` で偽装する
//!   必要があり、ドメイン集約に「嘘の値」を注入していた
//! - `save()` が CLI 側生成 `RecordId` を提示する一方、daemon が独自 ID を再生成して
//!   保存するため、ユーザに表示される ID が daemon vault に存在しない（嘘 ID 出荷）
//! - `exists()` が常に `Ok(true)` を返し trait 契約に対して嘘
//!
//! 案 D ではこれらを **型レベルで封じ込める**:
//!
//! - `IpcVaultRepository` は `VaultRepository` を実装しないので `Box<dyn VaultRepository>`
//!   に注入できない（CI grep `TC-CI-030` + `compile_fail` doctest `TC-UT-119` で二重防衛）
//! - `add_record` 戻り値の `RecordId` は daemon 側 `IpcResponse::Added { id }` を**そのまま**
//!   返す（CLI 側で `Uuid::*` を呼ばない、CI grep `TC-CI-029` 監査）
//!
//! 設計根拠: docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md
//! §案 D（VaultRepository trait 非実装 + RepositoryHandle enum）
//!
//! ```compile_fail
//! // TC-UT-119: `IpcVaultRepository` が `VaultRepository` trait を**実装しない**ことの
//! // 構造契約。本 doctest が `compile_fail` で成立する = 嘘 ID / 嘘 Plaintext を生む案 C
//! // が再発していない、を保証する（CI 監査 `TC-CI-030` の二重防衛）。
//! fn assert_not_vault_repository<T: shikomi_infra::persistence::VaultRepository>() {}
//! assert_not_vault_repository::<shikomi_cli::io::ipc_vault_repository::IpcVaultRepository>();
//! ```

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use shikomi_core::ipc::{
    IpcRequest, IpcResponse, ProtectionModeBanner, RecordSummary, SerializableSecretBytes,
};
use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};
use shikomi_infra::persistence::PersistenceError;
use time::OffsetDateTime;
use tokio::runtime::Runtime;

use super::ipc_client::IpcClient;
use crate::error::CliError;

// -------------------------------------------------------------------
// IpcVaultRepository
// -------------------------------------------------------------------

/// daemon との接続を保持し、`IpcRequest::ListRecords` を発行して `RecordSummary` 列を取得する。
///
/// `VaultRepository` trait は実装しない（縮減後のスコープでは不要）。
pub struct IpcVaultRepository {
    runtime: Runtime,
    client: Mutex<IpcClient>,
    socket_path: PathBuf,
}

impl IpcVaultRepository {
    /// daemon に接続して `IpcVaultRepository` を構築する（同期 wrapper）。
    ///
    /// # Errors
    /// 接続失敗（daemon 未起動）/ ハンドシェイク失敗時に `PersistenceError`。
    pub fn connect(socket_path: &Path) -> Result<Self, PersistenceError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| PersistenceError::IpcIo {
                reason: e.to_string(),
            })?;
        let client = runtime.block_on(IpcClient::connect(socket_path))?;
        Ok(Self {
            runtime,
            client: Mutex::new(client),
            socket_path: socket_path.to_path_buf(),
        })
    }

    /// `--vault-dir <DIR>` 経路を最優先で daemon に接続する（Issue #75 / Bug-F-007 解消、
    /// `cli-subcommands.md` §Bug-F-007 SSoT）。
    ///
    /// `vault_dir` が `Some(<DIR>)` の場合、socket 解決順序の **第 1 候補** として
    /// `<DIR>/shikomi.sock`（Unix）または `\\.\pipe\shikomi-{H}`（Windows、`<H>` は
    /// `windows_pipe_name_from_dir(<DIR>)` の純関数出力）を試行する。失敗時は
    /// `default_socket_path()` の順序（`SHIKOMI_VAULT_DIR` env / `XDG_RUNTIME_DIR` /
    /// `$HOME/.shikomi` / Windows user-SID 由来）に fallback する。
    ///
    /// 意味論契約: `<DIR>` は **vault.db の所在ディレクトリ**を指す（ユーザ認知モデル）。
    /// CLI は `<DIR>/vault.db` を直接 open せず、**同じディレクトリの `shikomi.sock`** のみを
    /// daemon socket として用いる（Phase 2 規定: CLI は IPC 経由のみ、vault.db 直接操作禁止、
    /// `cli-subcommands.md` §Bug-F-007 解消 §`--vault-dir` の意味論 SSoT）。
    ///
    /// ## Bug-F-009 (Option α) 反映 — エラー文言の MSG-S09(b) 強制
    ///
    /// `vault_dir` が `Some(<DIR>)` の場合、ユーザは明示的に「この vault dir の daemon に
    /// 接続したい」と意思表示している。ここで fallback 経路（`XDG_RUNTIME_DIR` / `HOME`）が
    /// 解決失敗 (`CannotResolveVaultDir`) すると古い infra MSG が前面に出てしまい、設計書
    /// SSoT (`cli-subcommands.md` §Bug-F-007 §エラー文言 SSoT) で約束した
    /// **MSG-S09(b) `--vault-dir <DIR>` 案内**が発火しない。
    ///
    /// 本関数は `vault_dir` 指定時の全経路失敗を `PersistenceError::DaemonNotRunning(primary)`
    /// に変換し、`presenter::error::render_daemon_not_running` の MSG-S09(b) hint
    /// （`pass --vault-dir <DIR>` 案内）を必ず発火させる。`primary` パスをそのまま付ける
    /// ことでユーザに「あなたが指定した DIR の socket が見つからなかった」を articulate する。
    ///
    /// # Errors
    /// - `vault_dir.is_some()` かつ全経路失敗: `PersistenceError::DaemonNotRunning(primary)`
    ///   （MSG-S09(b) 経路、Bug-F-009 Option α）
    /// - `vault_dir.is_none()` かつ fallback 解決失敗: `PersistenceError::CannotResolveVaultDir`
    ///   （ユーザが path hint を出していないため、infra error の素の伝播が UX 的に妥当）
    /// - `vault_dir.is_none()` かつ fallback connect 失敗: `PersistenceError::DaemonNotRunning(fallback)`
    pub fn connect_with_vault_dir(vault_dir: Option<&Path>) -> Result<Self, PersistenceError> {
        if let Some(dir) = vault_dir {
            let primary = vault_dir_socket_path(dir);
            // 第 1 候補で接続成功すれば即返却。
            if let Ok(repo) = Self::connect(&primary) {
                return Ok(repo);
            }
            // 第 1 候補が daemon 不在で失敗 → fallback 経路を試す。
            // Bug-F-009 (Option α): fallback 解決自体が失敗 (`XDG_RUNTIME_DIR` / `HOME` 未設定 等)
            // または fallback connect が失敗した場合、`vault_dir` 指定経路としてユーザに
            // MSG-S09(b) `pass --vault-dir <DIR>` 案内を返す責務を持つ。
            // primary パスを `DaemonNotRunning` に詰めることで「あなたが指定した DIR の
            // socket が見つからなかった」を articulate し、render_daemon_not_running 経由で
            // 正しい hint が発火する経路に流す。
            return match Self::default_socket_path() {
                Ok(fallback) => {
                    Self::connect(&fallback).or(Err(PersistenceError::DaemonNotRunning(primary)))
                }
                Err(_) => Err(PersistenceError::DaemonNotRunning(primary)),
            };
        }
        // `vault_dir` 未指定経路: 既存挙動を維持 (path hint なしのため infra error をそのまま伝播)。
        let fallback = Self::default_socket_path()?;
        Self::connect(&fallback)
    }

    /// OS デフォルトのソケットパスを解決する（Issue #75 Bug-F-007 解消の SSoT 順序、
    /// `cli-subcommands.md` §Bug-F-007 解消 §標準解決順序）:
    ///
    /// 1. `--vault-dir <DIR>` 引数（指定時、最優先）→ `<DIR>/shikomi.sock`（Unix）/
    ///    `\\.\pipe\shikomi-{H}`（Windows）。本関数の対象外（`connect_with_vault_dir` で先行処理）
    /// 2. **Linux**: `$XDG_RUNTIME_DIR/shikomi/daemon.sock`、未設定時は `dirs::runtime_dir()`
    /// 3. **macOS**: `dirs::cache_dir()/shikomi/daemon.sock`、未設定時は `$HOME/.shikomi/daemon.sock`
    /// 4. **Windows**: `\\.\pipe\shikomi-daemon-{user-sid}`（`windows_sid::resolve_self_user_sid`）
    ///
    /// **`SHIKOMI_VAULT_DIR` env 経由派生は Phase B 持ち越し**: 設計 SSoT は env を
    /// `<DIR>` 同等扱いと articulate しているが、実装には daemon 側の socket bind パス
    /// 変更（`daemon.sock` → `shikomi.sock` 一括 rename + `<DIR>` 派生）が伴うため、
    /// 既存 daemon e2e テスト群との互換維持を優先し本 PR では未着手とする
    /// （`cli-subcommands.md` §Bug-F-007 §「`shikomi-daemon` 側起動時の socket bind 仕様で
    /// 対称に固定」を別 PR で連携実装、設計書側 articulate 次イテレーションで `XDG_RUNTIME_DIR`
    /// `dirs::cache_dir` 経路と統一）。
    ///
    /// # Errors
    /// 解決元が利用不能な場合 `PersistenceError::CannotResolveVaultDir`。
    pub fn default_socket_path() -> Result<PathBuf, PersistenceError> {
        #[cfg(unix)]
        {
            unix_default_socket_path()
        }
        #[cfg(windows)]
        {
            let sid = crate::io::windows_sid::resolve_self_user_sid()?;
            Ok(PathBuf::from(format!(r"\\.\pipe\shikomi-daemon-{sid}")))
        }
    }

    /// 接続先ソケットパスへの参照を返す（ログ・エラー表示用）。
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// daemon にレコード summary 列 + 保護モードを要求する（`--ipc list` の主経路）。
    ///
    /// `IpcRequest::ListRecords` を 1 往復し、`Records { records, protection_mode }`
    /// variant を `ListSummariesOutcome` として返す。daemon 側の暗号化検出 /
    /// その他エラーは `PersistenceError` に写像する。
    ///
    /// **Sub-F (#44) Phase 3 / C-37**: `protection_mode` は CLI 側 `presenter::list`
    /// に必須引数として渡す責務 (REQ-S16 / 型レベル強制)。本メソッドは構造体で
    /// 両フィールドを返すことで、呼出側が `protection_mode` を捨てる経路を
    /// 構造的に困難化する (Default 値・`Option` を持たせない設計)。
    ///
    /// # Errors
    /// IPC 失敗 / 暗号化 vault 検出 / 不正応答時に `PersistenceError`。
    pub fn list_summaries(&self) -> Result<ListSummariesOutcome, PersistenceError> {
        match self.round_trip(&IpcRequest::ListRecords)? {
            IpcResponse::Records {
                records,
                protection_mode,
            } => Ok(ListSummariesOutcome {
                records,
                protection_mode,
            }),
            IpcResponse::Error(code) => Err(PersistenceError::from(code)),
            _ => Err(unexpected_response("ListRecords")),
        }
    }

    /// daemon に新規レコード追加を依頼する（`--ipc add` の主経路）。
    ///
    /// id は **daemon 側で生成**され `IpcResponse::Added { id }` でそのまま返る
    /// （CLI 側で `Uuid::*` を呼ばない契約、CI grep `TC-CI-029`）。
    /// `value: SecretString` は `SerializableSecretBytes::from_secret_string` で
    /// IPC 送信用ラッパに包む（平文取り出し経路は core 内に閉じる、CI grep
    /// `TC-CI-016` の遵守）。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 vault エラーは `PersistenceError` に写像。
    pub fn add_record(
        &self,
        kind: RecordKind,
        label: RecordLabel,
        value: SecretString,
        now: OffsetDateTime,
    ) -> Result<RecordId, PersistenceError> {
        let request = IpcRequest::AddRecord {
            kind,
            label,
            value: SerializableSecretBytes::from_secret_string(value),
            now,
        };
        match self.round_trip(&request)? {
            IpcResponse::Added { id } => Ok(id),
            IpcResponse::Error(code) => Err(PersistenceError::from(code)),
            _ => Err(unexpected_response("AddRecord")),
        }
    }

    /// daemon にレコード編集を依頼する（`--ipc edit` の主経路）。
    ///
    /// `label` と `value` は両方 `Option`、片方のみ更新も両方更新も可能
    /// （両方 `None` は CLI 側 `usecase::edit` で事前に拒否済み）。
    ///
    /// # Errors
    /// id 非存在 / IPC 失敗 / daemon 側 vault エラーは `PersistenceError` に写像。
    pub fn edit_record(
        &self,
        id: RecordId,
        label: Option<RecordLabel>,
        value: Option<SecretString>,
        now: OffsetDateTime,
    ) -> Result<RecordId, PersistenceError> {
        let request = IpcRequest::EditRecord {
            id,
            label,
            value: value.map(SerializableSecretBytes::from_secret_string),
            now,
        };
        match self.round_trip(&request)? {
            IpcResponse::Edited { id } => Ok(id),
            IpcResponse::Error(code) => Err(PersistenceError::from(code)),
            _ => Err(unexpected_response("EditRecord")),
        }
    }

    /// daemon にレコード削除を依頼する（`--ipc remove` の主経路）。
    ///
    /// 存在確認・label プレビュー取得は `list_summaries` 経由で別途行う設計
    /// （`docs/features/daemon-ipc/basic-design/flows.md §shikomi --ipc remove`）。
    /// 本メソッドは確定済みの id を `RemoveRecord` として送出するのみ。
    ///
    /// # Errors
    /// id 非存在 / IPC 失敗 / daemon 側 vault エラーは `PersistenceError` に写像。
    pub fn remove_record(&self, id: RecordId) -> Result<RecordId, PersistenceError> {
        let request = IpcRequest::RemoveRecord { id };
        match self.round_trip(&request)? {
            IpcResponse::Removed { id } => Ok(id),
            IpcResponse::Error(code) => Err(PersistenceError::from(code)),
            _ => Err(unexpected_response("RemoveRecord")),
        }
    }

    fn round_trip(&self, request: &IpcRequest) -> Result<IpcResponse, PersistenceError> {
        let mut client = self.client.lock().map_err(|_| PersistenceError::IpcIo {
            reason: "ipc client lock poisoned".to_owned(),
        })?;
        self.runtime.block_on(client.round_trip(request))
    }

    // ---------------- Sub-F (#44) Phase 2: vault サブコマンド V2 経路 ----------------
    //
    // 各メソッドは IPC 1 往復で完結し、`IpcResponse` を vault サブコマンド usecase
    // 層が必要とする戻り値型に変換する。エラーは `IpcErrorCode` → `CliError` を
    // `From<IpcErrorCode> for CliError` 経由で写像し、ExitCode SSoT への一本道を担保。
    //
    // 設計根拠: docs/features/vault-encryption/detailed-design/cli-subcommands.md
    // §処理フロー詳細（F-F1〜F-F8）

    /// `vault encrypt` 往復 (F-F1)。新生成 24 語を返す（C-19 所有権消費）。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn encrypt(
        &self,
        master_password: SecretString,
        accept_limits: bool,
    ) -> Result<Vec<SerializableSecretBytes>, CliError> {
        let request = IpcRequest::Encrypt {
            master_password: SerializableSecretBytes::from_secret_string(master_password),
            accept_limits,
        };
        match self.round_trip_for_vault(&request, "vault.encrypt")? {
            IpcResponse::Encrypted { disclosure } => Ok(disclosure),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.encrypt",
            }),
        }
    }

    /// `vault decrypt` 往復 (F-F2)。`confirmed` は CLI 側で `DECRYPT` 入力検証済の証跡。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn decrypt(&self, master_password: SecretString, confirmed: bool) -> Result<(), CliError> {
        let request = IpcRequest::Decrypt {
            master_password: SerializableSecretBytes::from_secret_string(master_password),
            confirmed,
        };
        match self.round_trip_for_vault(&request, "vault.decrypt")? {
            IpcResponse::Decrypted => Ok(()),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.decrypt",
            }),
        }
    }

    /// `vault unlock` 往復 (F-F3、password 経路 / `--recovery` 経路の両対応)。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn unlock(
        &self,
        master_password: SecretString,
        recovery: Option<Vec<SerializableSecretBytes>>,
    ) -> Result<(), CliError> {
        let request = IpcRequest::Unlock {
            master_password: SerializableSecretBytes::from_secret_string(master_password),
            recovery,
        };
        match self.round_trip_for_vault(&request, "vault.unlock")? {
            IpcResponse::Unlocked => Ok(()),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.unlock",
            }),
        }
    }

    /// `vault lock` 往復 (F-F4)。VEK 即 zeroize。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn lock(&self) -> Result<(), CliError> {
        match self.round_trip_for_vault(&IpcRequest::Lock, "vault.lock")? {
            IpcResponse::Locked => Ok(()),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.lock",
            }),
        }
    }

    /// `vault change-password` 往復 (F-F5、O(1)、VEK 不変)。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn change_password(&self, old: SecretString, new: SecretString) -> Result<(), CliError> {
        let request = IpcRequest::ChangePassword {
            old: SerializableSecretBytes::from_secret_string(old),
            new: SerializableSecretBytes::from_secret_string(new),
        };
        match self.round_trip_for_vault(&request, "vault.change_password")? {
            IpcResponse::PasswordChanged => Ok(()),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.change_password",
            }),
        }
    }

    /// `vault rekey` 往復 (F-F6)。新 24 語 + `cache_relocked` フラグを返す。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn rekey(&self, master_password: SecretString) -> Result<RekeyOutcome, CliError> {
        let request = IpcRequest::Rekey {
            master_password: SerializableSecretBytes::from_secret_string(master_password),
        };
        match self.round_trip_for_vault(&request, "vault.rekey")? {
            IpcResponse::Rekeyed {
                records_count,
                words,
                cache_relocked,
            } => Ok(RekeyOutcome {
                records_count,
                words,
                cache_relocked,
            }),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.rekey",
            }),
        }
    }

    /// `vault rotate-recovery` 往復 (F-F7)。新 24 語 + `cache_relocked` フラグを返す。
    ///
    /// # Errors
    /// IPC 失敗 / daemon 側 V2 エラーは `CliError` に写像。
    pub fn rotate_recovery(
        &self,
        master_password: SecretString,
    ) -> Result<RotateRecoveryOutcome, CliError> {
        let request = IpcRequest::RotateRecovery {
            master_password: SerializableSecretBytes::from_secret_string(master_password),
        };
        match self.round_trip_for_vault(&request, "vault.rotate_recovery")? {
            IpcResponse::RecoveryRotated {
                words,
                cache_relocked,
            } => Ok(RotateRecoveryOutcome {
                words,
                cache_relocked,
            }),
            IpcResponse::Error(code) => Err(CliError::from(code)),
            _ => Err(CliError::UnexpectedIpcResponse {
                request_kind: "vault.rotate_recovery",
            }),
        }
    }

    /// vault サブコマンド経路の round-trip helper。
    ///
    /// `round_trip` の `PersistenceError` を `CliError` へ写像する薄ラッパ。
    /// 本 helper を経由する全ての V2 メソッドは `IpcResponse::Error` のハンドリングを
    /// 個別に行うため、ここではトランスポート層エラーのみを写像する責務に絞る。
    fn round_trip_for_vault(
        &self,
        request: &IpcRequest,
        _request_kind: &'static str,
    ) -> Result<IpcResponse, CliError> {
        self.round_trip(request).map_err(CliError::from)
    }
}

// -------------------------------------------------------------------
// Sub-F (#44) Phase 3: list_summaries の戻り値型
// -------------------------------------------------------------------

/// `IpcVaultRepository::list_summaries` の戻り値（records + protection_mode）。
///
/// Sub-F (#44) C-37 で `presenter::list::render_list` のシグネチャに
/// `protection_mode: ProtectionModeBanner` を必須引数化したことに伴い、
/// daemon 応答 `IpcResponse::Records { records, protection_mode }` を本構造体に
/// 1:1 写像して呼出側に渡す。フィールドはどちらも `pub`、`Default` 実装は持たせず、
/// 呼出側が `protection_mode` を黙殺できないようにする (REQ-S16 / 型レベル強制)。
#[derive(Debug, Clone)]
pub struct ListSummariesOutcome {
    /// daemon から返された機密非含有 summary 列 (name / id / kind / 時刻のみ)。
    pub records: Vec<RecordSummary>,
    /// 保護モード (Plaintext / EncryptedLocked / EncryptedUnlocked / Unknown)。
    /// `Unknown` は CLI 側 `lib::run_list` で exit 3 fail-fast (REQ-S16 Fail-Secure)。
    pub protection_mode: ProtectionModeBanner,
}

// -------------------------------------------------------------------
// Sub-F (#44) Phase 2: vault サブコマンド戻り値型
// -------------------------------------------------------------------

/// `IpcVaultRepository::rekey` の戻り値（24 語 + `cache_relocked`）。
///
/// `cache_relocked: false` 時の MSG-S07/S20 連結表示は Phase 4 で実装。
/// Phase 2 は本構造体のみ提供し、usecase 側は `cache_relocked` を握り潰さず保持する。
#[derive(Debug)]
pub struct RekeyOutcome {
    /// 再暗号化されたレコード件数。
    pub records_count: usize,
    /// 新 BIP-39 24 語（rekey + recovery rotation 1 atomic で更新済）。
    pub words: Vec<SerializableSecretBytes>,
    /// 再 unlock の成否（false 時は MSG-S20 連結表示、C-31/C-36）。
    pub cache_relocked: bool,
}

/// `IpcVaultRepository::rotate_recovery` の戻り値（24 語 + `cache_relocked`）。
#[derive(Debug)]
pub struct RotateRecoveryOutcome {
    /// 新 BIP-39 24 語。
    pub words: Vec<SerializableSecretBytes>,
    /// 再 unlock の成否（false 時は MSG-S20 連結表示、C-31/C-36）。
    pub cache_relocked: bool,
}

#[cfg(unix)]
fn unix_default_socket_path() -> Result<PathBuf, PersistenceError> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir).join("shikomi").join("daemon.sock"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        return dirs::cache_dir()
            .map(|d| d.join("shikomi").join("daemon.sock"))
            .ok_or(PersistenceError::CannotResolveVaultDir);
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::runtime_dir()
            .map(|d| d.join("shikomi").join("daemon.sock"))
            .ok_or(PersistenceError::CannotResolveVaultDir)
    }
}

// -------------------------------------------------------------------
// Issue #75 Bug-F-007: `--vault-dir <DIR>` socket path 派生関数
// -------------------------------------------------------------------

/// `<DIR>` から daemon socket path を導出する純関数（Issue #75 Bug-F-007 解消、
/// `cli-subcommands.md` §`--vault-dir` の意味論 SSoT）。
///
/// - **Unix**: `<DIR>/shikomi.sock`
/// - **Windows**: `\\.\pipe\shikomi-{H}`（`<H>` = `windows_pipe_name_from_dir(<DIR>)`）
///
/// CLI / daemon が同一 `<DIR>` から同一 socket path を導出することを保証する純関数。
/// `connect_with_vault_dir` と `default_socket_path` の `SHIKOMI_VAULT_DIR` 経由経路で共用。
fn vault_dir_socket_path(dir: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        dir.join("shikomi.sock")
    }
    #[cfg(windows)]
    {
        let h = windows_pipe_name_from_dir(dir);
        PathBuf::from(format!(r"\\.\pipe\shikomi-{h}"))
    }
}

/// `<DIR>` 絶対パスから Windows pipe 名 `<H>` を導出する純関数（Issue #75 Bug-F-007 解消、
/// `cli-subcommands.md` §Bug-F-007 解消 §Windows pipe 名の `<H>` 契約）。
///
/// アルゴリズム:
/// 1. `<DIR>` を絶対パスに正規化（`Path::to_string_lossy()` 経由、NFC は ASCII / 一般的な
///    OS パスでは恒等変換のため省略 — 非 ASCII path での衝突可能性は受容、別 PR で再評価可）
/// 2. lowercase 化（Windows path の case-insensitive convention に整合、`to_lowercase()`）
/// 3. SHA-256 でダイジェスト
/// 4. 32 byte ダイジェスト → Base32（小文字 `a-z2-7`、パディング無し）→ 先頭 16 文字
///
/// 16 文字（5 bit × 16 = 80 bit）は 64 文字制限の Windows pipe 名内に収まり、誕生日攻撃で
/// `2^40` の衝突空間（実用上不可能）を確保する SSoT 妥協値。CLI / daemon が同一 `<DIR>` から
/// 同一 `<H>` を導出することで、どちらが先に socket bind したかに依らず接続できる。
///
/// # Examples
/// 異なる DIR は異なる pipe 名を導出し、同一 DIR は決定的に同じ名を返す。
#[cfg(any(windows, test))]
fn windows_pipe_name_from_dir(dir: &Path) -> String {
    use sha2::{Digest, Sha256};

    // (1) 絶対パスへ正規化（canonicalize は IO 失敗時に元のパスを返す best-effort）
    let abs = dir
        .to_path_buf()
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf());
    // (2) lowercase 化（Windows convention）
    let normalized = abs.to_string_lossy().to_lowercase();
    // (3) SHA-256
    let digest = Sha256::digest(normalized.as_bytes());
    // (4) Base32 (lowercase, no padding) → 先頭 16 文字
    base32_lower_no_pad(digest.as_ref(), 16)
}

/// Base32 lowercase no-pad encoder (RFC 4648 alphabet `a-z2-7`)。
///
/// `take` 文字数で切り詰めて返す（`windows_pipe_name_from_dir` 用に 16 文字固定運用）。
/// 32 byte 入力に対し最大 52 文字を生成可能だが、本関数は `take` で truncate する。
/// 純関数で外部 crate 依存を避けるため自前実装（20 行未満、Boy Scout）。
#[cfg(any(windows, test))]
fn base32_lower_no_pad(bytes: &[u8], take: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(take);
    let mut buf: u64 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        buf = (buf << 8) | u64::from(b);
        bits += 8;
        while bits >= 5 && out.len() < take {
            bits -= 5;
            let idx = ((buf >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
        if out.len() >= take {
            break;
        }
    }
    if out.len() < take && bits > 0 {
        let idx = ((buf << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

// -------------------------------------------------------------------
// 不正応答時の固定文言ヘルパ
// -------------------------------------------------------------------

/// daemon から想定外の `IpcResponse` variant を受信したときの固定文言エラー。
///
/// 動的なフォーマット（variant 名の埋め込み等）を**意図的に避ける**。daemon 側の
/// プロトコル違反は CLI から見れば「IPC 応答が壊れている」ことだけが意味を持つ。
fn unexpected_response(request_kind: &'static str) -> PersistenceError {
    PersistenceError::IpcDecode {
        reason: match request_kind {
            "ListRecords" => "unexpected response for ListRecords".to_owned(),
            "AddRecord" => "unexpected response for AddRecord".to_owned(),
            "EditRecord" => "unexpected response for EditRecord".to_owned(),
            "RemoveRecord" => "unexpected response for RemoveRecord".to_owned(),
            // `request_kind` は本ファイル内の static 文字列のみで呼ばれる契約。
            // 万一新規呼出側がここに到達しても固定文言を返す（fail safe）。
            _ => "unexpected ipc response".to_owned(),
        },
    }
}

#[cfg(test)]
mod windows_pipe_name_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn tc_f_u08_windows_pipe_name_is_deterministic_for_same_dir() {
        let dir = PathBuf::from("/tmp/shikomi-test-vault-dir-tc_f_u08");
        let h1 = windows_pipe_name_from_dir(&dir);
        let h2 = windows_pipe_name_from_dir(&dir);
        assert_eq!(h1, h2, "same DIR must derive same pipe name (purity)");
    }

    #[test]
    fn tc_f_u08_windows_pipe_name_is_16_chars_lowercase_alphanum() {
        let dir = PathBuf::from("/tmp/shikomi-test-vault-dir-tc_f_u08-len");
        let h = windows_pipe_name_from_dir(&dir);
        assert_eq!(h.len(), 16, "pipe name must be exactly 16 chars (80 bit)");
        assert!(
            h.chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c)),
            "pipe name must be Base32 lowercase no-pad (a-z2-7), got {h}"
        );
    }

    #[test]
    fn tc_f_u08_windows_pipe_name_differs_across_dirs() {
        let h1 = windows_pipe_name_from_dir(Path::new("/tmp/shikomi-vault-A"));
        let h2 = windows_pipe_name_from_dir(Path::new("/tmp/shikomi-vault-B"));
        assert_ne!(
            h1, h2,
            "different DIRs must derive different pipe names (collision resistance)"
        );
    }

    #[test]
    fn vault_dir_socket_path_is_pure() {
        let dir = PathBuf::from("/tmp/shikomi-test-vault-pure");
        let p1 = vault_dir_socket_path(&dir);
        let p2 = vault_dir_socket_path(&dir);
        assert_eq!(p1, p2, "vault_dir_socket_path must be pure");
        #[cfg(unix)]
        assert!(
            p1.ends_with("shikomi.sock"),
            "Unix socket path ends with shikomi.sock, got {p1:?}"
        );
    }

    /// TC-F-U09 (Bug-F-009): `--vault-dir <DIR>` 指定で primary が daemon 不在
    /// → fallback も失敗の経路で、エラーが `DaemonNotRunning(primary)` に変換される
    /// ことを機械検証（Option α、`presenter::error::render_daemon_not_running` 経由で
    /// MSG-S09(b) `pass --vault-dir <DIR>` 案内を発火させる SSoT 経路）。
    ///
    /// 旧実装では fallback が `CannotResolveVaultDir` を返した場合に古い infra MSG
    /// (`set SHIKOMI_VAULT_DIR or ensure a home directory is available`) が前面に出て
    /// 設計書 §Bug-F-007 §エラー文言 SSoT に違反していた（マユリ Bug-F-009 上申で発覚）。
    ///
    /// `IpcVaultRepository` は `Debug` 非実装のため `result.unwrap_err()` 経由で error を
    /// 取り出して variant 判定する（`Ok(_)` 経路に到達した場合は別 panic ヘルパで articulate）。
    #[test]
    fn tc_f_u09_connect_with_vault_dir_returns_daemon_not_running_with_primary_path() {
        // socket が存在しない tempdir を vault_dir として指定。
        // daemon は当然動いていないため primary connect は必ず失敗する。
        // fallback (default_socket_path) も daemon 不在で connect 失敗する想定 (CI 環境)。
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let result = super::IpcVaultRepository::connect_with_vault_dir(Some(tmp.path()));
        // Debug 非実装の Ok 変種は出さず、is_ok 判定で先に panic させる。
        assert!(
            result.is_err(),
            "Bug-F-009: --vault-dir を未存在 tempdir に向けたら connect は err を返すべき \
             (CI 環境で daemon が偶然走っていた場合は本テストの想定外)"
        );
        let err = result.err().expect("err checked above");
        match err {
            PersistenceError::DaemonNotRunning(p) => {
                let expected = vault_dir_socket_path(tmp.path());
                assert_eq!(
                    p, expected,
                    "DaemonNotRunning は primary (--vault-dir 由来) パスを保持して \
                     MSG-S09(b) で「指定された DIR の socket が見つからなかった」を articulate すべき"
                );
            }
            other => panic!(
                "Bug-F-009: expected DaemonNotRunning(primary) when --vault-dir is set and no \
                 daemon is reachable, got {other:?}"
            ),
        }
    }
}
