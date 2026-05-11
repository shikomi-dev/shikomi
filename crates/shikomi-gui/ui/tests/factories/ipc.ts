/**
 * IPC Factory — Sub-B 凍結 API 契約（ipc-client/detailed-design.md §2.3）に基づく
 * 型付き factory 関数群。
 *
 * assumed mock 禁止: factory 経由で生成した値のみを MockIPC stub に渡すこと。
 * インラインオブジェクトリテラル直書きは却下対象。
 */

import type { GUIError, ListEntriesResult, RecordSummary, VaultMode, EncryptVaultResult } from "../../src/lib/ipc";

// ---------------------------------------------------------------------------
// GUIError factory
// ---------------------------------------------------------------------------

/** daemon_not_running エラー */
export function errDaemonNotRunning(): GUIError {
  return { kind: "daemon_not_running", message: "daemon is not running" };
}

/** not_connected エラー */
export function errNotConnected(): GUIError {
  return { kind: "not_connected", message: "not connected" };
}

/** connection_failed エラー */
export function errConnectionFailed(): GUIError {
  return { kind: "connection_failed", message: "connection failed" };
}

/** vault_locked エラー */
export function errVaultLocked(): GUIError {
  return { kind: "ipc_error", ipc_code: "vault_locked", message: "vault is locked" };
}

/**
 * hotkey_conflict エラー。
 * @param hotkey_conflict_entry 競合エントリ名（凍結 API フィールド）
 */
export function errHotkeyConflict(hotkey_conflict_entry: string): GUIError {
  return {
    kind: "ipc_error",
    ipc_code: "hotkey_conflict",
    message: "hotkey conflict",
    hotkey_conflict_entry,
  };
}

/**
 * crypto エラー。
 * @param crypto_reason 暗号化理由（凍結 API フィールド）
 */
export function errCrypto(crypto_reason: "wrong-password" | "weak-password" | "nonce-limit-exceeded" | string): GUIError {
  return {
    kind: "ipc_error",
    ipc_code: "crypto",
    message: "crypto error",
    crypto_reason,
  };
}

/**
 * backoff_active エラー。
 * @param wait_secs 待機秒数（凍結 API フィールド）
 */
export function errBackoffActive(wait_secs: number): GUIError {
  return {
    kind: "ipc_error",
    ipc_code: "backoff_active",
    message: "backoff active",
    wait_secs,
  };
}

/** not_found エラー */
export function errNotFound(): GUIError {
  return { kind: "ipc_error", ipc_code: "not_found", message: "not found" };
}

/** invalid_input エラー */
export function errInvalidInput(message: string): GUIError {
  return { kind: "invalid_input", message };
}

/** recovery_required エラー */
export function errRecoveryRequired(): GUIError {
  return { kind: "ipc_error", ipc_code: "recovery_required", message: "recovery required" };
}

// ---------------------------------------------------------------------------
// RecordSummary factory
// ---------------------------------------------------------------------------

let _entrySeq = 1;

/** RecordSummary factory */
export function makeEntry(overrides: Partial<RecordSummary> = {}): RecordSummary {
  const id = `entry-${_entrySeq++}`;
  return {
    id,
    label: `ラベル${id}`,
    kind: "secret",
    hotkey: null,
    ...overrides,
  };
}

export function resetEntrySeq(): void {
  _entrySeq = 1;
}

// ---------------------------------------------------------------------------
// ListEntriesResult factory
// ---------------------------------------------------------------------------

export function makeListEntriesResult(
  entries: RecordSummary[] = [],
  vault_mode: VaultMode = "plaintext",
): ListEntriesResult {
  return { entries, vault_mode };
}

// ---------------------------------------------------------------------------
// EncryptVaultResult factory
// ---------------------------------------------------------------------------

export function makeEncryptVaultResult(phraseCount = 24): EncryptVaultResult {
  const phrases = Array.from({ length: phraseCount }, (_, i) => `word${i + 1}`);
  return { phrases };
}
