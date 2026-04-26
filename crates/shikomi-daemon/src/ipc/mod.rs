//! daemon の IPC サーバ実装（transport / framing / handshake / handler / server / v2_handler）。

pub mod framing;
pub mod handler;
pub mod handshake;
pub mod server;
pub mod transport;
pub mod v2_handler;
