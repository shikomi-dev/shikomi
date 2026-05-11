/**
 * VaultDecryptPanel — vault 暗号化解除パネル（REQ-UI-10）。
 *
 * 2ステップ確認（チェックボックス + ボタン）を要求する。
 * マスターパスワードは DOM ref 経由のみで保持し、invoke 直後に破棄する（R1-GUI-18）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.9
 */

import type { Component } from "solid-js";
import { createSignal } from "solid-js";
import type { GUIError } from "../lib/ipc";
import { decryptVault } from "../lib/ipc";
import { resolveMessage } from "../lib/errors";

interface Props {
  onDecrypted: () => void;
}

const VaultDecryptPanel: Component<Props> = (props) => {
  let passwordRef: HTMLInputElement | undefined;

  const [confirmed, setConfirmed] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  const handleSubmit = async () => {
    setErrorMsg(null);
    setLoading(true);
    const password = passwordRef?.value ?? "";
    try {
      await decryptVault(password, true);
      // 機密値を即破棄（R1-GUI-18）
      if (passwordRef) passwordRef.value = "";
      props.onDecrypted();
    } catch (e) {
      if (passwordRef) passwordRef.value = "";
      const err = e as GUIError;
      setErrorMsg(resolveMessage(err) ?? "解除に失敗しました");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="panel">
      <h2 class="panel-title">vault の暗号化を解除</h2>
      <div class="entry-form">
        <div class="field">
          <label class="field-label">マスターパスワード</label>
          <input
            ref={passwordRef}
            class="field-input"
            type="password"
            autocomplete="current-password"
          />
        </div>

        <label class="decrypt-checkbox-row">
          <input
            type="checkbox"
            checked={confirmed()}
            onChange={(e) => setConfirmed(e.currentTarget.checked)}
          />
          <span>
            vault の暗号化を解除します。登録済みのエントリが平文で保存されます。
          </span>
        </label>

        {errorMsg() && (
          <div class="inline-error">{errorMsg()}</div>
        )}

        <div class="form-actions">
          <button
            class="btn btn-danger"
            disabled={loading() || !confirmed()}
            onClick={handleSubmit}
          >
            解除する
          </button>
        </div>
      </div>
    </div>
  );
};

export default VaultDecryptPanel;
