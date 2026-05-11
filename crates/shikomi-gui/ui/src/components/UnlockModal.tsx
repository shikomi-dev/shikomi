/**
 * UnlockModal — vault_locked 時のアンロックオーバーレイ（REQ-UI-11）。
 *
 * マスターパスワードは DOM ref 経由のみで保持し、invoke 直後に破棄する（R1-GUI-18）。
 * backoff_active 時は wait_secs を表示しボタンを disabled にする。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.10
 */

import type { Component } from "solid-js";
import { createSignal } from "solid-js";
import type { GUIError } from "../lib/ipc";
import { unlockVault } from "../lib/ipc";
import { resolveMessage } from "../lib/errors";

interface Props {
  onUnlocked: () => void;
  onCancel: () => void;
}

const UnlockModal: Component<Props> = (props) => {
  let passwordRef: HTMLInputElement | undefined;

  const [loading, setLoading] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);
  const [backoffDisabled, setBackoffDisabled] = createSignal(false);

  const handleSubmit = async () => {
    setErrorMsg(null);
    setLoading(true);
    const password = passwordRef?.value ?? "";
    try {
      await unlockVault(password);
      // 機密値を即破棄（R1-GUI-18）
      if (passwordRef) passwordRef.value = "";
      props.onUnlocked();
    } catch (e) {
      if (passwordRef) passwordRef.value = "";
      const err = e as GUIError;
      if (err.kind === "ipc_error" && err.ipc_code === "backoff_active") {
        // errors.ts 経由でメッセージ取得。wait_secs はタイマー用途のみ（message パース禁止）
        const secs = err.wait_secs ?? 0;
        setErrorMsg(resolveMessage(err) ?? "試行回数の上限に達しました");
        setBackoffDisabled(true);
        setTimeout(() => setBackoffDisabled(false), secs * 1000);
      } else {
        setErrorMsg(resolveMessage(err) ?? "アンロックに失敗しました");
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="overlay-backdrop">
      <div class="overlay-modal">
        <h2 class="overlay-title">vault がロックされています</h2>

        <div class="field">
          <label class="field-label">マスターパスワード</label>
          <input
            ref={passwordRef}
            class="field-input"
            type="password"
            autocomplete="current-password"
          />
        </div>

        {errorMsg() && (
          <div class="inline-error">⚠ {errorMsg()}</div>
        )}

        <div class="modal-actions">
          <button
            class="btn btn-secondary"
            disabled={loading()}
            onClick={props.onCancel}
          >
            キャンセル
          </button>
          <button
            class="btn btn-primary"
            disabled={loading() || backoffDisabled()}
            onClick={handleSubmit}
          >
            アンロック
          </button>
        </div>
      </div>
    </div>
  );
};

export default UnlockModal;
