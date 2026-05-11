/**
 * VaultStatusBanner ユニットテスト
 *
 * TC-GUI-UI-UT01〜UT04: vault mode 別ラベル表示
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.3
 */

import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@solidjs/testing-library";
import VaultStatusBanner from "./VaultStatusBanner";

afterEach(cleanup);

describe("TC-GUI-UI-UT01: VaultStatusBanner — plaintext", () => {
  it("mode=plaintext → 「[平文]」テキスト表示", () => {
    const { getByText } = render(() => <VaultStatusBanner mode="plaintext" />);
    expect(getByText("[平文]")).toBeDefined();
  });
});

describe("TC-GUI-UI-UT02: VaultStatusBanner — encrypted_locked", () => {
  it("mode=encrypted_locked → 「[暗号化済・ロック中]」テキスト表示", () => {
    const { getByText } = render(() => (
      <VaultStatusBanner mode="encrypted_locked" />
    ));
    expect(getByText("[暗号化済・ロック中]")).toBeDefined();
  });
});

describe("TC-GUI-UI-UT03: VaultStatusBanner — encrypted_unlocked", () => {
  it("mode=encrypted_unlocked → 「[暗号化済・解除済]」テキスト表示", () => {
    const { getByText } = render(() => (
      <VaultStatusBanner mode="encrypted_unlocked" />
    ));
    expect(getByText("[暗号化済・解除済]")).toBeDefined();
  });
});

describe("TC-GUI-UI-UT04: VaultStatusBanner — unknown", () => {
  it("mode=unknown → 「[不明]」テキスト表示", () => {
    const { getByText } = render(() => <VaultStatusBanner mode="unknown" />);
    expect(getByText("[不明]")).toBeDefined();
  });
});
