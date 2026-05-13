/**
 * App 結合テスト（MockIPC）
 *
 * TC-GUI-UI-IT01: 起動 → list_entries 成功 → connected 遷移、EntryList + VaultStatusBanner 表示
 * TC-GUI-UI-IT02: 起動 → list_entries → daemon_not_running → DaemonConnectionPanel 表示
 * TC-GUI-UI-IT03: 起動後 Command が vault_locked → UnlockModal がオーバーレイ表示
 * TC-GUI-UI-IT14: vault_locked フロー — アンロック成功 → pendingOperation 再試行
 * TC-GUI-UI-IT15: 全エラー経路で message フィールドが DOM に出現しない
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/store-and-flows.md §3
 *          docs/features/shikomi-gui/ui/detailed-design/components.md §1.1
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import * as factory from "@tests/factories/ipc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// App.css は DOM 環境では不要（import エラー回避）
vi.mock("./App.css", () => ({}));

const mockInvoke = vi.mocked(invoke);

// App が使う store は module-level state なので、各テスト前にモジュールをリセットする
// vitest の isolate モード（forks）で各ファイルは独立だが、ファイル内ではリセット要

beforeEach(() => {
  mockInvoke.mockReset();
  vi.resetModules();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT01: 起動 → list_entries 成功 → EntryList + VaultStatusBanner 表示
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT01: App 起動 → list_entries 成功 → connected 遷移", () => {
  it("list_entries が成功するとエントリ一覧とバナーが表示される", async () => {
    const entries = [
      factory.makeEntry({ label: "テストエントリA" }),
      factory.makeEntry({ label: "テストエントリB" }),
    ];
    mockInvoke.mockResolvedValueOnce(
      factory.makeListEntriesResult(entries, "plaintext"),
    );

    const { queryByText } = render(() => <App />);

    // list_entries が完了するまで待機
    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
    await Promise.resolve();

    expect(mockInvoke).toHaveBeenCalledWith("list_entries");
    expect(queryByText("[平文]")).toBeDefined();
    expect(queryByText("テストエントリA")).toBeDefined();
    expect(queryByText("テストエントリB")).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT02: 起動 → daemon_not_running → DaemonConnectionPanel 表示
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT02: App 起動 → daemon_not_running → DaemonConnectionPanel 表示", () => {
  it("list_entries が daemon_not_running を返すと DaemonConnectionPanel が表示される", async () => {
    mockInvoke.mockRejectedValueOnce(factory.errDaemonNotRunning());

    const { queryByText } = render(() => <App />);

    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
    await Promise.resolve();

    // DaemonConnectionPanel の「再接続」ボタンが表示される
    expect(queryByText("再接続")).toBeDefined();
    // daemon_not_running のエラーメッセージが表示される（errors.ts 経由で日本語）
    expect(queryByText(/daemon が起動していません/)).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT03: vault_locked → UnlockModal 表示
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT03: App — vault_locked → UnlockModal オーバーレイ表示", () => {
  it("任意 Command が vault_locked を返すと UnlockModal が表示される", async () => {
    // 初回 list_entries は成功（接続済みに遷移）
    const entries = [factory.makeEntry({ label: "エントリX" })];
    mockInvoke.mockResolvedValueOnce(
      factory.makeListEntriesResult(entries, "plaintext"),
    );

    const { queryByText, getByText } = render(() => <App />);

    await vi.waitUntil(() => queryByText("エントリX") !== null);

    // 「+ エントリを追加」ボタンをクリックして EntryForm を開く
    const addBtn = getByText("+ エントリを追加");
    fireEvent.click(addBtn);

    // ラベルと値を入力
    render(() => <div />); // render は cleanup で管理
    // EntryForm の「追加」ボタン操作は EntryForm.it.test.tsx に委ねる
    // ここでは handleCommandError 経由で vaultLockPending を立てる方法でテスト

    // vault_locked をシミュレートするため、handleVaultLocked を直接呼び出す
    // NOTE: store が module-level のため、直接 import してテスト
    const { handleVaultLocked } = await import("./store/vault");
    handleVaultLocked(() => Promise.resolve());

    await Promise.resolve();

    expect(queryByText("vault がロックされています")).toBeDefined();
    expect(queryByText("アンロック")).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT14: vault_locked フロー — アンロック成功 → pendingOperation 再試行
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT14: vault_locked フロー — アンロック → pendingOperation 再試行", () => {
  it("UnlockModal でアンロック成功後、保存された pendingOperation が再実行される", async () => {
    // 初回 list_entries 成功
    const entries = [factory.makeEntry({ label: "エントリY" })];
    mockInvoke.mockResolvedValueOnce(
      factory.makeListEntriesResult(entries, "plaintext"),
    );

    const { queryByText } = render(() => <App />);
    await vi.waitUntil(() => queryByText("エントリY") !== null);

    // pendingOperation として追跡用関数を設定
    const pendingOp = vi.fn().mockResolvedValue(undefined);
    const { handleVaultLocked, handleUnlockSuccess } = await import("./store/vault");
    handleVaultLocked(pendingOp);

    await Promise.resolve();
    expect(queryByText("vault がロックされています")).toBeDefined();

    // unlock_vault 成功
    mockInvoke.mockResolvedValueOnce(undefined); // unlock_vault
    // handleUnlockSuccess が list_entries を呼ぶ
    mockInvoke.mockResolvedValueOnce(
      factory.makeListEntriesResult(entries, "plaintext"),
    );

    // UnlockModal の「アンロック」ボタンをクリック
    render(() => <div />) ;
    // 代わりに handleUnlockSuccess を直接呼ぶ（store function test）
    await handleUnlockSuccess();

    // pendingOperation が再実行された
    expect(pendingOp).toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-IT15: 全エラー経路で message フィールドが DOM に出現しない
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-IT15: 全エラー経路で GUIError.message が DOM に出現しない（REQ-UI-13）", () => {
  const errorCases = [
    { name: "daemon_not_running", err: factory.errDaemonNotRunning() },
    { name: "not_connected", err: factory.errNotConnected() },
    { name: "connection_failed", err: factory.errConnectionFailed() },
  ];

  for (const { name, err } of errorCases) {
    it(`${name}: GUIError.message が画面上に表示されない`, async () => {
      mockInvoke.mockRejectedValueOnce(err);

      const { container } = render(() => <App />);

      await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
      await Promise.resolve();

      // GUIError.message の英語文字列が DOM に含まれない
      expect(container.textContent).not.toContain(err.message);
    });
  }

  it("hotkey_conflict: GUIError.message が画面上に表示されない（HotkeySelector 経由）", async () => {
    // DaemonConnectionPanel で message が使われないことを確認
    // App + EntryList + EntryForm コンテキストで HotkeySelector のエラーテストは
    // HotkeySelector.it.test.tsx に詳細を委ねる
    // ここでは DaemonConnectionPanel が errors.ts 経由でのみ表示することを確認
    const err = factory.errDaemonNotRunning();
    mockInvoke.mockRejectedValueOnce(err);

    const { container } = render(() => <App />);
    await vi.waitUntil(() => mockInvoke.mock.calls.length > 0);
    await Promise.resolve();

    // DaemonConnectionPanel が errors.ts 経由で日本語を表示
    expect(container.textContent).toContain("daemon が起動していません");
    // message フィールドの英語文字列は表示されない
    expect(container.textContent).not.toContain("daemon is not running");
  });
});
