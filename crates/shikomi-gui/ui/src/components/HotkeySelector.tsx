/**
 * HotkeySelector — Ctrl+Alt+[1-9] ホットキー割当セレクタ（REQ-UI-07）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.6
 */

import type { Component } from "solid-js";
import { createSignal, For } from "solid-js";
import type { GUIError } from "../lib/ipc";
import { assignHotkey, removeHotkey } from "../lib/ipc";
import { resolveMessage } from "../lib/errors";

interface Props {
  entryId: string;
  currentHotkey: string | null;
  onChanged: () => void;
}

const HOTKEY_OPTIONS = [1, 2, 3, 4, 5, 6, 7, 8, 9].map(
  (n) => `Ctrl+Alt+${n}`,
);

/**
 * vault が保存する正規化形式 (例 "alt+ctrl+2") を UI 表示形式
 * (例 "Ctrl+Alt+2") にそろえる。daemon 側 Hotkey::parse が修飾キー
 * 順序を再ソートするため、GUI から送る前後で表記が変わりうる。
 */
function normalizeForUi(combo: string | null | undefined): string {
  if (!combo) return "";
  const m = combo.match(/(\d)$/);
  if (!m) return "";
  const candidate = `Ctrl+Alt+${m[1]}`;
  return HOTKEY_OPTIONS.includes(candidate) ? candidate : "";
}

const HotkeySelector: Component<Props> = (props) => {
  const [selected, setSelected] = createSignal<string>(
    normalizeForUi(props.currentHotkey),
  );
  const [conflictMsg, setConflictMsg] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);

  const handleChange = async (combo: string) => {
    if (!combo) return;
    setConflictMsg(null);
    setLoading(true);
    try {
      await assignHotkey(props.entryId, combo);
      setSelected(combo);
      props.onChanged();
    } catch (e) {
      const err = e as GUIError;
      const msg = resolveMessage(err);
      // v0.1.x debug: resolveMessage の戻り値を信用せず、kind/ipc_code が
      // 明確な既知パターンでない場合は常に raw JSON を見せて根本原因特定可能にする。
      const isKnown =
        err?.kind === "vault_locked" ||
        err?.kind === "daemon_not_running" ||
        err?.kind === "not_connected" ||
        err?.kind === "connection_failed" ||
        (err?.kind === "ipc_error" && err.ipc_code === "hotkey_conflict");
      if (msg && isKnown) {
        setConflictMsg(msg);
      } else {
        try {
          setConflictMsg(`raw: ${JSON.stringify(e)}`);
        } catch {
          setConflictMsg(`raw: ${String(e)}`);
        }
      }
      setSelected(normalizeForUi(props.currentHotkey));
    } finally {
      setLoading(false);
    }
  };

  const handleRemove = async () => {
    setConflictMsg(null);
    setLoading(true);
    try {
      await removeHotkey(props.entryId);
      setSelected("");
      props.onChanged();
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="hotkey-selector">
      <span class="field-label">ホットキー</span>
      <div class="hotkey-selector-row">
        <select
          class="field-select"
          value={selected()}
          disabled={loading()}
          style="width: auto;"
          onChange={(e) => handleChange(e.currentTarget.value)}
        >
          <option value="">（未設定）</option>
          <For each={HOTKEY_OPTIONS}>
            {(opt) => <option value={opt}>{opt}</option>}
          </For>
        </select>
        {selected() && (
          <button
            class="btn btn-secondary"
            disabled={loading()}
            onClick={handleRemove}
          >
            解除
          </button>
        )}
      </div>
      {conflictMsg() && (
        <div class="inline-error">⚠ {conflictMsg()}</div>
      )}
    </div>
  );
};

export default HotkeySelector;
