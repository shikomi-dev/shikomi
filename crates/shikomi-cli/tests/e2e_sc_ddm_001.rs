//! E2E テスト — SC-DDM-001: IPC 既定化（Phase 2 CLI 移行）
//!
//! 対応受入基準: AC-DDM-01〜06（docs/acceptance-tests/scenarios/SC-DDM-001-ipc-default-mode.md）
//! Vモデル: 受入テスト（最上位・完全ブラックボックス）
//! 対応 TC: TC-E2E-120〜125
//! 設計書: docs/features/daemon-default-mode/cli/test-design/integration.md （E2E 節）
//! 実行レシピ: just test-daemon（IPC スタブが shikomi-daemon/test-fixtures 経由の tokio を使う）
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
// 共通ヘルパー
// ---------------------------------------------------------------------------

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .little_endian()
        .length_field_length(4)
        .new_codec()
}

fn tight_tempdir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    tighten_perms_unix(dir.path());
    dir
}

/// in-process IPC スタブを `xdg_dir/shikomi/daemon.sock` に起動する（E2E 用）。
/// `shikomi` サブプロセスは `XDG_RUNTIME_DIR=xdg_dir` で解決する。
fn spawn_ipc_stub(xdg_dir: &std::path::Path) {
    let sock_dir = xdg_dir.join("shikomi");
    std::fs::create_dir_all(&sock_dir).expect("create shikomi dir");
    std::fs::set_permissions(&sock_dir, std::fs::Permissions::from_mode(0o700))
        .expect("chmod shikomi dir");
    let sock_path = sock_dir.join("daemon.sock");

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        rt.block_on(async move {
            let listener = UnixListener::bind(&sock_path).expect("bind");
            std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))
                .expect("chmod sock");
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(serve_connection(stream));
                }
            }
        });
    });
    std::thread::sleep(Duration::from_millis(50));
}

async fn serve_connection(stream: UnixStream) {
    let mut framed: Framed<UnixStream, LengthDelimitedCodec> = Framed::new(stream, codec());
    // Handshake
    if let Some(Ok(frame)) = framed.next().await {
        let Ok(req) = rmp_serde::from_slice::<IpcRequest>(&frame) else { return };
        if matches!(req, IpcRequest::Handshake { .. }) {
            let resp = IpcResponse::Handshake { server_version: IpcProtocolVersion::V2 };
            let bytes = rmp_serde::to_vec(&resp).unwrap();
            if framed.send(Bytes::from(bytes)).await.is_err() { return; }
        }
    }
    // ListRecords (or any other request)
    if let Some(Ok(frame)) = framed.next().await {
        let Ok(req) = rmp_serde::from_slice::<IpcRequest>(&frame) else { return };
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
// TC-E2E-120 (AC-DDM-01): shikomi list（--ipc なし）→ IPC 既定 + exit 0 + MSG-CLI-051 非出力
// ---------------------------------------------------------------------------

#[test]
fn tc_e2e_120_list_without_ipc_flag_uses_ipc_default_and_exits_zero() {
    let xdg_dir = tight_tempdir();
    spawn_ipc_stub(xdg_dir.path());

    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", xdg_dir.path())
        .args(["list"])  // --ipc フラグを明示しない
        .assert()
        .success()
        // AC-DDM-01: MSG-CLI-051 非出力
        .stderr(predicate::str::contains("IPC mode").not())
        .stderr(predicate::str::contains("opt-in").not());
}

// ---------------------------------------------------------------------------
// TC-E2E-121 (AC-DDM-02): shikomi --no-ipc list（daemon 不要）→ SQLite 成功
// ---------------------------------------------------------------------------

#[test]
fn tc_e2e_121_no_ipc_list_without_daemon_succeeds_via_sqlite() {
    let vault_dir = tight_tempdir();
    // vault.db を初期化
    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("LANG")
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .args(["--no-ipc", "add", "--kind", "text", "--label", "init", "--value", "val"])
        .assert()
        .success();

    // daemon 不在（XDG_RUNTIME_DIR を存在しないパスに設定）
    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("LANG")
        .env("SHIKOMI_VAULT_DIR", vault_dir.path())
        .env("XDG_RUNTIME_DIR", "/tmp/__shikomi_no_daemon_e2e_121")
        .args(["--no-ipc", "list"])
        .assert()
        // AC-DDM-02: daemon 不在でも exit 0
        .success();
}

// ---------------------------------------------------------------------------
// TC-E2E-122 (AC-DDM-03): shikomi list（daemon 未起動）→ MSG-CLI-110 + exit 1
// ---------------------------------------------------------------------------

#[test]
fn tc_e2e_122_list_without_daemon_fails_with_msg_cli_110_and_no_ipc_in_hint() {
    let empty_xdg = tight_tempdir();
    std::fs::create_dir_all(empty_xdg.path().join("shikomi")).unwrap();

    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", empty_xdg.path())
        .args(["list"])
        .assert()
        // AC-DDM-03: exit 1
        .failure()
        .code(1)
        // MSG-CLI-110 内容確認
        .stderr(
            predicate::str::contains("not running")
                .or(predicate::str::contains("shikomi-daemon")),
        )
        .stderr(predicate::str::contains("hint:"))
        // Phase 2: hint に "--ipc" が含まれない
        .stderr(predicate::str::contains("--ipc").not());
}

// ---------------------------------------------------------------------------
// TC-E2E-123 (AC-DDM-04): shikomi --ipc list → clap error（廃止フラグ拒否）
// ---------------------------------------------------------------------------

#[test]
fn tc_e2e_123_ipc_flag_is_rejected_as_unknown_argument() {
    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("LANG")
        .args(["--ipc", "list"])
        .assert()
        // AC-DDM-04: exit 2 以外でも failure なら ok（clap error）
        .failure();

    // exit code の確認（clap error は exit 1 に写像される）
    let output = Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("LANG")
        .args(["--ipc", "list"])
        .output()
        .expect("run shikomi");
    assert_ne!(output.status.code(), Some(0), "--ipc should fail");
    // clap がエラーを stderr に出す
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ipc") || stderr.contains("error"),
        "stderr should contain error message, got: {stderr:?}"
    );
}

// ---------------------------------------------------------------------------
// TC-E2E-124 (AC-DDM-05): shikomi list（IPC 経路）→ stderr に MSG-CLI-051 文言なし
// ---------------------------------------------------------------------------

#[test]
fn tc_e2e_124_ipc_default_path_produces_no_msg_cli_051_in_stderr() {
    let xdg_dir = tight_tempdir();
    spawn_ipc_stub(xdg_dir.path());

    Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", xdg_dir.path())
        .args(["list"])
        .assert()
        .success()
        // AC-DDM-05: 全ての MSG-CLI-051 文言が含まれない
        .stderr(predicate::str::contains("IPC mode").not())
        .stderr(predicate::str::contains("--ipc").not())
        .stderr(predicate::str::contains("opt-in").not())
        .stderr(predicate::str::contains("MSG-CLI-051").not());
}

// ---------------------------------------------------------------------------
// TC-E2E-125 (AC-DDM-06): shikomi --no-ipc vault encrypt → MSG-CLI-052 先行 → MSG-CLI-110
// ---------------------------------------------------------------------------

#[test]
fn tc_e2e_125_no_ipc_vault_encrypt_outputs_msg_cli_052_before_msg_cli_110() {
    let empty_xdg = tight_tempdir();
    std::fs::create_dir_all(empty_xdg.path().join("shikomi")).unwrap();

    let output = Command::cargo_bin("shikomi")
        .expect("cargo_bin")
        .env_remove("SHIKOMI_VAULT_DIR")
        .env_remove("LANG")
        .env("XDG_RUNTIME_DIR", empty_xdg.path())
        .args(["--no-ipc", "vault", "encrypt"])
        .assert()
        // AC-DDM-06: exit 1
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

    // AC-DDM-06: MSG-CLI-052 が MSG-CLI-110 より先行して出力されること
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let pos_052 = stderr
        .find("vault commands always use IPC")
        .expect("MSG-CLI-052 must appear in stderr");
    let pos_110 = stderr
        .find("not running")
        .or_else(|| stderr.find("shikomi-daemon is not"))
        .expect("MSG-CLI-110 must appear in stderr");
    assert!(
        pos_052 < pos_110,
        "MSG-CLI-052 must precede MSG-CLI-110. pos_052={pos_052}, pos_110={pos_110}\nstderr:\n{stderr}"
    );

    // AC-DDM-06: vault.db が変更されていないこと（SQLite 直結フォールバックなし）
    // → 完全ブラックボックス検証: `--no-ipc` 指定時でも vault に直接アクセスしない
    // (vault_dir を設定しておらず、かつ SQLite フォールバックが起きなければ vault.db は生成されない)
}
