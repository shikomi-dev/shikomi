//! IpcServer — listener / accept ループ / 接続ごとのタスク管理。

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::{IpcProtocolVersion, IpcRequest};
use shikomi_core::Vault;
use shikomi_infra::crypto::{AesGcmAeadAdapter, Argon2idAdapter, Bip39Pbkdf2Hkdf, Rng, ZxcvbnGate};
use shikomi_infra::persistence::vault_migration::VaultMigration;
use shikomi_infra::persistence::{SqliteVaultRepository, VaultRepository};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinSet;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::backoff::UnlockBackoff;
use crate::cache::VekCache;
use crate::error::ServerError;
use crate::ipc::v2_handler::{dispatch_v2, ClientState, V2Context};
use crate::ipc::{framing, handshake};
use crate::permission::peer_credential;

use super::transport::ListenerEnum;

/// graceful shutdown の in-flight 待機タイムアウト。
const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);

// -------------------------------------------------------------------
// IpcServer
// -------------------------------------------------------------------

/// IPC サーバ。listener から `accept` し、接続ごとにタスクを spawn する。
///
/// Sub-E (#43): `VaultMigration::new` が `&SqliteVaultRepository` 具体型を要求する
/// ため、本 struct のジェネリック `<R>` は撤去し具体型 `SqliteVaultRepository` 固定。
/// daemon は SqliteVaultRepository 一択 (cli-vault-commands/test では別経路でテスト)。
pub struct IpcServer {
    listener: Option<ListenerEnum>,
    repo: Arc<SqliteVaultRepository>,
    vault: Arc<Mutex<Vault>>,
    cache: VekCache,
    backoff: Arc<Mutex<UnlockBackoff>>,
}

impl IpcServer {
    /// IpcServer を構築する (Sub-E (#43): cache / backoff を注入)。
    #[must_use]
    pub fn new(
        listener: ListenerEnum,
        repo: Arc<SqliteVaultRepository>,
        vault: Arc<Mutex<Vault>>,
        cache: VekCache,
        backoff: Arc<Mutex<UnlockBackoff>>,
    ) -> Self {
        Self {
            listener: Some(listener),
            repo,
            vault,
            cache,
            backoff,
        }
    }

    /// shutdown 通知を受けるまで accept ループを実行する。
    ///
    /// shutdown 通知後は in-flight 接続が完了するまで最大 30 秒待機する。
    ///
    /// `shutdown` は `tokio::sync::watch::Receiver<bool>`。`true` への変更
    /// （または `Sender` の drop）で graceful shutdown を開始する。
    /// `Notify` ではなく `watch` を使うのは、シグナル到達と `notified()` 登録の
    /// race window で通知が消える BUG-DAEMON-IPC-002 を構造的に防ぐため。
    ///
    /// # Errors
    /// `ServerError::Accept` / `ServerError::Join`。
    pub async fn start_with_shutdown(
        &mut self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ServerError> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| ServerError::Accept(std::io::Error::other("listener missing")))?;
        let mut connections: JoinSet<()> = JoinSet::new();

        match listener {
            #[cfg(unix)]
            ListenerEnum::Unix { listener, .. } => {
                self.accept_loop_unix(listener, &mut connections, shutdown.clone())
                    .await?;
            }
            #[cfg(windows)]
            ListenerEnum::Windows {
                server, pipe_name, ..
            } => {
                self.accept_loop_windows(server, &pipe_name, &mut connections, shutdown.clone())
                    .await?;
            }
        }

        // graceful: in-flight 接続待機（タイムアウト 30 秒）
        match tokio::time::timeout(SHUTDOWN_GRACE, async {
            while connections.join_next().await.is_some() {}
        })
        .await
        {
            Ok(()) => {}
            Err(_) => {
                tracing::warn!(
                    target: "shikomi_daemon::ipc::server",
                    "in-flight requests did not complete within 30s; forcing shutdown"
                );
                connections.shutdown().await;
            }
        }

        tracing::info!(target: "shikomi_daemon::ipc::server", "server shutdown complete");
        Ok(())
    }

    // -----------------------------------------------------------------
    // Unix accept ループ
    // -----------------------------------------------------------------

    #[cfg(unix)]
    #[allow(clippy::too_many_lines)]
    async fn accept_loop_unix(
        &self,
        listener: tokio::net::UnixListener,
        connections: &mut JoinSet<()>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), ServerError> {
        // 既に shutdown=true なら即終了（signal が server 起動前に到達したケース）
        if *shutdown.borrow_and_update() {
            drop(listener);
            return Ok(());
        }
        loop {
            tokio::select! {
                // biased: shutdown を accept より優先して poll
                biased;
                changed = shutdown.changed() => {
                    // Ok(()) = `send(true)` 受信 / Err = Sender drop（fail-secure）
                    // どちらでも shutdown 経路に入る
                    let _ = changed;
                    tracing::info!(
                        target: "shikomi_daemon::ipc::server",
                        "stop accepting new connections"
                    );
                    drop(listener);
                    return Ok(());
                }
                accepted = listener.accept() => {
                    let (stream, _addr) = match accepted {
                        Ok(s) => s,
                        Err(err) => {
                            tracing::warn!(
                                target: "shikomi_daemon::ipc::server",
                                "accept failed: {err}"
                            );
                            continue;
                        }
                    };
                    if let Err(err) = peer_credential::verify(&stream) {
                        tracing::warn!(
                            target: "shikomi_daemon::permission",
                            "peer credential mismatch: {err}; closing connection"
                        );
                        drop(stream);
                        continue;
                    }
                    tracing::info!(target: "shikomi_daemon::ipc::server", "client connected");
                    let repo = Arc::clone(&self.repo);
                    let vault = Arc::clone(&self.vault);
                    let cache = self.cache.clone();
                    let backoff = Arc::clone(&self.backoff);
                    let shutdown_for_task = shutdown.clone();
                    connections.spawn(async move {
                        handle_connection(stream, repo, vault, cache, backoff, shutdown_for_task)
                            .await;
                    });
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Windows accept ループ
    // -----------------------------------------------------------------

    #[cfg(windows)]
    async fn accept_loop_windows(
        &self,
        first_server: tokio::net::windows::named_pipe::NamedPipeServer,
        pipe_name: &str,
        connections: &mut JoinSet<()>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), ServerError> {
        if *shutdown.borrow_and_update() {
            drop(first_server);
            return Ok(());
        }
        let mut current = first_server;
        loop {
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    let _ = changed;
                    tracing::info!(
                        target: "shikomi_daemon::ipc::server",
                        "stop accepting new connections"
                    );
                    drop(current);
                    return Ok(());
                }
                connect_result = current.connect() => {
                    // 後続インスタンスにも owner-only DACL を適用。unsafe は
                    // permission/windows_acl.rs に閉じる（CI grep TC-CI-019）。
                    let next = crate::permission::windows_acl::create_next_pipe_instance_owner_only(
                        pipe_name,
                    )
                    .map_err(ServerError::Accept)?;
                    let stream = std::mem::replace(&mut current, next);
                    if let Err(err) = connect_result {
                        tracing::warn!(
                            target: "shikomi_daemon::ipc::server",
                            "named pipe connect failed: {err}"
                        );
                        drop(stream);
                        continue;
                    }
                    if let Err(err) = peer_credential::verify(&stream) {
                        tracing::warn!(
                            target: "shikomi_daemon::permission",
                            "peer credential mismatch: {err}; closing connection"
                        );
                        drop(stream);
                        continue;
                    }
                    tracing::info!(target: "shikomi_daemon::ipc::server", "client connected");
                    let repo = Arc::clone(&self.repo);
                    let vault = Arc::clone(&self.vault);
                    let cache = self.cache.clone();
                    let backoff = Arc::clone(&self.backoff);
                    let shutdown_for_task = shutdown.clone();
                    connections.spawn(async move {
                        handle_connection(stream, repo, vault, cache, backoff, shutdown_for_task)
                            .await;
                    });
                }
            }
        }
    }
}

// -------------------------------------------------------------------
// 接続単位タスク
// -------------------------------------------------------------------

/// 接続ハンドラ。ハンドシェイク 1 往復 → リクエスト/レスポンス N 往復 → 切断。
///
/// Sub-E (#43): handshake 完了後 `ClientState::Handshake { version }` を構築し、
/// 各リクエストを `dispatch_v2` 経由で V1+V2 統合 dispatch する。`VaultMigration`
/// は `Argon2idAdapter` 等の無状態 adapter から per-connection で構築する
/// (cheap、各 adapter は zero-sized 相当)。
///
/// `shutdown` 受信時にも in-flight リクエストの応答送信は完了させる。
async fn handle_connection<S>(
    stream: S,
    repo: Arc<SqliteVaultRepository>,
    vault: Arc<Mutex<Vault>>,
    cache: VekCache,
    backoff: Arc<Mutex<UnlockBackoff>>,
    mut shutdown: watch::Receiver<bool>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut framed: Framed<S, LengthDelimitedCodec> = Framed::new(stream, framing::codec());

    if let Err(err) = handshake::negotiate(&mut framed).await {
        tracing::warn!(
            target: "shikomi_daemon::ipc::handshake",
            "handshake failed: {err}; closing connection"
        );
        return;
    }
    // handshake 完了 → ClientState::Handshake { version } 確定。
    // 既存 `negotiate` 実装は server_version (V2) との一致のみ Ok を返す設計のため、
    // ここでは V2 を仮定する (V1 client 互換は許可リスト方式 §C-28 で別経路、
    // negotiate 改修は後続 commit で対応)。
    let client_state = ClientState::Handshake {
        version: IpcProtocolVersion::current(),
    };

    // 既に shutdown=true なら handshake 完了直後に閉じる
    if *shutdown.borrow_and_update() {
        return;
    }
    loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                let _ = changed;
                tracing::info!(
                    target: "shikomi_daemon::ipc::server",
                    "shutdown received; closing connection"
                );
                return;
            }
            frame = framed.next() => {
                match frame {
                    None => {
                        // 通常切断
                        return;
                    }
                    Some(Err(err)) => {
                        tracing::warn!(
                            target: "shikomi_daemon::ipc::server",
                            "frame error: {err}; closing connection"
                        );
                        return;
                    }
                    Some(Ok(bytes)) => {
                        let request: IpcRequest = match rmp_serde::from_slice(&bytes) {
                            Ok(req) => req,
                            Err(err) => {
                                tracing::warn!(
                                    target: "shikomi_daemon::ipc::handler",
                                    "MessagePack decode failed: {err}; closing connection"
                                );
                                drop(bytes);
                                return;
                            }
                        };
                        // Sub-E: per-request に VaultMigration を構築 (無状態、cheap)。
                        // Sub-D `VaultMigration::new` は具体型 `&'a` を取るため、本関数の
                        // スコープでスタック上に adapter を構築 → `&` 借用を渡す形にする。
                        let kdf_pw = Argon2idAdapter::default();
                        let kdf_recovery = Bip39Pbkdf2Hkdf;
                        let aead = AesGcmAeadAdapter;
                        let rng = Rng;
                        let gate = ZxcvbnGate::default();
                        let repo_ref: &SqliteVaultRepository = Arc::as_ref(&repo);
                        let migration = VaultMigration::new(
                            repo_ref,
                            &kdf_pw,
                            &kdf_recovery,
                            &aead,
                            &rng,
                            &gate,
                        );

                        let ctx = V2Context {
                            repo: repo_ref,
                            vault: &vault,
                            cache: &cache,
                            backoff: &backoff,
                            migration: &migration,
                        };

                        let response = dispatch_v2(&ctx, client_state, request).await;
                        let response_bytes = match rmp_serde::to_vec(&response) {
                            Ok(b) => b,
                            Err(err) => {
                                tracing::warn!(
                                    target: "shikomi_daemon::ipc::handler",
                                    "MessagePack encode failed: {err}; closing connection"
                                );
                                return;
                            }
                        };
                        if let Err(err) = framed.send(Bytes::from(response_bytes)).await {
                            tracing::warn!(
                                target: "shikomi_daemon::ipc::server",
                                "frame send failed: {err}; closing connection"
                            );
                            return;
                        }
                    }
                }
            }
        }
    }
}
