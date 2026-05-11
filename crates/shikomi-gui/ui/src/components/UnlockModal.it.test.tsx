/**
 * UnlockModal 結合テスト（MockIPC）
 *
 * TC-GUI-UI-UT25: unlock_vault invoke 後 DOM ref ゼロ化（REQ-UI-14）
 * TC-GUI-UI-IT12: wrong-password → 「パスワードが一致しません」インライン表示
 * TC-GUI-UI-IT13: backoff_active { wait_secs: 30 } → 「30秒後に再試行してください」+ ボタン disabled
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.10
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import UnlockModal from "./UnlockModal";
import * as factory from "@tests/factories/ipc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

async function typePasswordAndSubmit(container: HTMLElement, password: string) {
  const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;
  Object.defineProperty(passwordInput, "value", { writable: true, value: password });
  fireEvent.input(passwordInput, { target: { value: password } });
  const btn = container.querySelector("button.btn-primary") as HTMLButtonElement;
  fireEvent.click(btn);
  return { passwordInput, btn };
}

describe("TC-GUI-UI-UT25: UnlockModal — unlock_vault invoke 後 DOM ref ゼロ化（REQ-UI-14）", () => {
  it("unlock_vault 成功後にパスワード input の value が空になる", async () => {
    mockInvoke.mockResolvedValueOnce(undefined); // unlock_vault Ok

    const { container } = render(() => (
      <UnlockModal onUnlocked={vi.fn()} onCancel={vi.fn()} />
    ));

    const { passwordInput } = await typePasswordAndSubmit(container, "mySecretPass");

    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
    await Promise.resolve();

    // DOM ref がゼロ化されていること（R1-GUI-18）
    expect(passwordInput.value).toBe("");
  });
});

describe("TC-GUI-UI-IT12: UnlockModal — crypto/wrong-password → エラー表示 + 再入力可", () => {
  it("unlock_vault → crypto wrong-password → 「パスワードが一致しません」インライン表示", async () => {
    mockInvoke.mockRejectedValueOnce(factory.errCrypto("wrong-password"));

    const { container, queryByText } = render(() => (
      <UnlockModal onUnlocked={vi.fn()} onCancel={vi.fn()} />
    ));

    await typePasswordAndSubmit(container, "wrongPass");

    await vi.waitUntil(() => queryByText(/パスワードが一致しません/) !== null);

    expect(queryByText(/パスワードが一致しません/)).toBeDefined();

    // 再入力可能（ボタンが enabled に戻っている）
    const unlockBtn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    expect(unlockBtn).not.toBeDisabled();
  });
});

describe("TC-GUI-UI-IT13: UnlockModal — backoff_active wait_secs=30 → wait_secs 表示 + ボタン disabled", () => {
  it("unlock_vault → backoff_active { wait_secs: 30 } → 「30秒後に再試行」+ ボタン disabled", async () => {
    mockInvoke.mockRejectedValueOnce(factory.errBackoffActive(30));

    const { container, queryByText } = render(() => (
      <UnlockModal onUnlocked={vi.fn()} onCancel={vi.fn()} />
    ));

    const { btn } = await typePasswordAndSubmit(container, "anyPass");

    await vi.waitUntil(() => queryByText(/30秒後に再試行/) !== null);

    expect(queryByText(/30秒後に再試行してください/)).toBeDefined();

    // backoff_active 期間中はアンロックボタンが disabled
    expect(btn).toBeDisabled();
  });

  it("backoff_active: message フィールド（英語）が DOM に出現しない（REQ-UI-13）", async () => {
    const errObj = factory.errBackoffActive(30);
    mockInvoke.mockRejectedValueOnce(errObj);

    const { container, queryByText } = render(() => (
      <UnlockModal onUnlocked={vi.fn()} onCancel={vi.fn()} />
    ));

    await typePasswordAndSubmit(container, "anyPass");
    await vi.waitUntil(() => queryByText(/30秒後に再試行/) !== null);

    // GUIError.message の英語文字列が DOM に表示されていない
    expect(container.textContent).not.toContain(errObj.message);
  });
});
