/**
 * RecoveryPhraseDisplay ユニットテスト
 *
 * TC-GUI-UI-UT15: 24語 props → 番号付きで全語表示
 * TC-GUI-UI-UT16: 「転記完了」ボタン押下 → onConfirmed() 呼び出し
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.11
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import RecoveryPhraseDisplay from "./RecoveryPhraseDisplay";

afterEach(cleanup);

const SAMPLE_PHRASES = Array.from({ length: 24 }, (_, i) => `word${i + 1}`);

describe("TC-GUI-UI-UT15: RecoveryPhraseDisplay — 24語 番号付き全表示（REQ-UI-09, REQ-UI-14）", () => {
  it("24語の phrases を番号付きで全て表示する", () => {
    const { getByText } = render(() => (
      <RecoveryPhraseDisplay phrases={SAMPLE_PHRASES} onConfirmed={vi.fn()} />
    ));
    // 全語が表示されていることを確認
    for (let i = 0; i < SAMPLE_PHRASES.length; i++) {
      // 番号付き表示: "1. word1", "2. word2", ...
      expect(getByText(`${i + 1}.`)).toBeDefined();
      expect(getByText(SAMPLE_PHRASES[i])).toBeDefined();
    }
  });

  it("24語すべて表示され、語の欠落がない（REQ-UI-14: コンポーネント内に phrases が存在する間は全語可視）", () => {
    const { container } = render(() => (
      <RecoveryPhraseDisplay phrases={SAMPLE_PHRASES} onConfirmed={vi.fn()} />
    ));
    // recovery-grid 内に 24 アイテム存在
    const words = container.querySelectorAll(".recovery-word");
    expect(words.length).toBe(24);
  });
});

describe("TC-GUI-UI-UT16: RecoveryPhraseDisplay — 「転記完了」ボタン → onConfirmed()", () => {
  it("「転記完了」ボタン押下 → onConfirmed が 1 度呼ばれる", () => {
    const onConfirmed = vi.fn();
    const { getByRole } = render(() => (
      <RecoveryPhraseDisplay phrases={SAMPLE_PHRASES} onConfirmed={onConfirmed} />
    ));
    const btn = getByRole("button", { name: "転記完了" });
    fireEvent.click(btn);
    expect(onConfirmed).toHaveBeenCalledTimes(1);
  });
});
