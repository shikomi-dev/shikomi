/**
 * EntryForm — エントリ追加 / 編集フォーム（REQ-UI-04, REQ-UI-05）。
 *
 * 機密値（value フィールド）は DOM ref 経由のみで保持し、
 * invoke 直後に "" 上書きして破棄する（R1-GUI-18）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.5
 */

import type { Component } from "solid-js";
import { createSignal, Show } from "solid-js";
import type { GUIError, RecordSummary } from "../lib/ipc";
import { addEntry, updateEntry } from "../lib/ipc";
import { handleVaultLocked, handleDisconnect, refreshEntries } from "../store/vault";
import { resolveMessage, isVaultLocked, isDisconnectError } from "../lib/errors";
import HotkeySelector from "./HotkeySelector";

interface Props {
  mode: "add" | "edit";
  entry?: RecordSummary;
  onSuccess: () => void;
  onCancel: () => void;
}

const EntryForm: Component<Props> = (props) => {
  // DOM ref — 機密値は signal に入れない（R1-GUI-18）
  let valueRef: HTMLInputElement | undefined;

  const [label, setLabel] = createSignal(props.entry?.label ?? "");
  const [kind, setKind] = createSignal<"text" | "secret">(
    props.entry?.kind ?? "secret",
  );
  const [showValue, setShowValue] = createSignal(false);
  const [labelError, setLabelError] = createSignal<string | null>(null);
  const [valueError, setValueError] = createSignal<string | null>(null);
  const [submitError, setSubmitError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);

  // add モードでは entry がまだ存在しないため、HotkeySelector に渡せる
  // entryId が無い。代わりに「保存時に同梱」する pending な選択を保持する。
  // edit モードでは HotkeySelector が直接 assignHotkey IPC を打つ。
  const [pendingHotkey, setPendingHotkey] = createSignal<string>("");

  // ホットキー変更後に一覧を更新するためのフラグ
  const handleHotkeyChanged = async () => {
    await refreshEntries();
  };

  const validate = (): boolean => {
    let valid = true;
    if (!label().trim()) {
      setLabelError("ラベルを入力してください");
      valid = false;
    } else {
      setLabelError(null);
    }
    if (props.mode === "add" && !valueRef?.value.trim()) {
      setValueError("値を入力してください");
      valid = false;
    } else {
      setValueError(null);
    }
    return valid;
  };

  const handleSubmitAdd = async () => {
    if (!validate()) return;
    setSubmitError(null);
    setLoading(true);
    const value = valueRef?.value ?? "";
    try {
      await addEntry(label(), value, kind(), pendingHotkey() || null);
      // 機密値を即破棄（R1-GUI-18）
      if (valueRef) valueRef.value = "";
      await refreshEntries();
      props.onSuccess();
    } catch (e) {
      // 機密値を即破棄（R1-GUI-18） — エラーパスでも遅延なく実施
      if (valueRef) valueRef.value = "";
      const err = e as GUIError;
      if (isVaultLocked(err)) {
        // vault_locked: 機密値を含むクロージャを pendingOperation に格納しない（REQ-UI-14）
        // 再試行は entries 更新のみに留め、フォーム再入力はユーザーに委ねる
        handleVaultLocked(refreshEntries);
        setSubmitError("アンロック後、エントリを再入力してください");
        return;
      }
      if (isDisconnectError(err)) {
        handleDisconnect(err.kind);
        return;
      }
      setSubmitError(resolveMessage(err) ?? "追加に失敗しました");
    } finally {
      setLoading(false);
    }
  };

  const handleSubmitEdit = async () => {
    if (!validate()) return;
    setSubmitError(null);
    setLoading(true);

    const newLabel =
      label() !== props.entry?.label ? label() : null;
    const newValue =
      valueRef?.value.trim() ? valueRef.value : null;

    // 変更なし → update_entry を呼ばずキャンセル扱い（ipc-client §3.3 Sub-C 契約）
    if (newLabel === null && newValue === null) {
      if (valueRef) valueRef.value = "";
      setLoading(false);
      props.onCancel();
      return;
    }

    const id = props.entry!.id;

    try {
      await updateEntry(id, newLabel, newValue);
      if (valueRef) valueRef.value = "";
      await refreshEntries();
      props.onSuccess();
    } catch (e) {
      // 機密値を即破棄（R1-GUI-18）
      if (valueRef) valueRef.value = "";
      const err = e as GUIError;
      if (isVaultLocked(err)) {
        // vault_locked: newValue（機密値）を含むクロージャを pendingOperation に格納しない（REQ-UI-14）
        handleVaultLocked(refreshEntries);
        setSubmitError("アンロック後、エントリを再入力してください");
        return;
      }
      if (isDisconnectError(err)) {
        handleDisconnect(err.kind);
        return;
      }
      setSubmitError(resolveMessage(err) ?? "保存に失敗しました");
    } finally {
      setLoading(false);
    }
  };

  const handleSubmit = () => {
    if (props.mode === "add") handleSubmitAdd();
    else handleSubmitEdit();
  };

  return (
    <div class="panel">
      <h2 class="panel-title">
        {props.mode === "add" ? "エントリを追加" : "エントリを編集"}
      </h2>
      <div class="entry-form">
        {/* ラベル */}
        <div class="field">
          <label class="field-label">ラベル *</label>
          <input
            class="field-input"
            type="text"
            value={label()}
            onInput={(e) => setLabel(e.currentTarget.value)}
          />
          {labelError() && (
            <div class="inline-error">{labelError()}</div>
          )}
        </div>

        {/* 値（DOM ref 経由、signal に入れない） */}
        <div class="field">
          <label class="field-label">
            {props.mode === "add" ? "値 *" : "値（変更する場合のみ入力）"}
          </label>
          <div class="field-value-row">
            <input
              ref={valueRef}
              class="field-input"
              type={showValue() ? "text" : "password"}
              autocomplete="off"
            />
            <button
              class="btn btn-secondary"
              type="button"
              onClick={() => setShowValue((v) => !v)}
            >
              {showValue() ? "隠す" : "表示"}
            </button>
          </div>
          {valueError() && (
            <div class="inline-error">{valueError()}</div>
          )}
        </div>

        {/* 種別 */}
        <div class="field">
          <label class="field-label">種別</label>
          <select
            class="field-select"
            style="width: auto;"
            value={kind()}
            onChange={(e) =>
              setKind(e.currentTarget.value as "text" | "secret")
            }
          >
            <option value="secret">シークレット</option>
            <option value="text">テキスト</option>
          </select>
        </div>

        {/* ホットキー: edit モードは HotkeySelector が直接 IPC、
           add モードは pendingHotkey signal で保持し addEntry に同梱 */}
        <Show when={props.mode === "edit" && props.entry}>
          <HotkeySelector
            entryId={props.entry!.id}
            currentHotkey={props.entry!.hotkey}
            onChanged={handleHotkeyChanged}
          />
        </Show>
        <Show when={props.mode === "add"}>
          <div class="hotkey-selector">
            <span class="field-label">ホットキー</span>
            <select
              class="field-select"
              value={pendingHotkey()}
              disabled={loading()}
              style="width: auto;"
              onChange={(e) => setPendingHotkey(e.currentTarget.value)}
            >
              <option value="">（未設定）</option>
              <option value="Ctrl+Alt+1">Ctrl+Alt+1</option>
              <option value="Ctrl+Alt+2">Ctrl+Alt+2</option>
              <option value="Ctrl+Alt+3">Ctrl+Alt+3</option>
              <option value="Ctrl+Alt+4">Ctrl+Alt+4</option>
              <option value="Ctrl+Alt+5">Ctrl+Alt+5</option>
              <option value="Ctrl+Alt+6">Ctrl+Alt+6</option>
              <option value="Ctrl+Alt+7">Ctrl+Alt+7</option>
              <option value="Ctrl+Alt+8">Ctrl+Alt+8</option>
              <option value="Ctrl+Alt+9">Ctrl+Alt+9</option>
            </select>
          </div>
        </Show>

        {submitError() && (
          <div class="inline-error">{submitError()}</div>
        )}

        <div class="form-actions">
          <button
            class="btn btn-secondary"
            disabled={loading()}
            onClick={props.onCancel}
          >
            キャンセル
          </button>
          <button
            class="btn btn-primary"
            disabled={loading()}
            onClick={handleSubmit}
          >
            {props.mode === "add" ? "追加" : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default EntryForm;
