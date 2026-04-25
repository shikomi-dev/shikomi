//! `PeerCredentialSource` trait の in-test 実装（integration.md §8.1 の契約どおり trait 注入）。
//!
//! 本番コードには `#[cfg(test)]` バイパス分岐を置かない方針のため、IT で使う
//! mock 実装を tests/common/ 配下に閉じ込めた。
//!
//! 対応 Issue: #26

use shikomi_daemon::permission::peer_credential::{
    PeerCredentialSource, PeerIdentity, PeerVerificationError,
};

/// 常に同じ Uid を `peer` / `self` として返す mock。`verify` が成功する。
pub struct MatchingUidPeer {
    pub uid: u32,
}

impl PeerCredentialSource for MatchingUidPeer {
    fn peer_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        Ok(PeerIdentity::Uid(self.uid))
    }
    fn self_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        Ok(PeerIdentity::Uid(self.uid))
    }
}

/// `peer` と `self` で異なる Uid を返す mock。`verify` が `IdentityMismatch` を返す。
pub struct MismatchingUidPeer {
    pub peer: u32,
    pub self_uid: u32,
}

impl PeerCredentialSource for MismatchingUidPeer {
    fn peer_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        Ok(PeerIdentity::Uid(self.peer))
    }
    fn self_identity(&self) -> Result<PeerIdentity, PeerVerificationError> {
        Ok(PeerIdentity::Uid(self.self_uid))
    }
}
