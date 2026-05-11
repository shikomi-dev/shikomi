/**
 * VaultDecryptPanel ユニットテスト
 *
 * TC-GUI-UI-UT10: チェックボックス未チェック → 「解除する」ボタン disabled
 * TC-GUI-UI-UT11: チェックボックスチェック後 → 「解除する」ボタン enabled
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.9
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import VaultDecryptPanel from "./VaultDecryptPanel";

// decryptVault は invoke 経由。このテストでは送信まで行わないが
// モジュールレベルの @tauri-apps/api/core import エラーを防ぐためモック
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// store/vault は実ストアを使用（状態変化を起こさないテスト）
vi.mock("../store/vault", () => ({
  handleCommandError: vi.fn().mockReturnValue(false),
  refreshEntries: vi.fn().mockResolvedValue(undefined),
}));

afterEach(cleanup);

describe("TC-GUI-UI-UT10: VaultDecryptPanel — チェックボックス未チェック時 disabled", () => {
  it("初期状態（confirmed=false）→ 「解除する」ボタンが disabled", () => {
    const { getByRole } = render(() => (
      <VaultDecryptPanel onDecrypted={vi.fn()} />
    ));
    const btn = getByRole("button", { name: "解除する" });
    expect(btn).toBeDisabled();
  });
});

describe("TC-GUI-UI-UT11: VaultDecryptPanel — チェックボックスチェック後 enabled", () => {
  it("チェックボックス → checked 後は「解除する」ボタンが enabled", () => {
    const { getByRole } = render(() => (
      <VaultDecryptPanel onDecrypted={vi.fn()} />
    ));
    const checkbox = getByRole("checkbox");
    const btn = getByRole("button", { name: "解除する" });

    // 初期は disabled
    expect(btn).toBeDisabled();

    // チェックボックスにチェック
    fireEvent.click(checkbox);

    // enabled に変わる
    expect(btn).not.toBeDisabled();
  });
});
