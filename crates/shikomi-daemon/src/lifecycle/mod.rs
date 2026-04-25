//! daemon ライフサイクル（シングルインスタンス先取り、graceful shutdown、ソケットパス解決）。

pub mod shutdown;
pub mod signal_mask;
pub mod single_instance;
pub mod socket_path;
