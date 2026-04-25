//! IPC スキーマの単一真実源（DRY、`tech-stack.md` §2.1 / `process-model.md` §4.2）。
//!
//! 本モジュールは pure 型定義のみを持ち、I/O を伴わない。
//! `tokio` / `rmp-serde` / `tokio-util` には依存しない（`tech-stack.md` §4.5）。
//! MessagePack シリアライズ実装は呼び出し側 crate（`shikomi-daemon` / `shikomi-cli`）の責務。
//!
//! 設計根拠:
//! - docs/features/daemon-ipc/basic-design/ipc-protocol.md
//! - docs/features/daemon-ipc/detailed-design/protocol-types.md

pub mod error_code;
pub mod request;
pub mod response;
pub mod secret_bytes;
pub mod summary;
pub mod version;

pub use error_code::IpcErrorCode;
pub use request::IpcRequest;
pub use response::IpcResponse;
pub use secret_bytes::SerializableSecretBytes;
pub use summary::RecordSummary;
pub use version::IpcProtocolVersion;

// -------------------------------------------------------------------
// プロトコル定数
// -------------------------------------------------------------------

/// IPC フレームの最大長（16 MiB）。
///
/// daemon / cli 双方の `LengthDelimitedCodec::max_frame_length` に渡し、超過は接続切断（DoS 対策）。
///
/// 設計根拠: `process-model.md` §4.2、`detailed-design/daemon-runtime.md §framing::codec`
pub const MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024;
