/**
 * EntryForm 結合テスト（MockIPC）
 *
 * TC-GUI-UI-UT12: 追加モード、ラベル空文字送信 → バリデーションエラー表示・add_entry 未呼び出し
 * TC-GUI-UI-UT13: 追加モード、値空文字送信 → バリデーションエラー表示・add_entry 未呼び出し
 * TC-GUI-UI-UT14: 編集モード、変更なし送信 → update_entry 未呼び出し・onCancel() 呼び出し
 * TC-GUI-UI-IT04: 追加モード、ラベル+値入力送信 → add_entry invoke 成功 → onSuccess() 呼び出し
 * TC-GUI-UI-IT05: 編集モード、ラベル変更送信 → update_entry invoke 成功 → onSuccess() 呼び出し
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.5
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import EntryForm from "./EntryForm";
import { makeEntry, makeListEntriesResult } from "@tests/factories/ipc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("../store/vault", () => ({
  handleCommandError: vi.fn().mockReturnValue(false),
  refreshEntries: vi.fn().mockResolvedValue(undefined),
}));

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT12: 追加モード — ラベル空文字送信
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT12: EntryForm(add) — ラベル空文字 → バリデーションエラー、add_entry 未呼び出し", () => {
  it("ラベル未入力でフォーム送信 → 「ラベルを入力してください」エラー表示", async () => {
    const { getByRole, queryByText } = render(() => (
      <EntryForm mode="add" onSuccess={vi.fn()} onCancel={vi.fn()} />
    ));

    // 値のみ入力（ラベルは空のまま）
    // ラベルは空のまま送信
    const addBtn = getByRole("button", { name: "追加" });
    fireEvent.click(addBtn);

    // バリデーションエラーが表示される
    expect(queryByText(/ラベルを入力してください/)).toBeDefined();

    // add_entry invoke が呼ばれていない
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT13: 追加モード — 値空文字送信
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT13: EntryForm(add) — 値空文字 → バリデーションエラー、add_entry 未呼び出し", () => {
  it("ラベル入力・値未入力でフォーム送信 → 「値を入力してください」エラー表示", async () => {
    const { queryByText, container } = render(() => (
      <EntryForm mode="add" onSuccess={vi.fn()} onCancel={vi.fn()} />
    ));

    // ラベル入力
    const labelInput = container.querySelector("input[type=text]") as HTMLInputElement;
    fireEvent.input(labelInput, { target: { value: "テストエントリ" } });

    // 値は空のまま送信（value input は DOM ref なので空のまま）
    const addBtn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    fireEvent.click(addBtn);

    // バリデーションエラーが表示される
    expect(queryByText(/値を入力してください/)).toBeDefined();

    // add_entry invoke が呼ばれていない
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT14: 編集モード — 変更なし → Silent Skip
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT14: EntryForm(edit) — 変更なし → update_entry 未呼び出し、onCancel()", () => {
  it("初期値から変更せずに送信 → update_entry が呼ばれず onCancel が実行される", async () => {
    const entry = makeEntry({ label: "元ラベル", kind: "text" });
    const onCancel = vi.fn();

    const { container } = render(() => (
      <EntryForm
        mode="edit"
        entry={entry}
        onSuccess={vi.fn()}
        onCancel={onCancel}
      />
    ));

    // ラベルを変更しない（初期値のまま）
    // value フィールドは DOM ref なので空のまま（変更なし扱い）
    const saveBtn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    fireEvent.click(saveBtn);

    // update_entry が呼ばれていない
    expect(mockInvoke).not.toHaveBeenCalledWith("update_entry", expect.anything());
    // onCancel が呼ばれる
    await vi.waitUntil(() => onCancel.mock.calls.length > 0);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT04: 追加モード — 正常送信
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT04: EntryForm(add) — ラベル+値入力 → add_entry 成功 → onSuccess()", () => {
  it("ラベル・値入力後に送信 → add_entry invoke → onSuccess() 呼び出し", async () => {
    const newEntry = makeEntry({ label: "新エントリ", kind: "secret" });
    mockInvoke.mockResolvedValueOnce(newEntry); // add_entry
    mockInvoke.mockResolvedValueOnce(makeListEntriesResult([newEntry])); // list_entries

    const onSuccess = vi.fn();
    const { container } = render(() => (
      <EntryForm mode="add" onSuccess={onSuccess} onCancel={vi.fn()} />
    ));

    // ラベル入力
    const labelInput = container.querySelector("input[type=text]") as HTMLInputElement;
    fireEvent.input(labelInput, { target: { value: "新エントリ" } });

    // 値入力（DOM ref 経由）
    const valueInput = container.querySelector("input[type=password]") as HTMLInputElement;
    Object.defineProperty(valueInput, "value", { writable: true, value: "secret123" });

    // 送信
    const addBtn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    fireEvent.click(addBtn);

    await vi.waitUntil(() => onSuccess.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("add_entry", {
      label: "新エントリ",
      value: "secret123",
      kind: "secret",
      hotkey: null,
    });
    expect(onSuccess).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT05: 編集モード — ラベル変更
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT05: EntryForm(edit) — ラベル変更 → update_entry 成功 → onSuccess()", () => {
  it("ラベルを変更して送信 → update_entry invoke → onSuccess() 呼び出し", async () => {
    const entry = makeEntry({ label: "元ラベル", kind: "text" });
    const updatedEntry = { ...entry, label: "新ラベル" };
    mockInvoke.mockResolvedValueOnce(updatedEntry); // update_entry
    mockInvoke.mockResolvedValueOnce(makeListEntriesResult([updatedEntry])); // list_entries

    const onSuccess = vi.fn();
    const { container } = render(() => (
      <EntryForm
        mode="edit"
        entry={entry}
        onSuccess={onSuccess}
        onCancel={vi.fn()}
      />
    ));

    // ラベル変更
    const labelInput = container.querySelector("input[type=text]") as HTMLInputElement;
    fireEvent.input(labelInput, { target: { value: "新ラベル" } });

    const saveBtn = container.querySelector("button.btn-primary") as HTMLButtonElement;
    fireEvent.click(saveBtn);

    await vi.waitUntil(() => onSuccess.mock.calls.length > 0);

    expect(mockInvoke).toHaveBeenCalledWith("update_entry", {
      id: entry.id,
      label: "新ラベル",
      value: null,
    });
    expect(onSuccess).toHaveBeenCalledTimes(1);
  });
});
