/**
 * DaemonConnectionPanel — daemon 未接続時の案内パネル（REQ-UI-01）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.2
 */

import type { Component } from "solid-js";
import { resolveMessage } from "../lib/errors";
import type { GUIError } from "../lib/ipc";

interface Props {
  /** GUIError.kind 値 */
  errorKind: string;
  onRetry: () => void;
}

const DaemonConnectionPanel: Component<Props> = (props) => {
  const message = () => {
    const err: GUIError = {
      kind: props.errorKind,
      message: "",
    };
    return resolveMessage(err) ?? "接続エラーが発生しました。アプリを再起動してください";
  };

  return (
    <div class="daemon-panel">
      <p class="daemon-panel-msg">⚠ {message()}</p>
      <button class="btn btn-primary" onClick={props.onRetry}>
        再接続
      </button>
    </div>
  );
};

export default DaemonConnectionPanel;
