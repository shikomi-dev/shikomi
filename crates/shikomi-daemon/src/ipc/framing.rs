//! IPC フレーミング設定（`tokio_util::codec::LengthDelimitedCodec`、最大 16 MiB）。

use shikomi_core::ipc::MAX_FRAME_LENGTH;
use tokio_util::codec::LengthDelimitedCodec;

/// 4 byte LE length prefix + payload の codec を構築する。
///
/// daemon / cli 双方が同じ codec 設定で接続する。
#[must_use]
pub fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_length(4)
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_codec()
}
