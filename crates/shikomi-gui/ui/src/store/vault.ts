/**
 * vault リアクティブストア。
 *
 * connectionStatus / entries / vaultMode / vaultLockPending を一元管理する。
 * 機密値（パスワード・recovery フレーズ）はここに格納しない（R1-GUI-18）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/store-and-flows.md §2〜4
 */

import { createSignal } from "solid-js";
import type { RecordSummary, VaultMode } from "../lib/ipc";
import { listEntries } from "../lib/ipc";
import type { GUIError } from "../lib/ipc";
import { isDisconnectError, isVaultLocked } from "../lib/errors";

// ---------------------------------------------------------------------------
// 状態
// ---------------------------------------------------------------------------

export type ConnectionStatus = "connecting" | "connected" | "disconnected";

const [connectionStatus, setConnectionStatus] =
  createSignal<ConnectionStatus>("connecting");

const [entries, setEntries] = createSignal<RecordSummary[]>([]);

const [vaultMode, setVaultMode] = createSignal<VaultMode>("unknown");

const [vaultLockPending, setVaultLockPending] = createSignal(false);

/** UnlockModal 解除後に再試行する操作。機密値は含めない（クロージャとして保持）。 */
const [pendingOperation, setPendingOperation] =
  createSignal<(() => Promise<void>) | null>(null);

/** DaemonConnectionPanel に渡す最後のエラー kind */
const [lastErrorKind, setLastErrorKind] = createSignal<string>("not_connected");

// ---------------------------------------------------------------------------
// 公開状態 accessor
// ---------------------------------------------------------------------------

export const vaultStore = {
  connectionStatus,
  entries,
  vaultMode,
  vaultLockPending,
  lastErrorKind,
} as const;

// ---------------------------------------------------------------------------
// ストア操作（§2.2）
// ---------------------------------------------------------------------------

/**
 * list_entries を呼び出してエントリと vault 状態を更新する。
 * 成功時に connectionStatus を "connected" に遷移させる。
 */
export async function refreshEntries(): Promise<void> {
  try {
    const result = await listEntries();
    setEntries(result.entries);
    setVaultMode(result.vault_mode);
    setConnectionStatus("connected");
  } catch (e) {
    const err = e as GUIError;
    if (isDisconnectError(err)) {
      handleDisconnect(err.kind);
    }
    // vault_locked はリスト取得では発生しない想定だが念のため無視
  }
}

/**
 * vault_locked エラーを受けて UnlockModal を表示し、
 * アンロック後に再試行する操作を保存する。
 *
 * @param operation アンロック後に再試行するコールバック
 */
export function handleVaultLocked(operation: () => Promise<void>): void {
  setPendingOperation(() => operation);
  setVaultLockPending(true);
}

/**
 * unlock_vault 成功後に呼び出す。
 * pendingOperation を再試行し、UnlockModal を閉じる。
 */
export async function handleUnlockSuccess(): Promise<void> {
  const op = pendingOperation();
  setVaultLockPending(false);
  setPendingOperation(null);
  if (op) {
    await op();
  }
  await refreshEntries();
}

/** UnlockModal をキャンセルして閉じる。 */
export function handleUnlockCancel(): void {
  setVaultLockPending(false);
  setPendingOperation(null);
}

/** 切断系エラーを受けて connectionStatus を "disconnected" に遷移させる。 */
export function handleDisconnect(errorKind: string): void {
  setLastErrorKind(errorKind);
  setConnectionStatus("disconnected");
}

/**
 * GUIError を受け取り、vault_locked 判定 → handleVaultLocked を透過処理する。
 * vault_locked でない場合は false を返し、呼び出し元がエラー表示を行う。
 *
 * @param err      Tauri Command から返ってきたエラー
 * @param retryFn  アンロック後に再試行する操作
 * @returns vault_locked として処理した場合 true
 */
export function handleCommandError(
  err: GUIError,
  retryFn: () => Promise<void>,
): boolean {
  if (isVaultLocked(err)) {
    handleVaultLocked(retryFn);
    return true;
  }
  if (isDisconnectError(err)) {
    handleDisconnect(err.kind);
    return true;
  }
  return false;
}
