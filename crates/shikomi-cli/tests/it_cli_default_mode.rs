//! 結合テスト — daemon-default-mode / cli / TC-IT-110〜114
//!
//! 設計書: docs/features/daemon-default-mode/cli/test-design/integration.md
//! 実行レシピ: just test-daemon
//! 対応 Issue: #126

#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use assert_cmd::Command;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use predicates::prelude::*;
use shikomi_core::ipc::{
    IpcProtocolVersion, IpcRequest, IpcResponse, ProtectionModeBanner, MAX_FRAME_LENGTH,
};
use tempfile::TempDir;
use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use common::tighten_perms_unix;

// ---------------------------------------------------------------------------
// ヘルパー
// ---------------------------------------------------------------------------

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .little_endian()
        .length_field_length(4)
        .new_codec()
}

/// 一時ディレクトリを 0o700 で作成する（tight_tempdir）。
fn tight_tempdir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    dir
}

/// in-process ListRecords スタブサーバを別スレッドで起動する。
///
/// `xdg_dir` の `shikomi/` サブディレクトリに daemon.sock を作成し、
/// 接続を受け入れて Handshake + ListRecords (0件 Plaintext) に応答する。
/// `shikomi` サブプロセスは `XDG_RUNTIME_DIR=xdg_dir` で解決する。
fn spawn_list_stub_server(xdg_dir: &std::path::Path) {
    let sock_dir = xdg_dir.join("shikomi");
    std::fs::create_dir_all(&sock_dir).expect("create shikomi dir");
    std::fs::set_permissions(&sock_dir, std::fs::Permissions::from_mode(0o700))
        .expect("chmod shikomi dir");
    let sock_path = sock_dir.join("daemon.sock");

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build runtime");
        rt.block_on(async move {
            let listener = UnixListener::bind(&sock_path).expect("bind stub");
            std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))
                .expect("chmod sock");
            // 複数接続を受け付けるためループ（テスト中に複数 shikomi プロセスが接続する場合）
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(handle_connection(stream));
                }
            }
        });
    });
    // accept loop 開始を待つ
    std::thread::sleep(Duration::from_millis(50));
}

async fn handle_connection(stream: UnixStream) {
    let mut framed: Framed<UnixStream, LengthDelimitedCodec> = Framed::new(stream, codec());
    // Handshake
    if let Some(Ok(frame)) = framed.next().await {
        let req: IpcRequest = match rmp_serde::from_slice(&frame) {
            Ok(r) => r,
            Err(_) => return,
        };
        if matches!(req, IpcRequest::Handshake { .. }) {
            let resp = IpcResponse::Handshake {
                server_version: IpcProtocolVersion::V2,
            };
            let bytes = rmp_serde::to_vec(&resp).unwrap();
            if framed.send(Bytes::from(bytes)).await.is_err() {
                return;
            }
        }
    }
    // ListRecords
    if let Some(Ok(frame)) = framed.next().await {
        let req: IpcRequest = match rmp_serde::from_slice(&frame) {
            Ok(r) => r,
            Err(_) => return,
        };
        if matches!(req, IpcRequest::ListRecords) {
            let resp = IpcResponse::Records {
                records: vec![],
                protection_mode: ProtectionModeBanner::Plaintext,
            };
            let bytes = rmp_serde::to_vec(&resp).unwrap();
            let _ = framed.send(Bytes::from(bytes)).await;
        }
    }
}

// ---------------------------------------------------------------------------
// TC-IT-110: shikomi list（daemon 起動中 / --no-ipc なし）→ IPC 経路で成功 + MSG-CLI-051 非出力
// TC-IT-113: 同じコマンドで stderr の MSG-CLI-051 文言不在確認（同一テスト関数内で多アサーション）
// 設計書: integration.md §TC-IT-110, §TC-IT-113
// ---------------------------------------------------------------------------

#[test]
fn tc_it_110_and_113_list_via_ipc_default_succeeds_and_no_msg_cli_051() {
    let xdg_dir = tight_tempdir();
    spawn_list_stub_server(xdg_dir.path());

    let output = Command::cargo_bin("shikomi")
        .expect("cargo_bin shikomi")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", xdg_dir.path())
        .args(["list"])
        .assert()
        // TC-IT-110
        .success()
        // TC-IT-113: stderr に MSG-CLI-051 文言が含まれない
        .stderr(predicate::str::contains("IPC mode").not())
        .stderr(predicate::str::contains("--ipc").not())
        .stderr(predicate::str::contains("opt-in").not())
        .stderr(predicate::str::contains("MSG-CLI-051").not());

    // --ipc フラグを明示していないことをコメントで補足（IPC 既定の検証）
    let _ = output;
}

// ---------------------------------------------------------------------------
// TC-IT-111: shikomi --no-ipc list（daemon 不要）→ SQLite 直結で成功
// 設計書: integration.md §TC-IT-111
// ---------------------------------------------------------------------------

#[test]
fn tc_it_111_no_ipc_list_uses_sqlite_directly() {
    let vault_dir = tight_tempdir();
    // 空 vault.db を事前生成（add で初期化）
    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("LANG")
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args(["--no-ipc", "add", "--kind", "text", "--label", "L0", "--value", "V0"])
        .assert()
        .success();
    // --no-ipc list 実行（daemon 不在でも成功するはず）
    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("LANG")
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        // XDG_RUNTIME_DIR を意図的に空の tempdir に向けてソケット不在を保証
        .env("XDG_RUNTIME_DIR", "/tmp/__shikomi_no_daemon_it111")
        .args(["--no-ipc", "list"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// TC-IT-112: daemon 未起動 + shikomi list → MSG-CLI-110 + exit 1
// 設計書: integration.md §TC-IT-112
// ---------------------------------------------------------------------------

#[test]
fn tc_it_112_list_without_daemon_fails_with_msg_cli_110() {
    let empty_xdg = tight_tempdir();
    // shikomi サブディレクトリを作成するがソケットは作らない
    std::fs::create_dir_all(empty_xdg.path().join("shikomi")).unwrap();

    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", empty_xdg.path())
        .args(["list"])
        .assert()
        .failure()
        .code(1)
        // MSG-CLI-110 原因文
        .stderr(
            predicate::str::contains("not running")
                .or(predicate::str::contains("shikomi-daemon")),
        )
        // hint に daemon 起動コマンド案内
        .stderr(predicate::str::contains("hint:"))
        // Phase 2 廃止フラグを案内しない
        .stderr(predicate::str::contains("--ipc").not());
}

// ---------------------------------------------------------------------------
// TC-IT-114: shikomi --no-ipc vault encrypt → vault IPC 強制（--no-ipc 無視）
// 期待: MSG-CLI-052 先行出力 → MSG-CLI-110
// 設計書: integration.md §TC-IT-114
// ---------------------------------------------------------------------------

#[test]
fn tc_it_114_no_ipc_vault_encrypt_forces_ipc_with_msg_cli_052_first() {
    let empty_xdg = tight_tempdir();
    std::fs::create_dir_all(empty_xdg.path().join("shikomi")).unwrap();

    let output = Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", empty_xdg.path())
        .args(["--no-ipc", "vault", "encrypt"])
        .assert()
        .failure()
        .code(1)
        // MSG-CLI-052 が含まれる
        .stderr(predicate::str::contains(
            "vault commands always use IPC; --no-ipc does not apply",
        ))
        // MSG-CLI-110 が含まれる
        .stderr(
            predicate::str::contains("not running")
                .or(predicate::str::contains("shikomi-daemon")),
        )
        .get_output()
        .clone();

    // MSG-CLI-052 が MSG-CLI-110 より先に出力されることを確認
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let pos_052 = stderr
        .find("vault commands always use IPC")
        .expect("MSG-CLI-052 should appear in stderr");
    let pos_110 = stderr
        .find("not running")
        .or_else(|| stderr.find("shikomi-daemon is not"))
        .expect("MSG-CLI-110 should appear in stderr");
    assert!(
        pos_052 < pos_110,
        "MSG-CLI-052 must appear before MSG-CLI-110 in stderr. \
         pos_052={pos_052}, pos_110={pos_110}, stderr:\n{stderr}"
    );
}
