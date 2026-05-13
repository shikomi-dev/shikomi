/**
 * App — ルートコンポーネント（Sub-A 骨格を Sub-C で本実装に置き換え）。
 *
 * daemon 接続状態・vault 状態を store/vault.ts で管理し、
 * 子コンポーネントへ配布する（§1.1）。
 * 機密変数（recovery phrases）は onEncrypted 受取直後に null 上書きする（R1-GUI-18）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.1
 *          docs/features/shikomi-gui/ui/detailed-design/store-and-flows.md §2〜3
 */

import type { Component } from "solid-js";
import { createSignal, onMount, Show } from "solid-js";
import "./App.css";

import {
  vaultStore,
  refreshEntries,
  handleUnlockSuccess,
  handleUnlockCancel,
} from "./store/vault";
import VaultStatusBanner from "./components/VaultStatusBanner";
import DaemonConnectionPanel from "./components/DaemonConnectionPanel";
import EntryList from "./components/EntryList";
import EntryForm from "./components/EntryForm";
import VaultEncryptPanel from "./components/VaultEncryptPanel";
import VaultDecryptPanel from "./components/VaultDecryptPanel";
import UnlockModal from "./components/UnlockModal";
import RecoveryPhraseDisplay from "./components/RecoveryPhraseDisplay";

type ActiveView = "main" | "settings";
type FormMode =
  | { kind: "add" }
  | { kind: "edit"; id: string }
  | null;

const App: Component = () => {
  const [activeView, setActiveView] = createSignal<ActiveView>("main");
  const [formMode, setFormMode] = createSignal<FormMode>(null);

  // recovery phrases — signal 非格納、props 経由のみで子に渡す（R1-GUI-18）
  let recoveryPhrases: string[] | null = null;
  const [showRecovery, setShowRecovery] = createSignal(false);

  onMount(() => {
    refreshEntries();
  });

  const handleEncrypted = (phrases: string[]) => {
    recoveryPhrases = phrases;
    setShowRecovery(true);
  };

  const handleRecoveryConfirmed = () => {
    // 親が保持する phrases を即 null 上書きしてからコンポーネントをアンマウント（R1-GUI-18）
    recoveryPhrases = null;
    setShowRecovery(false);
    refreshEntries();
  };

  const handleDecrypted = () => {
    refreshEntries();
  };

  const handleFormSuccess = () => {
    setFormMode(null);
  };

  const resetTab = (view: ActiveView) => {
    setActiveView(view);
    setFormMode(null);
  };

  return (
    <div class="app-root">
      {/* 保護モードバナー（常時固定表示） */}
      <VaultStatusBanner mode={vaultStore.vaultMode()} />

      {/* daemon 未接続時 */}
      <Show when={vaultStore.connectionStatus() === "disconnected"}>
        <div class="app-content">
          <DaemonConnectionPanel
            errorKind={vaultStore.lastErrorKind()}
            onRetry={refreshEntries}
          />
        </div>
      </Show>

      {/* 接続済み */}
      <Show when={vaultStore.connectionStatus() === "connected"}>
        {/* タブ */}
        <div class="tab-bar">
          <button
            class={`tab-btn${activeView() === "main" ? " active" : ""}`}
            onClick={() => resetTab("main")}
          >
            メイン
          </button>
          <button
            class={`tab-btn${activeView() === "settings" ? " active" : ""}`}
            onClick={() => resetTab("settings")}
          >
            設定
          </button>
        </div>

        <div class="app-content">
          {/* メインタブ */}
          <Show when={activeView() === "main"}>
            <Show
              when={formMode()}
              fallback={
                <EntryList
                  entries={vaultStore.entries()}
                  onEdit={(id) => setFormMode({ kind: "edit", id })}
                  onAdd={() => setFormMode({ kind: "add" })}
                />
              }
            >
              {(mode) => {
                const m = mode();
                if (m.kind === "add") {
                  return (
                    <EntryForm
                      mode="add"
                      onSuccess={handleFormSuccess}
                      onCancel={() => setFormMode(null)}
                    />
                  );
                }
                return (
                  <EntryForm
                    mode="edit"
                    entry={vaultStore
                      .entries()
                      .find((e) => e.id === (m as { kind: "edit"; id: string }).id)}
                    onSuccess={handleFormSuccess}
                    onCancel={() => setFormMode(null)}
                  />
                );
              }}
            </Show>
          </Show>

          {/* 設定タブ */}
          <Show when={activeView() === "settings"}>
            <div
              style="display: flex; flex-direction: column; gap: var(--space-4);"
            >
              <Show
                when={
                  vaultStore.vaultMode() === "plaintext" ||
                  vaultStore.vaultMode() === "unknown"
                }
              >
                <VaultEncryptPanel onEncrypted={handleEncrypted} />
              </Show>
              <Show
                when={
                  vaultStore.vaultMode() === "encrypted_locked" ||
                  vaultStore.vaultMode() === "encrypted_unlocked"
                }
              >
                <VaultDecryptPanel onDecrypted={handleDecrypted} />
              </Show>
            </div>
          </Show>
        </div>
      </Show>

      {/* UnlockModal オーバーレイ（vault_locked 時） */}
      <Show when={vaultStore.vaultLockPending()}>
        <UnlockModal
          onUnlocked={handleUnlockSuccess}
          onCancel={handleUnlockCancel}
        />
      </Show>

      {/* RecoveryPhraseDisplay オーバーレイ（encrypt 成功後） */}
      <Show when={showRecovery() && recoveryPhrases !== null}>
        <RecoveryPhraseDisplay
          phrases={recoveryPhrases!}
          onConfirmed={handleRecoveryConfirmed}
        />
      </Show>
    </div>
  );
};

export default App;
