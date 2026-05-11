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
import { handleCommandError, refreshEntries } from "../store/vault";
import { resolveMessage } from "../lib/errors";
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
    const submitFn = async () => {
      await addEntry(label(), value, kind(), null);
    };
    try {
      await submitFn();
      // 機密値を即破棄（R1-GUI-18）
      if (valueRef) valueRef.value = "";
      await refreshEntries();
      props.onSuccess();
    } catch (e) {
      if (valueRef) valueRef.value = "";
      const err = e as GUIError;
      if (handleCommandError(err, submitFn)) return;
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
    const submitFn = async () => {
      await updateEntry(id, newLabel, newValue);
    };

    try {
      await submitFn();
      if (valueRef) valueRef.value = "";
      await refreshEntries();
      props.onSuccess();
    } catch (e) {
      if (valueRef) valueRef.value = "";
      const err = e as GUIError;
      if (handleCommandError(err, submitFn)) return;
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

        {/* ホットキー（編集モードのみ） */}
        <Show when={props.mode === "edit" && props.entry}>
          <HotkeySelector
            entryId={props.entry!.id}
            currentHotkey={props.entry!.hotkey}
            onChanged={handleHotkeyChanged}
          />
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
