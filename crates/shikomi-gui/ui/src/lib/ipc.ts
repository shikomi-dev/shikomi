/**
 * Tauri Commands の型付き invoke wrapper。
 *
 * 各 Tauri Command のシグネチャを型付きで定義し、
 * コンポーネントが直接 `window.__TAURI__.invoke` を呼ぶ実装を禁止する。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/basic-design.md §3
 */

import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// ipc-client 型定義（Sub-B の Tauri Command 戻り値と一致させる）
// ---------------------------------------------------------------------------

export interface RecordSummary {
  id: string;
  label: string;
  kind: "text" | "secret";
  hotkey: string | null;
}

export interface ListEntriesResult {
  entries: RecordSummary[];
  vault_mode: VaultMode;
}

export type VaultMode =
  | "plaintext"
  | "encrypted_locked"
  | "encrypted_unlocked"
  | "unknown";

export interface VaultStatusResult {
  vault_mode: VaultMode;
}

export interface EncryptVaultResult {
  phrases: string[];
}

// ---------------------------------------------------------------------------
// GUIError — Sub-B ipc-client detailed-design.md §2.3 凍結 API 契約
// ---------------------------------------------------------------------------

export interface GUIError {
  kind: string;
  ipc_code?: string;
  message: string;
  wait_secs?: number;
  crypto_reason?: string;
  hotkey_conflict_entry?: string;
}

// ---------------------------------------------------------------------------
// invoke wrappers
// ---------------------------------------------------------------------------

export function listEntries(): Promise<ListEntriesResult> {
  return invoke<ListEntriesResult>("list_entries");
}

export function getVaultStatus(): Promise<VaultStatusResult> {
  return invoke<VaultStatusResult>("get_vault_status");
}

export function addEntry(
  label: string,
  value: string,
  kind: "text" | "secret",
  hotkey: string | null,
): Promise<RecordSummary> {
  return invoke<RecordSummary>("add_entry", { label, value, kind, hotkey });
}

export function updateEntry(
  id: string,
  label: string | null,
  value: string | null,
): Promise<RecordSummary> {
  return invoke<RecordSummary>("update_entry", { id, label, value });
}

export function deleteEntry(id: string): Promise<void> {
  return invoke<void>("delete_entry", { id });
}

export function assignHotkey(entryId: string, combo: string): Promise<void> {
  return invoke<void>("assign_hotkey", { entryId, combo });
}

export function removeHotkey(entryId: string): Promise<void> {
  return invoke<void>("remove_hotkey", { entryId });
}

export function encryptVault(password: string): Promise<EncryptVaultResult> {
  return invoke<EncryptVaultResult>("encrypt_vault", { password });
}

export function decryptVault(
  password: string,
  confirmed: boolean,
): Promise<void> {
  return invoke<void>("decrypt_vault", { password, confirmed });
}

export function unlockVault(password: string): Promise<void> {
  return invoke<void>("unlock_vault", { password });
}
