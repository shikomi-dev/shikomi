/**
 * EntryList ユニットテスト
 *
 * TC-GUI-UI-UT26: 種別 text/secret のラベル表示
 * TC-GUI-UI-UT27: ホットキーバッジ表示 / 未設定時は空欄
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.4
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup } from "@solidjs/testing-library";
import EntryList from "./EntryList";
import { makeEntry } from "@tests/factories/ipc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("../store/vault", () => ({
  handleCommandError: vi.fn().mockReturnValue(false),
  refreshEntries: vi.fn().mockResolvedValue(undefined),
}));

afterEach(cleanup);

describe("TC-GUI-UI-UT26: EntryList — 種別ラベル表示", () => {
  it("kind=text → 「テキスト」表示", () => {
    const entries = [makeEntry({ kind: "text", label: "テキストエントリ" })];
    const { getByText } = render(() => (
      <EntryList entries={entries} onEdit={vi.fn()} onAdd={vi.fn()} />
    ));
    expect(getByText("テキスト")).toBeDefined();
  });

  it("kind=secret → 「シークレット」表示", () => {
    const entries = [makeEntry({ kind: "secret", label: "シークレットエントリ" })];
    const { getByText } = render(() => (
      <EntryList entries={entries} onEdit={vi.fn()} onAdd={vi.fn()} />
    ));
    expect(getByText("シークレット")).toBeDefined();
  });
});

describe("TC-GUI-UI-UT27: EntryList — ホットキーバッジ表示", () => {
  it("hotkey 設定済み → バッジ表示", () => {
    const entries = [
      makeEntry({ hotkey: "Ctrl+Alt+3", label: "ホットキー付き" }),
    ];
    const { getByText } = render(() => (
      <EntryList entries={entries} onEdit={vi.fn()} onAdd={vi.fn()} />
    ));
    const badge = getByText("Ctrl+Alt+3");
    expect(badge).toBeDefined();
    expect(badge.classList.contains("hotkey-badge")).toBe(true);
  });

  it("hotkey 未設定 → バッジなし（─ 表示）", () => {
    const entries = [
      makeEntry({ hotkey: null, label: "ホットキーなし" }),
    ];
    const { container } = render(() => (
      <EntryList entries={entries} onEdit={vi.fn()} onAdd={vi.fn()} />
    ));
    // hotkey-badge が存在しない
    expect(container.querySelector(".hotkey-badge")).toBeNull();
    // ─ 記号が表示される
    expect(container.textContent).toContain("─");
  });
});
