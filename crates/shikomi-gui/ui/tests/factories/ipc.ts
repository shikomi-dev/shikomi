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

/**
 * invalid_input エラー。
 * @param invalid_input_code 凍結 API フィールド（§2.2）。未指定時は unknown フォールバックを確認
 * @param message デバッグ用英語メッセージ（ユーザー表示禁止）
 */
export function errInvalidInput(
  invalid_input_code?: string,
  message = "invalid input",
): GUIError {
  return { kind: "invalid_input", invalid_input_code, message };
}

/** label_empty — ラベル空欄 */
export const errLabelEmpty = () => errInvalidInput("label_empty", "label must not be empty");
/** label_invalid — ラベル形式不正（Rust 凍結文字列: "invalid label format"） */
export const errLabelInvalid = () => errInvalidInput("label_invalid", "invalid label format");
/** value_empty — 値空欄 */
export const errValueEmpty = () => errInvalidInput("value_empty", "value must not be empty");
/** password_empty — パスワード空欄（encrypt/decrypt/unlock 共通） */
export const errPasswordEmpty = () => errInvalidInput("password_empty", "master password must not be empty");
/** confirmation_required — decrypt 確認チェックなし */
export const errConfirmationRequired = () => errInvalidInput("confirmation_required", "decrypt confirmation required");
/** id_invalid — 無効 UUID */
export const errIdInvalid = () => errInvalidInput("id_invalid", "invalid uuid");
/** hotkey_invalid — ホットキー形式不正 */
export const errHotkeyInvalid = () => errInvalidInput("hotkey_invalid", "invalid hotkey combo");
/** unknown invalid_input_code — フォールバック確認用 */
export const errInvalidInputUnknown = () => errInvalidInput("unknown_code", "some unknown error");

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
