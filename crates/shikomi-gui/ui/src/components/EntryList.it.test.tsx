/**
 * EntryList 結合テスト（MockIPC）
 *
 * TC-GUI-UI-IT06: 削除ボタン → 確認ダイアログ → 確認 → delete_entry → list_entries 再取得
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.4
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import EntryList from "./EntryList";
import { makeEntry } from "@tests/factories/ipc";
import { refreshEntries } from "../store/vault";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// vi.mock はホイストされるため変数参照不可。vi.fn() で直接定義する
vi.mock("../store/vault", () => ({
  handleCommandError: vi.fn().mockReturnValue(false),
  refreshEntries: vi.fn().mockResolvedValue(undefined),
}));

const mockInvoke = vi.mocked(invoke);
const mockRefreshEntries = vi.mocked(refreshEntries);

beforeEach(() => {
  mockInvoke.mockReset();
  mockRefreshEntries.mockReset();
  mockRefreshEntries.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("TC-GUI-UI-IT06: EntryList — 削除フロー（確認 → delete_entry → 一覧更新）", () => {
  it("削除ボタン → 確認ダイアログ → 「削除する」確認 → delete_entry invoke → refreshEntries 呼び出し", async () => {
    const entry = makeEntry({ label: "削除対象エントリ" });
    mockInvoke.mockResolvedValueOnce(undefined); // delete_entry

    const { getByText, queryByText } = render(() => (
      <EntryList entries={[entry]} onEdit={vi.fn()} onAdd={vi.fn()} />
    ));

    // 削除ボタンをクリック
    const deleteBtn = getByText("削除");
    fireEvent.click(deleteBtn);

    // 確認ダイアログが表示される
    expect(queryByText(/削除対象エントリ.*削除しますか/)).toBeDefined();

    // 「削除する」ボタンをクリック
    const confirmBtn = getByText("削除する");
    fireEvent.click(confirmBtn);

    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("delete_entry", { id: entry.id });
    expect(mockRefreshEntries).toHaveBeenCalled();
  });

  it("削除ボタン → キャンセル → delete_entry 未呼び出し", async () => {
    const entry = makeEntry({ label: "キャンセル対象エントリ" });

    const { getByText } = render(() => (
      <EntryList entries={[entry]} onEdit={vi.fn()} onAdd={vi.fn()} />
    ));

    fireEvent.click(getByText("削除"));
    fireEvent.click(getByText("キャンセル"));

    expect(mockInvoke).not.toHaveBeenCalledWith("delete_entry", expect.anything());
  });
});
