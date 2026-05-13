/**
 * VaultEncryptPanel — vault 暗号化オプトインパネル（REQ-UI-08）。
 *
 * マスターパスワードは DOM ref 経由のみで保持し、
 * invoke 直後に "" 上書きして破棄する（R1-GUI-18）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.7
 */

import type { Component } from "solid-js";
import { createSignal } from "solid-js";
import type { GUIError } from "../lib/ipc";
import { encryptVault } from "../lib/ipc";
import { resolveMessage } from "../lib/errors";
import PasswordStrengthMeter from "./PasswordStrengthMeter";

interface Props {
  onEncrypted: (phrases: string[]) => void;
}

const VaultEncryptPanel: Component<Props> = (props) => {
  // DOM ref — パスワードは signal に格納しない（R1-GUI-18）
  let passwordRef: HTMLInputElement | undefined;

  // PasswordStrengthMeter 用のリアルタイム評価値（パスワード本体は signal 非格納）
  const [previewPassword, setPreviewPassword] = createSignal("");
  const [score, setScore] = createSignal(0);
  const [loading, setLoading] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  const handleSubmit = async () => {
    const password = passwordRef?.value ?? "";
    setErrorMsg(null);
    setLoading(true);
    try {
      const result = await encryptVault(password);
      // 機密値を即破棄（R1-GUI-18）
      if (passwordRef) passwordRef.value = "";
      setPreviewPassword("");
      props.onEncrypted(result.phrases);
    } catch (e) {
      if (passwordRef) passwordRef.value = "";
      setPreviewPassword("");
      const err = e as GUIError;
      setErrorMsg(resolveMessage(err) ?? "暗号化に失敗しました");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="panel">
      <h2 class="panel-title">vault を暗号化</h2>
      <div class="entry-form">
        <div class="field">
          <label class="field-label">マスターパスワード</label>
          <input
            ref={passwordRef}
            class="field-input"
            type="password"
            autocomplete="new-password"
            onInput={(e) => setPreviewPassword(e.currentTarget.value)}
          />
        </div>

        <PasswordStrengthMeter
          password={previewPassword()}
          onScore={setScore}
        />

        {errorMsg() && (
          <div class="inline-error">{errorMsg()}</div>
        )}

        <div class="form-actions">
          <button
            class="btn btn-primary"
            disabled={loading() || score() < 3}
            onClick={handleSubmit}
          >
            暗号化
          </button>
        </div>
      </div>
    </div>
  );
};

export default VaultEncryptPanel;
