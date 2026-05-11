/**
 * errors.ts ユニットテスト
 *
 * TC-GUI-UI-UT17 〜 UT23: ipc_code 別変換、vault_locked 制御フロー信号
 * TC-GUI-UI-UT28 〜 UT35: invalid_input_code 全7種変換（§6.2 凍結 API 契約）
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/ux-and-visual.md §6
 */

import { describe, it, expect } from "vitest";
import { resolveError, resolveMessage, isVaultLocked, isDisconnectError } from "./errors";
import * as factory from "@tests/factories/ipc";

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT17: daemon_not_running → 日本語メッセージ
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT17: errors.ts — daemon_not_running", () => {
  it("daemon_not_running → 「daemonが起動していません」メッセージ", () => {
    const result = resolveError(factory.errDaemonNotRunning());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toContain("daemon が起動していません");
      expect(result.text).toContain("shikomi start");
      // message フィールド内容が戻り値に混入していない
      expect(result.text).not.toContain("daemon is not running");
    }
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT18: ipc_code=hotkey_conflict, hotkey_conflict_entry フィールド使用
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT18: errors.ts — hotkey_conflict + hotkey_conflict_entry", () => {
  it("hotkey_conflict_entry あり → エントリ名を含む競合メッセージ", () => {
    const result = resolveError(factory.errHotkeyConflict("my-entry"));
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toContain("my-entry");
      // message フィールド内容が混入していない
      expect(result.text).not.toContain("hotkey conflict");
    }
  });

  it("hotkey_conflict_entry なし → 汎用競合メッセージ", () => {
    const err = { kind: "ipc_error", ipc_code: "hotkey_conflict", message: "hotkey conflict" };
    const result = resolveError(err as any);
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toContain("既に使用されています");
    }
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT19〜21: ipc_code=crypto, crypto_reason 別分岐
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT19: errors.ts — crypto wrong-password", () => {
  it("crypto_reason=wrong-password → 「パスワードが一致しません」", () => {
    const result = resolveError(factory.errCrypto("wrong-password"));
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("パスワードが一致しません");
      expect(result.text).not.toContain("crypto error");
    }
  });
});

describe("TC-GUI-UI-UT20: errors.ts — crypto weak-password", () => {
  it("crypto_reason=weak-password → 「パスワードが脆弱すぎます」", () => {
    const result = resolveError(factory.errCrypto("weak-password"));
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("パスワードが脆弱すぎます");
    }
  });
});

describe("TC-GUI-UI-UT21: errors.ts — crypto nonce-limit-exceeded", () => {
  it("crypto_reason=nonce-limit-exceeded → 「vaultの再暗号化が必要です」含む", () => {
    const result = resolveError(factory.errCrypto("nonce-limit-exceeded"));
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toContain("vault の再暗号化が必要です");
    }
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT22: ipc_code=backoff_active, wait_secs 補間
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT22: errors.ts — backoff_active wait_secs 補間", () => {
  it("wait_secs=30 → 「30秒後に再試行してください」含む", () => {
    const result = resolveError(factory.errBackoffActive(30));
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toContain("30");
      expect(result.text).toContain("秒後に再試行してください");
      // message フィールド内容が混入していない
      expect(result.text).not.toContain("backoff active");
    }
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT23: ipc_code=vault_locked → 制御フロー信号（VaultLockedSignal）
// ---------------------------------------------------------------------------
describe("TC-GUI-UI-UT23: errors.ts — vault_locked → VaultLockedSignal", () => {
  it("vault_locked → type=vault_locked の制御フロー信号（日本語メッセージでない）", () => {
    const result = resolveError(factory.errVaultLocked());
    expect(result.type).toBe("vault_locked");
    // message 型でないこと（日本語テキストが戻らない）
    expect("text" in result).toBe(false);
  });

  it("resolveMessage: vault_locked → null を返す", () => {
    const msg = resolveMessage(factory.errVaultLocked());
    expect(msg).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// isVaultLocked / isDisconnectError ヘルパー
// ---------------------------------------------------------------------------
describe("isVaultLocked", () => {
  it("vault_locked エラーで true", () => {
    expect(isVaultLocked(factory.errVaultLocked())).toBe(true);
  });

  it("daemon_not_running エラーで false", () => {
    expect(isVaultLocked(factory.errDaemonNotRunning())).toBe(false);
  });
});

describe("isDisconnectError", () => {
  it("daemon_not_running で true", () => {
    expect(isDisconnectError(factory.errDaemonNotRunning())).toBe(true);
  });

  it("not_connected で true", () => {
    expect(isDisconnectError(factory.errNotConnected())).toBe(true);
  });

  it("connection_failed で true", () => {
    expect(isDisconnectError(factory.errConnectionFailed())).toBe(true);
  });

  it("vault_locked で false", () => {
    expect(isDisconnectError(factory.errVaultLocked())).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// TC-GUI-UI-UT28〜UT35: invalid_input_code 全7種変換（§6.2 凍結 API 契約）
// ---------------------------------------------------------------------------

describe("TC-GUI-UI-UT28: errors.ts — invalid_input label_empty", () => {
  it("invalid_input_code=label_empty → 「ラベルを入力してください」", () => {
    const result = resolveError(factory.errLabelEmpty());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("ラベルを入力してください");
      // message フィールドの英語文言が混入していない
      expect(result.text).not.toContain("label must not be empty");
    }
  });
});

describe("TC-GUI-UI-UT29: errors.ts — invalid_input value_empty", () => {
  it("invalid_input_code=value_empty → 「値を入力してください」", () => {
    const result = resolveError(factory.errValueEmpty());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("値を入力してください");
      // かつて includes("empty") で label_empty と誤マッチしていた欠陥の回帰防止
      expect(result.text).not.toContain("ラベル");
      expect(result.text).not.toContain("value must not be empty");
    }
  });
});

describe("TC-GUI-UI-UT30: errors.ts — invalid_input password_empty", () => {
  it("invalid_input_code=password_empty → 「パスワードを入力してください」", () => {
    const result = resolveError(factory.errPasswordEmpty());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("パスワードを入力してください");
      // かつて includes("empty") で label_empty と誤マッチしていた欠陥の回帰防止
      expect(result.text).not.toContain("ラベル");
      expect(result.text).not.toContain("master password must not be empty");
    }
  });
});

describe("TC-GUI-UI-UT31: errors.ts — invalid_input confirmation_required", () => {
  it("invalid_input_code=confirmation_required → 「確認チェックボックスを有効にしてください」", () => {
    const result = resolveError(factory.errConfirmationRequired());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("確認チェックボックスを有効にしてください");
      expect(result.text).not.toContain("decrypt confirmation required");
    }
  });
});

describe("TC-GUI-UI-UT32: errors.ts — invalid_input id_invalid", () => {
  it("invalid_input_code=id_invalid → 「無効なエントリIDです」", () => {
    const result = resolveError(factory.errIdInvalid());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("無効なエントリIDです");
      expect(result.text).not.toContain("invalid uuid");
    }
  });
});

describe("TC-GUI-UI-UT33: errors.ts — invalid_input hotkey_invalid", () => {
  it("invalid_input_code=hotkey_invalid → 「ホットキーの形式が正しくありません」", () => {
    const result = resolveError(factory.errHotkeyInvalid());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("ホットキーの形式が正しくありません");
      expect(result.text).not.toContain("invalid hotkey combo");
    }
  });
});

describe("TC-GUI-UI-UT34: errors.ts — invalid_input unknown code フォールバック", () => {
  it("未知の invalid_input_code → 「入力内容に誤りがあります」（フォールバック）", () => {
    const result = resolveError(factory.errInvalidInputUnknown());
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("入力内容に誤りがあります");
    }
  });

  it("invalid_input_code 未設定（undefined）→ フォールバック", () => {
    // invalid_input_code が存在しない場合でもフォールバックが返る
    const result = resolveError(factory.errInvalidInput(undefined));
    expect(result.type).toBe("message");
    if (result.type === "message") {
      expect(result.text).toBe("入力内容に誤りがあります");
    }
  });
});

describe("TC-GUI-UI-UT35: errors.ts — invalid_input 全種で message フィールド非混入（REQ-UI-13）", () => {
  const cases = [
    ["label_empty", factory.errLabelEmpty()],
    ["value_empty", factory.errValueEmpty()],
    ["password_empty", factory.errPasswordEmpty()],
    ["confirmation_required", factory.errConfirmationRequired()],
    ["id_invalid", factory.errIdInvalid()],
    ["hotkey_invalid", factory.errHotkeyInvalid()],
    ["unknown_code", factory.errInvalidInputUnknown()],
  ] as const;

  for (const [code, err] of cases) {
    it(`${code}: GUIError.message の英語文言が戻り値に混入しない`, () => {
      const result = resolveError(err);
      expect(result.type).toBe("message");
      if (result.type === "message") {
        expect(result.text).not.toBe(err.message);
        expect(result.text).toMatch(/[぀-ヿ一-鿿]/);
      }
    });
  }
});

// ---------------------------------------------------------------------------
// ipc_code 全分岐カバレッジ確認（IT15 の UT 準拠分：message 混入がないこと）
// ---------------------------------------------------------------------------
describe("全 ipc_code 分岐で GUIError.message が戻り値に混入しないこと（REQ-UI-13）", () => {
  const cases: Array<[string, any]> = [
    ["daemon_not_running", factory.errDaemonNotRunning()],
    ["not_connected", factory.errNotConnected()],
    ["connection_failed", factory.errConnectionFailed()],
    ["hotkey_conflict+entry", factory.errHotkeyConflict("passwd-entry")],
    ["crypto/wrong-password", factory.errCrypto("wrong-password")],
    ["crypto/weak-password", factory.errCrypto("weak-password")],
    ["crypto/nonce-limit-exceeded", factory.errCrypto("nonce-limit-exceeded")],
    ["backoff_active", factory.errBackoffActive(30)],
    ["not_found", factory.errNotFound()],
    ["recovery_required", factory.errRecoveryRequired()],
  ];

  for (const [name, err] of cases) {
    it(`${name}: message フィールド内容が戻り値テキストに出ない`, () => {
      const result = resolveError(err);
      if (result.type === "message") {
        // message フィールドの値（英語）が result.text に混入していない
        expect(result.text).not.toBe(err.message);
        // 基本的に英語ではなく日本語メッセージが返る
        expect(result.text).toMatch(/[぀-ヿ一-鿿]/);
      }
    });
  }
});
