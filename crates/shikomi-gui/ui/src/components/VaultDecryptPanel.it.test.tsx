/**
 * VaultDecryptPanel 結合テスト（MockIPC）
 *
 * TC-GUI-UI-IT11: チェックボックス + パスワード入力 + 送信
 *                 → decrypt_vault(confirmed: true) 成功 → onDecrypted() 呼び出し
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.9
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import VaultDecryptPanel from "./VaultDecryptPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(cleanup);

describe("TC-GUI-UI-IT11: VaultDecryptPanel — チェックボックス + 送信 → onDecrypted()", () => {
  it("チェックボックスチェック + パスワード入力 + 送信 → decrypt_vault(confirmed=true) invoke → onDecrypted() 呼び出し", async () => {
    mockInvoke.mockResolvedValueOnce(undefined);

    const onDecrypted = vi.fn();
    const { container, getByRole } = render(() => (
      <VaultDecryptPanel onDecrypted={onDecrypted} />
    ));

    // パスワード入力
    const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;
    Object.defineProperty(passwordInput, "value", { writable: true, value: "myMasterPass" });
    fireEvent.input(passwordInput, { target: { value: "myMasterPass" } });

    // チェックボックスをチェック
    const checkbox = getByRole("checkbox");
    fireEvent.click(checkbox);

    // 「解除する」ボタンが enabled になる
    const btn = getByRole("button", { name: "解除する" });
    expect(btn).not.toBeDisabled();

    // 送信
    fireEvent.click(btn);

    await vi.waitUntil(() => onDecrypted.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("decrypt_vault", {
      password: "myMasterPass",
      confirmed: true,
    });
    expect(onDecrypted).toHaveBeenCalledTimes(1);
  });

  it("送信後 DOM ref がゼロ化されていること（REQ-UI-14）", async () => {
    mockInvoke.mockResolvedValueOnce(undefined);

    const { container, getByRole } = render(() => (
      <VaultDecryptPanel onDecrypted={vi.fn()} />
    ));

    const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;
    Object.defineProperty(passwordInput, "value", { writable: true, value: "myMasterPass" });

    const checkbox = getByRole("checkbox");
    fireEvent.click(checkbox);

    const btn = getByRole("button", { name: "解除する" });
    fireEvent.click(btn);

    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
    await Promise.resolve();

    expect(passwordInput.value).toBe("");
  });
});
