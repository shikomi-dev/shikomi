/**
 * VaultEncryptPanel ユニットテスト
 *
 * TC-GUI-UI-UT09: score < 3 の間は「暗号化」ボタンが disabled
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.7
 *          docs/features/shikomi-gui/ui/detailed-design/ux-and-visual.md §5
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import VaultEncryptPanel from "./VaultEncryptPanel";
import zxcvbn from "zxcvbn";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

afterEach(cleanup);

// 実観測 score 値（zxcvbn v4.4 実測済み、PasswordStrengthMeter.test.tsx と同じ境界値）
// score 2: "myPassword99"
// score 3: "correct horse"
const PASSWORD_SCORE_2 = "myPassword99";
const PASSWORD_SCORE_3 = "correct horse";

describe("TC-GUI-UI-UT09: VaultEncryptPanel — score < 3 で「暗号化」ボタン disabled", () => {
  it("初期状態（パスワード空）→「暗号化」ボタンが disabled", () => {
    const { getByRole } = render(() => (
      <VaultEncryptPanel onEncrypted={vi.fn()} />
    ));
    const btn = getByRole("button", { name: "暗号化" });
    expect(btn).toBeDisabled();
  });

  it("score=2 のパスワード入力 → 「暗号化」ボタンが disabled（disabled 上限境界）", () => {
    // 実観測確認
    expect(zxcvbn(PASSWORD_SCORE_2).score).toBe(2);

    const { getByRole, container } = render(() => (
      <VaultEncryptPanel onEncrypted={vi.fn()} />
    ));
    const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;
    fireEvent.input(passwordInput, { target: { value: PASSWORD_SCORE_2 } });

    const btn = getByRole("button", { name: "暗号化" });
    expect(btn).toBeDisabled();
  });

  it("score=3 のパスワード入力 → 「暗号化」ボタンが enabled（enabled 下限境界）", () => {
    // 実観測確認
    expect(zxcvbn(PASSWORD_SCORE_3).score).toBe(3);

    const { getByRole, container } = render(() => (
      <VaultEncryptPanel onEncrypted={vi.fn()} />
    ));
    const passwordInput = container.querySelector("input[type=password]") as HTMLInputElement;
    fireEvent.input(passwordInput, { target: { value: PASSWORD_SCORE_3 } });

    const btn = getByRole("button", { name: "暗号化" });
    expect(btn).not.toBeDisabled();
  });
});
