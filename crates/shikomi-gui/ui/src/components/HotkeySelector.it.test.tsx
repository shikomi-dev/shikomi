/**
 * HotkeySelector 結合テスト（MockIPC）
 *
 * TC-GUI-UI-IT07: Ctrl+Alt+3 選択 → assign_hotkey invoke 成功 → onChanged() 呼び出し
 * TC-GUI-UI-IT08: hotkey_conflict { hotkey_conflict_entry } → 競合エントリ名表示（message 不使用）
 * TC-GUI-UI-IT09: 「解除」ボタン → remove_hotkey invoke 成功 → onChanged() 呼び出し
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.6
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import HotkeySelector from "./HotkeySelector";
import * as factory from "@tests/factories/ipc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(cleanup);

describe("TC-GUI-UI-IT07: HotkeySelector — assign_hotkey 成功 → onChanged()", () => {
  it("Ctrl+Alt+3 を選択 → assign_hotkey invoke が正しい引数で呼ばれ onChanged が実行される", async () => {
    mockInvoke.mockResolvedValueOnce(undefined);

    const onChanged = vi.fn();
    const { getByRole } = render(() => (
      <HotkeySelector
        entryId="entry-001"
        currentHotkey={null}
        onChanged={onChanged}
      />
    ));

    const select = getByRole("combobox");
    fireEvent.change(select, { target: { value: "Ctrl+Alt+3" } });

    await vi.waitUntil(() => onChanged.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("assign_hotkey", {
      entryId: "entry-001",
      combo: "Ctrl+Alt+3",
    });
    expect(onChanged).toHaveBeenCalledTimes(1);
  });
});

describe("TC-GUI-UI-IT08: HotkeySelector — hotkey_conflict → 競合エントリ名インライン表示（errors.ts 経由）", () => {
  it("assign_hotkey → hotkey_conflict { hotkey_conflict_entry: 'passwd-entry' } → errors.ts 定義の文言で表示", async () => {
    const errObj = factory.errHotkeyConflict("passwd-entry");
    mockInvoke.mockRejectedValueOnce(errObj);

    const { container, queryByText } = render(() => (
      <HotkeySelector
        entryId="entry-001"
        currentHotkey={null}
        onChanged={vi.fn()}
      />
    ));

    const select = container.querySelector("select") as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "Ctrl+Alt+3" } });

    await vi.waitUntil(
      () => queryByText(/passwd-entry/) !== null,
      { timeout: 3000 },
    );

    // errors.ts §6.1 の定義文言と完全一致（独自メッセージ構築禁止）
    // errors.ts: 「選択したホットキーは別エントリ（${hotkey_conflict_entry}）に割り当て済みです」
    const errorEl = queryByText(/passwd-entry/);
    expect(errorEl).not.toBeNull();
    expect(errorEl!.textContent).toContain("選択したホットキーは別エントリ（passwd-entry）に割り当て済みです");

    // message フィールド（英語）が DOM に出ていない（REQ-UI-13）
    expect(container.textContent).not.toContain(errObj.message);
  });

  it("hotkey_conflict_entry なし → 「選択したホットキーは既に使用されています」（errors.ts フォールバック）", async () => {
    // hotkey_conflict_entry を省略したエラー
    const errObj = { kind: "ipc_error", ipc_code: "hotkey_conflict", message: "hotkey conflict" } as any;
    mockInvoke.mockRejectedValueOnce(errObj);

    const { container, queryByText } = render(() => (
      <HotkeySelector
        entryId="entry-001"
        currentHotkey={null}
        onChanged={vi.fn()}
      />
    ));

    const select = container.querySelector("select") as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "Ctrl+Alt+3" } });

    await vi.waitUntil(
      () => queryByText(/既に使用されています/) !== null,
      { timeout: 3000 },
    );

    expect(queryByText(/選択したホットキーは既に使用されています/)).not.toBeNull();
    expect(container.textContent).not.toContain("hotkey conflict");
  });
});

describe("TC-GUI-UI-IT09: HotkeySelector — remove_hotkey 成功 → onChanged()", () => {
  it("「解除」ボタン押下 → remove_hotkey invoke が呼ばれ onChanged が実行される", async () => {
    // assign_hotkey 成功でホットキーを設定した状態を作る
    mockInvoke.mockResolvedValueOnce(undefined); // assign_hotkey 成功

    const onChanged = vi.fn();
    const { container } = render(() => (
      <HotkeySelector
        entryId="entry-001"
        currentHotkey="Ctrl+Alt+3"
        onChanged={onChanged}
      />
    ));

    // 解除ボタンが表示されているはず（currentHotkey が設定済み）
    const removeBtn = container.querySelector("button");
    expect(removeBtn).toBeDefined();
    expect(removeBtn!.textContent).toContain("解除");

    mockInvoke.mockReset();
    mockInvoke.mockResolvedValueOnce(undefined); // remove_hotkey 成功

    fireEvent.click(removeBtn!);

    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("remove_hotkey", {
      entryId: "entry-001",
    });
    expect(onChanged).toHaveBeenCalled();
  });
});
