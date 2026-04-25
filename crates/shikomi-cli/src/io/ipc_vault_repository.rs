//! `IpcVaultRepository` — `VaultRepository` trait の IPC クライアント実装。
//!
//! 設計根拠:
//! - docs/features/daemon-ipc/detailed-design/ipc-vault-repository.md
//! - docs/features/daemon-ipc/basic-design/flows.md §CLI 側

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use shikomi_core::ipc::{
    IpcErrorCode, IpcRequest, IpcResponse, RecordSummary, SerializableSecretBytes,
};
use shikomi_core::{
    DomainError, Record, RecordId, RecordKind, RecordPayload, SecretBytes, SecretString, Vault,
    VaultHeader, VaultVersion,
};
use shikomi_infra::persistence::{PersistenceError, VaultRepository};
use time::OffsetDateTime;
use tokio::runtime::Runtime;

use super::ipc_client::IpcClient;

// -------------------------------------------------------------------
// IpcVaultRepository
// -------------------------------------------------------------------

/// daemon 経由で vault 操作を行う `VaultRepository` 実装。
///
/// 実装方針（`detailed-design/ipc-vault-repository.md §load の実装方針` 案 C）:
/// - `load` は `IpcRequest::ListRecords` を発行し、`RecordSummary` のリストを取得。
///   ローカル shadow に保持し、`Vault` 集約として再構築する（Secret 値は擬似プレースホルダ）。
/// - `save` は新 `Vault` と shadow の差分を計算し、IPC `Add/Edit/Remove` を発行する。
///
/// 既知の制約（設計書の `案 C 問題点` に明示）:
/// - **Add 経路の ID 不整合**: CLI の `usecase::add` が生成した `RecordId` と、daemon が
///   `IpcRequest::AddRecord` 受信時に独立生成する `RecordId` は一致しない。CLI は
///   `Added` 応答の id を破棄しローカル vault の id を `presenter::success::render_added` に
///   渡すため、ユーザに表示される id は daemon 側 vault に存在しない。本制約は設計時点で
///   認識済み（`detailed-design/ipc-vault-repository.md` 案 C 問題点）で、`VaultRepository`
///   trait 分割（案 B）への移行 PR は本 Issue のスコープ外。`--ipc` を opt-in に留める根拠の
///   一つでもある。
pub struct IpcVaultRepository {
    runtime: Runtime,
    client: Mutex<IpcClient>,
    socket_path: PathBuf,
    shadow: Mutex<ShadowState>,
}

#[derive(Default)]
struct ShadowState {
    /// 最終 `load` 時点の `RecordSummary` リスト（id 順）。
    summaries: Vec<RecordSummary>,
    /// 投影 Vault に乗せた Plaintext(SecretString) のバイト列 snapshot（id → bytes）。
    /// `save` 時に CLI 側で保持された新 `Vault` のペイロードと byte 比較して
    /// 変更検出を行う（Secret kind の値変更も label 変更と独立に検出可能）。
    payloads: HashMap<RecordId, Vec<u8>>,
}

impl IpcVaultRepository {
    /// daemon に接続して `IpcVaultRepository` を構築する（同期 wrapper）。
    ///
    /// 内部で `tokio::runtime::Builder::new_current_thread().enable_all().build()?` を起動。
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
            shadow: Mutex::new(ShadowState::default()),
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

    fn round_trip(&self, request: &IpcRequest) -> Result<IpcResponse, PersistenceError> {
        let mut client = self.client.lock().map_err(|_| PersistenceError::IpcIo {
            reason: "ipc client lock poisoned".to_owned(),
        })?;
        self.runtime.block_on(client.round_trip(request))
    }
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
// VaultRepository trait 実装
// -------------------------------------------------------------------

impl VaultRepository for IpcVaultRepository {
    fn load(&self) -> Result<Vault, PersistenceError> {
        let response = self.round_trip(&IpcRequest::ListRecords)?;
        let summaries = match response {
            IpcResponse::Records(s) => s,
            IpcResponse::Error(code) => return Err(map_ipc_error_code(&code)),
            _ => {
                return Err(PersistenceError::IpcDecode {
                    reason: "unexpected response variant".to_owned(),
                });
            }
        };

        let now = OffsetDateTime::now_utc();

        // 投影 Vault を構築（Secret は空 placeholder、Text は preview を平文として復元）
        let header = VaultHeader::new_plaintext(VaultVersion::CURRENT, now).map_err(|e| {
            PersistenceError::IpcDecode {
                reason: format!("vault projection failed: {}", domain_marker(&e)),
            }
        })?;
        let mut vault = Vault::new(header);
        let mut payload_snapshot: HashMap<RecordId, Vec<u8>> = HashMap::new();
        for s in &summaries {
            let placeholder = match s.kind {
                RecordKind::Text => s.value_preview.clone().unwrap_or_default(),
                RecordKind::Secret => String::new(),
            };
            payload_snapshot.insert(s.id.clone(), placeholder.as_bytes().to_vec());
            let payload = RecordPayload::Plaintext(SecretString::from_string(placeholder));
            let record = Record::new(s.id.clone(), s.kind, s.label.clone(), payload, now);
            vault
                .add_record(record)
                .map_err(|e| PersistenceError::IpcDecode {
                    reason: format!("vault projection failed: {}", domain_marker(&e)),
                })?;
        }

        let mut shadow = self.shadow.lock().map_err(|_| PersistenceError::IpcIo {
            reason: "shadow lock poisoned".to_owned(),
        })?;
        shadow.summaries = summaries;
        shadow.payloads = payload_snapshot;
        drop(shadow);

        Ok(vault)
    }

    fn save(&self, vault: &Vault) -> Result<(), PersistenceError> {
        let snapshot: Vec<&Record> = vault.records().iter().collect();

        let shadow = self.shadow.lock().map_err(|_| PersistenceError::IpcIo {
            reason: "shadow lock poisoned".to_owned(),
        })?;
        let shadow_summaries_owned: Vec<RecordSummary> = shadow.summaries.clone();
        let shadow_payloads_owned: HashMap<RecordId, Vec<u8>> = shadow.payloads.clone();
        drop(shadow);

        let shadow_ids: std::collections::HashSet<&RecordId> =
            shadow_summaries_owned.iter().map(|s| &s.id).collect();
        let shadow_summaries: HashMap<&RecordId, &RecordSummary> =
            shadow_summaries_owned.iter().map(|s| (&s.id, s)).collect();

        let now = OffsetDateTime::now_utc();

        // 削除: shadow にあって snapshot にない id
        let snapshot_ids: std::collections::HashSet<&RecordId> =
            snapshot.iter().map(|r| r.id()).collect();
        for shadow_id in &shadow_ids {
            if !snapshot_ids.contains(*shadow_id) {
                let request = IpcRequest::RemoveRecord {
                    id: (*shadow_id).clone(),
                };
                expect_ok(self.round_trip(&request)?)?;
            }
        }

        // 追加 / 更新
        for record in &snapshot {
            let id = record.id();
            if shadow_ids.contains(id) {
                // 既存: 変更検出
                let summary = shadow_summaries.get(id).copied();
                let label_changed = summary
                    .map(|s| s.label.as_str() != record.label().as_str())
                    .unwrap_or(true);
                let new_bytes = extract_payload_bytes(record)?;
                let payload_changed = match shadow_payloads_owned.get(id) {
                    Some(prev) => prev.as_slice() != new_bytes.as_slice(),
                    None => true,
                };
                if !label_changed && !payload_changed {
                    continue;
                }
                let request = IpcRequest::EditRecord {
                    id: id.clone(),
                    label: if label_changed {
                        Some(record.label().clone())
                    } else {
                        None
                    },
                    value: if payload_changed {
                        Some(SerializableSecretBytes::new(SecretBytes::from_vec(
                            new_bytes,
                        )))
                    } else {
                        None
                    },
                    now,
                };
                expect_ok(self.round_trip(&request)?)?;
            } else {
                // 新規追加
                let new_bytes = extract_payload_bytes(record)?;
                let request = IpcRequest::AddRecord {
                    kind: record.kind(),
                    label: record.label().clone(),
                    value: SerializableSecretBytes::new(SecretBytes::from_vec(new_bytes)),
                    now,
                };
                expect_ok(self.round_trip(&request)?)?;
            }
        }

        // shadow を最新に
        let mut shadow = self.shadow.lock().map_err(|_| PersistenceError::IpcIo {
            reason: "shadow lock poisoned".to_owned(),
        })?;
        shadow.summaries = snapshot
            .iter()
            .map(|r| RecordSummary::from_record(r))
            .collect();
        shadow.payloads = snapshot
            .iter()
            .map(|r| {
                let bytes = extract_payload_bytes(r).unwrap_or_default();
                (r.id().clone(), bytes)
            })
            .collect();

        Ok(())
    }

    fn exists(&self) -> Result<bool, PersistenceError> {
        // daemon が稼働している = vault が存在する（daemon 起動時 load 成功済み）。
        Ok(true)
    }
}

// -------------------------------------------------------------------
// 補助関数
// -------------------------------------------------------------------

fn expect_ok(response: IpcResponse) -> Result<IpcResponse, PersistenceError> {
    match response {
        IpcResponse::Error(code) => Err(map_ipc_error_code(&code)),
        other => Ok(other),
    }
}

fn map_ipc_error_code(code: &IpcErrorCode) -> PersistenceError {
    match code {
        IpcErrorCode::EncryptionUnsupported => PersistenceError::UnsupportedYet {
            feature: "encrypted vault persistence",
            tracking_issue: None,
        },
        IpcErrorCode::NotFound { .. } => PersistenceError::IpcDecode {
            reason: "record not found".to_owned(),
        },
        IpcErrorCode::InvalidLabel { .. } => PersistenceError::IpcDecode {
            reason: "invalid label".to_owned(),
        },
        IpcErrorCode::Persistence { .. } => PersistenceError::IpcIo {
            reason: "persistence error".to_owned(),
        },
        IpcErrorCode::Domain { .. } => PersistenceError::IpcIo {
            reason: "domain error".to_owned(),
        },
        IpcErrorCode::Internal { .. } => PersistenceError::IpcIo {
            reason: "internal error".to_owned(),
        },
        _ => PersistenceError::IpcIo {
            reason: "unknown error code".to_owned(),
        },
    }
}

/// `Record` のペイロードを UTF-8 バイト列として抽出する（差分計算用 + IPC 送信用）。
///
/// `Plaintext(SecretString)` のみサポート（暗号化モードは Phase 1 未対応）。
/// secret 取り出しは core 側 `clone_secret_string_bytes` に集約する。
fn extract_payload_bytes(record: &Record) -> Result<Vec<u8>, PersistenceError> {
    match record.payload() {
        RecordPayload::Plaintext(secret) => {
            Ok(shikomi_core::secret::clone_secret_string_bytes(secret))
        }
        RecordPayload::Encrypted(_) => Err(PersistenceError::UnsupportedYet {
            feature: "encrypted vault persistence",
            tracking_issue: None,
        }),
    }
}

fn domain_marker(err: &DomainError) -> &'static str {
    // DomainError は `#[non_exhaustive]`（後続バリアント追加に対する破壊的変更回避）。
    // 未知バリアントは固定文言「domain error」へ写像し、reason 漏洩は起こさない。
    match err {
        DomainError::InvalidRecordLabel(_) => "invalid label",
        DomainError::InvalidRecordId(_) => "invalid record id",
        DomainError::InvalidRecordPayload(_) => "invalid payload",
        DomainError::InvalidVaultHeader(_) => "invalid header",
        DomainError::VaultConsistencyError(_) => "vault consistency",
        DomainError::UnsupportedVaultVersion(_) => "unsupported vault version",
        _ => "domain error",
    }
}
