/**
 * PasswordStrengthMeter ユニットテスト
 *
 * TC-GUI-UI-UT05〜UT08: zxcvbn 実ライブラリ使用。score 0/2/3/4 境界値
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/ux-and-visual.md §5
 *
 * NOTE: zxcvbn 実観測 score:
 *   score 0: "" (空 = 0 扱い) / "a" / 単純1文字
 *   score 2: "password123" (実測 score=1) → "Pa$$w0rd" などで確認 → 実測で調整
 *   score 3: "correctHorseBatteryStaple" 相当
 *   score 4: "correctHorseBatteryStaple123!@#" 相当
 *
 * 境界値を実観測した入力値でコメントに記録する（test-design.md §3.2 要件）
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup } from "@solidjs/testing-library";
import PasswordStrengthMeter from "./PasswordStrengthMeter";
import zxcvbn from "zxcvbn";

afterEach(cleanup);

// 実観測: zxcvbn score の境界値確認（zxcvbn v4.4 実測値）
// score 0: "a"            → zxcvbn("a").score === 0
// score 2: "myPassword99" → zxcvbn("myPassword99").score === 2 (実観測済)
// score 3: "correct horse" → score === 3 (実観測済)
// score 4: "correctHorseBatteryStaple" → score === 4 (実観測済)

const PASSWORD_SCORE_0 = "a";
const PASSWORD_SCORE_2 = "myPassword99";
const PASSWORD_SCORE_3 = "correct horse";
const PASSWORD_SCORE_4 = "correctHorseBatteryStaple";

describe("TC-GUI-UI-UT05: PasswordStrengthMeter — score 0", () => {
  it("score 0 パスワード → 「非常に脆弱」ラベル + onScore(0) 呼び出し", () => {
    // 実観測でスコアが 0 であることを確認
    expect(zxcvbn(PASSWORD_SCORE_0).score).toBe(0);

    const onScore = vi.fn();
    const { getByText } = render(() => (
      <PasswordStrengthMeter password={PASSWORD_SCORE_0} onScore={onScore} />
    ));
    expect(getByText(/非常に脆弱/)).toBeDefined();
    expect(onScore).toHaveBeenCalledWith(0);
  });
});

describe("TC-GUI-UI-UT06: PasswordStrengthMeter — score 2（disabled 上限境界値）", () => {
  it("score 2 パスワード → 「普通」ラベル + onScore(2) 呼び出し", () => {
    // 実観測でスコアが 2 であることを確認
    const actualScore = zxcvbn(PASSWORD_SCORE_2).score;
    expect(actualScore).toBe(2);

    const onScore = vi.fn();
    const { getByText } = render(() => (
      <PasswordStrengthMeter password={PASSWORD_SCORE_2} onScore={onScore} />
    ));
    expect(getByText(/普通/)).toBeDefined();
    expect(onScore).toHaveBeenCalledWith(2);
  });
});

describe("TC-GUI-UI-UT07: PasswordStrengthMeter — score 3（enabled 下限境界値）", () => {
  it("score 3 パスワード → 「強い」ラベル + onScore(3) 呼び出し", () => {
    // 実観測でスコアが 3 であることを確認
    const actualScore = zxcvbn(PASSWORD_SCORE_3).score;
    expect(actualScore).toBe(3);

    const onScore = vi.fn();
    const { getByText } = render(() => (
      <PasswordStrengthMeter password={PASSWORD_SCORE_3} onScore={onScore} />
    ));
    expect(getByText(/強い/)).toBeDefined();
    expect(onScore).toHaveBeenCalledWith(3);
  });
});

describe("TC-GUI-UI-UT08: PasswordStrengthMeter — score 4", () => {
  it("score 4 パスワード → 「非常に強い」ラベル + onScore(4) 呼び出し", () => {
    // 実観測でスコアが 4 であることを確認
    const actualScore = zxcvbn(PASSWORD_SCORE_4).score;
    expect(actualScore).toBe(4);

    const onScore = vi.fn();
    const { getByText } = render(() => (
      <PasswordStrengthMeter password={PASSWORD_SCORE_4} onScore={onScore} />
    ));
    expect(getByText(/非常に強い/)).toBeDefined();
    expect(onScore).toHaveBeenCalledWith(4);
  });
});

describe("PasswordStrengthMeter — 空パスワード", () => {
  it("空パスワード → onScore(0) 呼び出し（strength meter 未表示）", () => {
    const onScore = vi.fn();
    render(() => (
      <PasswordStrengthMeter password="" onScore={onScore} />
    ));
    expect(onScore).toHaveBeenCalledWith(0);
  });
});
