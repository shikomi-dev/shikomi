//! `IpcRequest::ListRecords` の処理。
//!
//! Sub-F (#44) で `IpcResponse::Records` が `{ records, protection_mode }` 構造体に
//! 変更され、保護モードバナー (REQ-S16) を同梱するようになった。本ハンドラは
//! `Vault::protection_mode` のみで Plaintext / Encrypted を判定する。
//! 「Locked / Unlocked」の区別は dispatch_v2 経由で `VekCache::is_unlocked` を見る
//! 経路があるが、この pure ハンドラは vault のみを引数で受けるため、Plaintext と
//! `Unknown` のみを返す。Encrypted Locked/Unlocked 判定は `dispatch_v2` の上位経路で
//! 必要に応じて差し替える (Sub-F 工程3 後続 Phase で `dispatch_v2` 側で
//! protection_mode を上書きする責務追加予定、現 Phase 1 では Plaintext のみ確実)。

use shikomi_core::ipc::{IpcResponse, ProtectionModeBanner, RecordSummary};
use shikomi_core::{ProtectionMode, Vault};

pub(super) fn handle_list(vault: &Vault) -> IpcResponse {
    let summaries: Vec<RecordSummary> = vault
        .records()
        .iter()
        .map(RecordSummary::from_record)
        .collect();
    let protection_mode = match vault.protection_mode() {
        ProtectionMode::Plaintext => ProtectionModeBanner::Plaintext,
        // Encrypted の Locked/Unlocked 区別は本 pure ハンドラでは判定不能
        // (`VekCache` を引数に持たない)。Sub-F Phase 1 では Locked 既定で返却し、
        // Phase 4 (`dispatch_v2`) で Unlocked への上書き経路を追加する。
        ProtectionMode::Encrypted => ProtectionModeBanner::EncryptedLocked,
        // `#[non_exhaustive]` cross-crate 防御: 未知 variant は `Unknown` で fail-secure
        _ => ProtectionModeBanner::Unknown,
    };
    IpcResponse::Records {
        records: summaries,
        protection_mode,
    }
}
