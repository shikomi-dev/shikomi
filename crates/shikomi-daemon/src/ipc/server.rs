//! IpcServer — listener / accept ループ / 接続ごとのタスク管理。

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use shikomi_core::ipc::IpcRequest;
use shikomi_core::Vault;
use shikomi_infra::persistence::VaultRepository;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinSet;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::error::ServerError;
use crate::ipc::{framing, handler, handshake};
use crate::permission::peer_credential;

use super::transport::ListenerEnum;

/// graceful shutdown の in-flight 待機タイムアウト。
const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);

// -------------------------------------------------------------------
// IpcServer
// -------------------------------------------------------------------

/// IPC サーバ。listener から `accept` し、接続ごとにタスクを spawn する。
pub struct IpcServer<R: VaultRepository + Send + Sync + 'static> {
    listener: Option<ListenerEnum>,
    repo: Arc<R>,
    vault: Arc<Mutex<Vault>>,
}

impl<R: VaultRepository + Send + Sync + 'static> IpcServer<R> {
    /// IpcServer を構築する。
    #[must_use]
    pub fn new(listener: ListenerEnum, repo: Arc<R>, vault: Arc<Mutex<Vault>>) -> Self {
        Self {
            listener: Some(listener),
            repo,
            vault,
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
                    let shutdown_for_task = shutdown.clone();
                    connections.spawn(async move {
                        handle_connection(stream, repo, vault, shutdown_for_task).await;
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
                    let shutdown_for_task = shutdown.clone();
                    connections.spawn(async move {
                        handle_connection(stream, repo, vault, shutdown_for_task).await;
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
/// `shutdown` 受信時にも in-flight リクエストの応答送信は完了させる。
async fn handle_connection<S, R>(
    stream: S,
    repo: Arc<R>,
    vault: Arc<Mutex<Vault>>,
    mut shutdown: watch::Receiver<bool>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
    R: VaultRepository + Send + Sync + 'static,
{
    let mut framed: Framed<S, LengthDelimitedCodec> = Framed::new(stream, framing::codec());

    if let Err(err) = handshake::negotiate(&mut framed).await {
        tracing::warn!(
            target: "shikomi_daemon::ipc::handshake",
            "handshake failed: {err}; closing connection"
        );
        return;
    }

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
                        // ロック取得 → ハンドラ呼出 → ロック解放（応答送信前に解放）
                        let response = {
                            let mut vault_guard = vault.lock().await;
                            handler::handle_request(&*repo, &mut vault_guard, request)
                        };
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
