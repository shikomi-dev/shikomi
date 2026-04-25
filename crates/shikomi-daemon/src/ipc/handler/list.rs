//! `IpcRequest::ListRecords` の処理。

use shikomi_core::ipc::{IpcResponse, RecordSummary};
use shikomi_core::Vault;

pub(super) fn handle_list(vault: &Vault) -> IpcResponse {
    let summaries: Vec<RecordSummary> = vault
        .records()
        .iter()
        .map(RecordSummary::from_record)
        .collect();
    IpcResponse::Records(summaries)
}
