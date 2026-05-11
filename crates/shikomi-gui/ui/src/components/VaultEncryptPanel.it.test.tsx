/**
 * VaultEncryptPanel 結合テスト（MockIPC）
 *
 * TC-GUI-UI-UT24: invoke 後 DOM ref がゼロ化されること（REQ-UI-14, R1-GUI-18）
 * TC-GUI-UI-IT10: score ≥ 3 → encrypt_vault 成功 → onEncrypted(phrases) 呼び出し
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.7
 *          docs/features/shikomi-gui/ui/detailed-design/store-and-flows.md §4
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import VaultEncryptPanel from "./VaultEncryptPanel";
import * as factory from "@tests/factories/ipc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(cleanup);

// score 3 パスワード（実観測済 "correct horse" = score 3, zxcvbn v4.4）
const PASSWORD_SCORE_3 = "correct horse";

describe("TC-GUI-UI-UT24: VaultEncryptPanel — invoke 後 DOM ref ゼロ化（REQ-UI-14）", () => {
  it("encrypt_vault invoke 後にパスワード input の value が空になる", async () => {
    mockInvoke.mockResolvedValueOnce(factory.makeEncryptVaultResult());

    const { container } = render(() => (
      <VaultEncryptPanel onEncrypted={vi.fn()} />
    ));

    const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;

    // パスワード入力（DOM ref + previewPassword signal 両方更新）
    // まず DOM ref に直接値をセット
    Object.defineProperty(passwordInput, "value", {
      writable: true,
      value: PASSWORD_SCORE_3,
    });
    // previewPassword signal 経由でも score を更新（onInput イベント）
    fireEvent.input(passwordInput, { target: { value: PASSWORD_SCORE_3 } });

    // 「暗号化」ボタンをクリック
    const btn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    expect(btn).not.toBeDisabled();
    fireEvent.click(btn);

    // invoke が完了するまで待機
    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
    // マイクロタスクが完了するまで待機
    await Promise.resolve();

    // DOM ref の value が "" にゼロ化されていること（R1-GUI-18）
    expect(passwordInput.value).toBe("");
  });
});

describe("TC-GUI-UI-IT10: VaultEncryptPanel — encrypt_vault 成功 → onEncrypted(phrases)", () => {
  it("score ≥ 3 パスワード送信 → encrypt_vault invoke → onEncrypted(24語) 呼び出し", async () => {
    const expectedResult = factory.makeEncryptVaultResult(24);
    mockInvoke.mockResolvedValueOnce(expectedResult);

    const onEncrypted = vi.fn();
    const { container } = render(() => (
      <VaultEncryptPanel onEncrypted={onEncrypted} />
    ));

    const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;
    Object.defineProperty(passwordInput, "value", {
      writable: true,
      value: PASSWORD_SCORE_3,
    });
    fireEvent.input(passwordInput, { target: { value: PASSWORD_SCORE_3 } });

    const btn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    fireEvent.click(btn);

    await vi.waitUntil(() => onEncrypted.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("encrypt_vault", {
      password: PASSWORD_SCORE_3,
    });
    expect(onEncrypted).toHaveBeenCalledWith(expectedResult.phrases);
    expect(onEncrypted.mock.calls[0][0]).toHaveLength(24);
  });
});
