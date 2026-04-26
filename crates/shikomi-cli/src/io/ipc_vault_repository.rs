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

    /// OS デフォルトのソケットパスを解決する。
    ///
    /// - **Linux**: `$XDG_RUNTIME_DIR/shikomi/daemon.sock`、未設定時は `dirs::runtime_dir()`
    /// - **macOS**: `dirs::cache_dir()/shikomi/daemon.sock`
    /// - **Windows**: `\\.\pipe\shikomi-daemon-{user-sid}`
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
