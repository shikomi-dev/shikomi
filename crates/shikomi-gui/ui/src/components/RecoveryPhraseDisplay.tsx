/**
 * RecoveryPhraseDisplay — recovery 24 語表示オーバーレイ（REQ-UI-09）。
 *
 * 「転記完了」ボタン押下まで背面操作をブロックする。
 * コンポーネントのマウント解除後に phrases 参照が消える（R1-GUI-18）。
 * 親は onEncrypted(phrases) を受け取った直後に自身の変数を null 上書きする。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.11
 */

import type { Component } from "solid-js";
import { For } from "solid-js";

interface Props {
  phrases: string[];
  onConfirmed: () => void;
}

const RecoveryPhraseDisplay: Component<Props> = (props) => {
  return (
    <div class="overlay-backdrop">
      <div class="overlay-modal">
        <h2 class="overlay-title">リカバリフレーズを安全な場所に転記してください</h2>
        <p class="recovery-caution">
          このフレーズはここでしか確認できません。必ず安全な場所に保管してください。
        </p>

        <div class="recovery-grid">
          <For each={props.phrases}>
            {(word, i) => (
              <div class="recovery-word">
                <span class="recovery-num">{i() + 1}.</span>
                <span>{word}</span>
              </div>
            )}
          </For>
        </div>

        <div class="modal-actions">
          <button class="btn btn-primary" onClick={props.onConfirmed}>
            転記完了
          </button>
        </div>
      </div>
    </div>
  );
};

export default RecoveryPhraseDisplay;
