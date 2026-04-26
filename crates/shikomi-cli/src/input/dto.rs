//! UseCase への入力 DTO。
//!
//! clap 派生型（生 `String` / `bool` を持つ）とは分離し、ドメイン検証済み型のみを保持する。
//! `run()` のみが両者の変換役を担い、UseCase 層には検証済みの DTO を渡す（Parse, don't validate）。
//!
//! 設計根拠: docs/features/cli-vault-commands/detailed-design/public-api.md
//! §`shikomi_cli::input`

use shikomi_core::{RecordId, RecordKind, RecordLabel, SecretString};

// -------------------------------------------------------------------
// AddInput
// -------------------------------------------------------------------

/// `add` UseCase の入力 DTO。
#[derive(Debug)]
pub struct AddInput {
    pub kind: RecordKind,
    pub label: RecordLabel,
    pub value: SecretString,
}

// -------------------------------------------------------------------
// EditInput
// -------------------------------------------------------------------

/// `edit` UseCase の入力 DTO。
///
/// `label` / `value` は少なくとも 1 つが `Some` である必要がある（呼び出し側の事前条件、
/// `run_edit` で検証）。`kind` フィールドは持たない（Phase 1 スコープ外）。
#[derive(Debug)]
pub struct EditInput {
    pub id: RecordId,
    pub label: Option<RecordLabel>,
    pub value: Option<SecretString>,
}

// -------------------------------------------------------------------
// ConfirmedRemoveInput
// -------------------------------------------------------------------

/// `remove` UseCase の入力 DTO。**型の存在そのものが「確認済み」を表す**。
///
/// `confirmed: bool` フィールドは持たない。`ConfirmedRemoveInput::new(id)` を呼べるのは
/// `run_remove` の TTY プロンプト or `--yes` 経路のみ。非 TTY + `--yes` 未指定は
/// `CliError::NonInteractiveRemove` で Fail Fast されて UseCase 到達前に return する。
///
/// 設計根拠: docs/features/cli-vault-commands/basic-design/error.md
/// §確認強制の型レベル実装
#[derive(Debug)]
pub struct ConfirmedRemoveInput {
    id: RecordId,
}

impl ConfirmedRemoveInput {
    /// 確認経路を経たことを呼び出し側責任で宣言しながら入力を構築する。
    #[must_use]
    pub fn new(id: RecordId) -> Self {
        Self { id }
    }

    /// 内包する `RecordId` への参照を返す。
    #[must_use]
    pub fn id(&self) -> &RecordId {
        &self.id
    }
}

// -------------------------------------------------------------------
// テスト
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shikomi_core::RecordId;
    use uuid::Uuid;

    #[test]
    fn test_confirmed_remove_input_new_constructs_from_record_id() {
        let id = RecordId::new(Uuid::now_v7()).unwrap();
        let input = ConfirmedRemoveInput::new(id.clone());
        assert_eq!(input.id(), &id);
    }
}
