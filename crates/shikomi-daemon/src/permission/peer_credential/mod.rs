//! ピア資格情報検証の OS 非依存エントリ + trait 抽象。
//!
//! 設計根拠:
//! - docs/features/daemon-ipc/basic-design/security.md §ピア資格情報検証の設計詳細
//! - docs/features/daemon-ipc/test-design/integration.md §8.1 ピア検証のテスト時バイパス経路
//!
//! 本番コードに `#[cfg(test)]` 分岐 / env 裏口を**置かない**（trait 一本化）。

use thiserror::Error;

// -------------------------------------------------------------------
// PeerVerificationError
// -------------------------------------------------------------------

/// ピア検証失敗の理由（識別子は OS 依存値、log には出すがクライアントへは返さない）。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PeerVerificationError {
    /// ピア識別子が daemon プロセスのものと一致しない。
    #[error("peer identity mismatch")]
    IdentityMismatch,
    /// ピア識別子の取得失敗（kernel API エラー等）。
    #[error("peer identity lookup failed: {0}")]
    Lookup(std::io::Error),
}

// -------------------------------------------------------------------
// PeerCredentialSource trait
// -------------------------------------------------------------------

/// 接続ストリームから「ピア識別子」と「自プロセス識別子」を取得する trait。
///
/// 本番実装: `impl PeerCredentialSource for tokio::net::UnixStream`（cfg unix）/
/// `impl PeerCredentialSource for tokio::net::windows::named_pipe::NamedPipeServer`（cfg windows）。
/// テスト用は `tests/common/peer_mock.rs` に定義（本 crate からは見えないが trait で injection 可能）。
pub trait PeerCredentialSource {
    /// ピア識別子（Unix: UID、Windows: SID 文字列）。
    ///
    /// # Errors
    /// kernel API 失敗時 `PeerVerificationError::Lookup`。
    fn peer_identity(&self) -> Result<PeerIdentity, PeerVerificationError>;

    /// 自プロセスの識別子（daemon 自身の UID / SID）。
    ///
    /// # Errors
    /// 同上。
    fn self_identity(&self) -> Result<PeerIdentity, PeerVerificationError>;
}

// -------------------------------------------------------------------
// PeerIdentity
// -------------------------------------------------------------------

/// OS 非依存のピア識別子。比較は `PartialEq` のみ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerIdentity {
    /// Unix: 接続元 UID。
    Uid(u32),
    /// Windows: 接続元 User SID（文字列形式）。
    Sid(String),
}

// -------------------------------------------------------------------
// verify 関数
// -------------------------------------------------------------------

/// ピア識別子と自プロセス識別子を比較し、不一致なら `IdentityMismatch` を返す。
///
/// 設計根拠:
/// - UDS `0600` / Named Pipe owner-only で OS レイヤが先に拒否するが、本検証は
///   多層防御として必須（`process-model.md` §4.2 認証 (1)）
///
/// # Errors
/// `PeerVerificationError::IdentityMismatch` / `Lookup`。
pub fn verify<S: PeerCredentialSource + ?Sized>(source: &S) -> Result<(), PeerVerificationError> {
    let peer = source.peer_identity()?;
    let self_id = source.self_identity()?;
    if peer == self_id {
        Ok(())
    } else {
        Err(PeerVerificationError::IdentityMismatch)
    }
}
