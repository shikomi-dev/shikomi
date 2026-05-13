/**
 * PasswordStrengthMeter — zxcvbn 強度評価メーター（REQ-UI-08）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.8
 *          docs/features/shikomi-gui/ui/detailed-design/ux-and-visual.md §5
 */

import type { Component } from "solid-js";
import { createEffect, For, Show } from "solid-js";
import zxcvbn from "zxcvbn";

interface Props {
  password: string;
  onScore: (score: number) => void;
}

const SCORE_LABELS = ["非常に脆弱", "脆弱", "普通", "強い", "非常に強い"];
const SCORE_COLORS = [
  "var(--color-strength-0)",
  "var(--color-strength-1)",
  "var(--color-strength-2)",
  "var(--color-strength-3)",
  "var(--color-strength-4)",
];

const PasswordStrengthMeter: Component<Props> = (props) => {
  let lastResult = zxcvbn("");

  createEffect(() => {
    const pwd = props.password;
    if (!pwd) {
      props.onScore(0);
      return;
    }
    lastResult = zxcvbn(pwd);
    props.onScore(lastResult.score);
  });

  const result = () => (props.password ? zxcvbn(props.password) : null);
  const score = () => result()?.score ?? 0;

  return (
    <div class="strength-meter">
      <div class="strength-bar-track">
        <div
          class="strength-bar-fill"
          style={{
            width: `${((score() + 1) / 5) * 100}%`,
            background: SCORE_COLORS[score()],
          }}
        />
      </div>
      <Show when={props.password}>
        <span class="strength-label">
          {SCORE_LABELS[score()]} ({score()}/4)
        </span>
        <Show when={result()?.feedback.warning}>
          <span class="strength-warning">
            ⚠ {result()!.feedback.warning}
          </span>
        </Show>
        <Show when={(result()?.feedback.suggestions ?? []).length > 0}>
          <ul class="strength-suggestions">
            <For each={result()!.feedback.suggestions}>
              {(s) => <li>{s}</li>}
            </For>
          </ul>
        </Show>
      </Show>
    </div>
  );
};

export default PasswordStrengthMeter;
