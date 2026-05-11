/**
 * VaultStatusBanner — 保護モードを常時表示するバナー（REQ-UI-03）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.3
 */

import type { Component } from "solid-js";
import type { VaultMode } from "../lib/ipc";

interface Props {
  mode: VaultMode;
}

const LABELS: Record<VaultMode, string> = {
  plaintext:            "[平文]",
  encrypted_locked:     "[暗号化済・ロック中]",
  encrypted_unlocked:   "[暗号化済・解除済]",
  unknown:              "[不明]",
};

const CSS_CLASSES: Record<VaultMode, string> = {
  plaintext:            "plaintext",
  encrypted_locked:     "encrypted-locked",
  encrypted_unlocked:   "encrypted-unlocked",
  unknown:              "unknown",
};

const VaultStatusBanner: Component<Props> = (props) => {
  return (
    <div class={`vault-status-banner ${CSS_CLASSES[props.mode]}`}>
      {LABELS[props.mode]}
    </div>
  );
};

export default VaultStatusBanner;
