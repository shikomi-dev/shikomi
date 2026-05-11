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

const HotkeySelector: Component<Props> = (props) => {
  const [selected, setSelected] = createSignal<string>(
    props.currentHotkey ?? "",
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
      if (err.kind === "ipc_error" && err.ipc_code === "hotkey_conflict") {
        // errors.ts 経由でメッセージ取得（独自メッセージ構築禁止）
        setConflictMsg(resolveMessage(err) ?? "選択したホットキーは既に使用されています");
        setSelected(props.currentHotkey ?? "");
      }
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
