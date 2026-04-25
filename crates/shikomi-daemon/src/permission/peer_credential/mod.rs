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

// -------------------------------------------------------------------
// ユニットテスト（テスト設計 `test-design/unit.md §2.6 TC-UT-020〜024`）
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    //! `PeerCredentialSource` trait の動作を in-test mock 経由で検証する。
    //!
    //! 設計根拠: `test-design/unit.md §2.6 / §3.1` — trait 注入一本化、
    //! 本番コードに `#[cfg(test)]` バイパスを書かない。
    //!
    //! 対応 Issue: #26

    use super::*;

    /// テスト用 trait 実装。`peer` / `slf` を直接指定してロジック単体を検証する。
    struct TestPeerCredential {
        peer: Result<PeerIdentity, PeerIdFailure>,
        slf: Result<PeerIdentity, PeerIdFailure>,
    }

    /// `std::io::Error` は `Clone` 不可のため lookup 失敗の種類を enum で表現。
    #[derive(Clone)]
    enum PeerIdFailure {
        Unsupported,
    }

    fn to_err(f: &PeerIdFailure) -> PeerVerificationError {
        match f {
            PeerIdFailure::Unsupported => PeerVerificationError::Lookup(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "test",
            )),
        }
    }

    impl PeerCredentialSource for TestPeerCredential {
        fn peer_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
            self.peer.as_ref().cloned().map_err(to_err)
        }
        fn self_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
            self.slf.as_ref().cloned().map_err(to_err)
        }
    }

    /// TC-UT-020: ピア識別子一致（Uid）→ Ok。
    #[test]
    fn test_verify_uid_match_returns_ok() {
        let src = TestPeerCredential {
            peer: Ok(PeerIdentity::Uid(1000)),
            slf: Ok(PeerIdentity::Uid(1000)),
        };
        assert!(verify(&src).is_ok());
    }

    /// TC-UT-021: ピア識別子不一致（Uid）→ IdentityMismatch。
    #[test]
    fn test_verify_uid_mismatch_returns_identity_mismatch() {
        let src = TestPeerCredential {
            peer: Ok(PeerIdentity::Uid(2000)),
            slf: Ok(PeerIdentity::Uid(1000)),
        };
        assert!(matches!(
            verify(&src),
            Err(PeerVerificationError::IdentityMismatch)
        ));
    }

    /// TC-UT-022: peer lookup 失敗 → Lookup error（接続切断経路）。
    #[test]
    fn test_verify_peer_lookup_failure_returns_lookup() {
        let src = TestPeerCredential {
            peer: Err(PeerIdFailure::Unsupported),
            slf: Ok(PeerIdentity::Uid(1000)),
        };
        assert!(matches!(
            verify(&src),
            Err(PeerVerificationError::Lookup(_))
        ));
    }

    /// TC-UT-023: Windows 版 正常（Sid）→ Ok。
    #[test]
    fn test_verify_sid_match_returns_ok() {
        let sid = "S-1-5-21-1111-2222-3333-1001".to_owned();
        let src = TestPeerCredential {
            peer: Ok(PeerIdentity::Sid(sid.clone())),
            slf: Ok(PeerIdentity::Sid(sid)),
        };
        assert!(verify(&src).is_ok());
    }

    /// TC-UT-024: Windows 版 異常（Sid 不一致）→ IdentityMismatch。
    #[test]
    fn test_verify_sid_mismatch_returns_identity_mismatch() {
        let src = TestPeerCredential {
            peer: Ok(PeerIdentity::Sid("S-1-5-21-111-222-333-1001".into())),
            slf: Ok(PeerIdentity::Sid("S-1-5-21-111-222-333-1002".into())),
        };
        assert!(matches!(
            verify(&src),
            Err(PeerVerificationError::IdentityMismatch)
        ));
    }

    /// TC-UT-024+: Uid と Sid の型違いも不一致として検出される（型システム同型性）。
    #[test]
    fn test_verify_different_identity_types_are_unequal() {
        let src = TestPeerCredential {
            peer: Ok(PeerIdentity::Uid(1000)),
            slf: Ok(PeerIdentity::Sid("S-1-5-21-0-0-0-1001".into())),
        };
        assert!(matches!(
            verify(&src),
            Err(PeerVerificationError::IdentityMismatch)
        ));
    }
}
