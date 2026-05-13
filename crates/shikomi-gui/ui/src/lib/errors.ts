/**
 * GUIError.kind / ipc_code → 日本語メッセージ変換（単一責務モジュール）。
 *
 * 全コンポーネントはこのモジュール経由でのみエラーメッセージを取得する。
 * `message` フィールドをユーザーに表示してはならない。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/ux-and-visual.md §6
 *          docs/features/shikomi-gui/ui/basic-design.md §3.2
 *          ipc-client/detailed-design.md §2.3（凍結 API 契約）
 */

import type { GUIError } from "./ipc";

// ---------------------------------------------------------------------------
// ErrorResult — errors.ts の変換結果型
// ---------------------------------------------------------------------------

/** 日本語テキストとして表示する場合 */
export interface MessageResult {
  type: "message";
  text: string;
}

/** UnlockModal 表示に切り替える制御フロー信号 */
export interface VaultLockedSignal {
  type: "vault_locked";
}

export type ErrorResult = MessageResult | VaultLockedSignal;

// ---------------------------------------------------------------------------
// GUIError → ErrorResult 変換（§6 仕様に従う）
// ---------------------------------------------------------------------------

/**
 * GUIError を受け取り UI 表示用の ErrorResult を返す。
 * `message` フィールドを戻り値に含めない。
 */
export function resolveError(err: GUIError): ErrorResult {
  switch (err.kind) {
    case "daemon_not_running":
      return {
        type: "message",
        text: "daemon が起動していません。`shikomi daemon install` で自動起動を有効化するか、`systemctl --user start shikomi-daemon` で手動起動してください",
      };

    case "not_connected":
      return {
        type: "message",
        text: "接続が切断されました。アプリを再起動してください",
      };

    case "connection_failed":
      return {
        type: "message",
        text: "接続に失敗しました。アプリを再起動してください",
      };

    case "protocol_version_mismatch":
      return {
        type: "message",
        text: "shikomi のバージョンが一致しません。アプリとデーモンを同じバージョンに揃えてください",
      };

    case "ipc_error":
      return resolveIpcError(err);

    case "invalid_input":
      return resolveInvalidInput(err);

    case "encode_error":
    case "decode_error":
    case "unexpected_response":
      return {
        type: "message",
        text: "内部エラーが発生しました。アプリを再起動してください",
      };

    default:
      return {
        type: "message",
        text: "予期しないエラーが発生しました",
      };
  }
}

/** ipc_error サブ分岐（ipc_code で分岐、message に依存しない） */
function resolveIpcError(err: GUIError): ErrorResult {
  switch (err.ipc_code) {
    case "vault_locked":
      return { type: "vault_locked" };

    case "hotkey_conflict":
      return {
        type: "message",
        text: `${err.hotkey_conflict_entry
          ? `選択したホットキーは別エントリ（${err.hotkey_conflict_entry}）に割り当て済みです`
          : "選択したホットキーは既に使用されています"}`,
      };

    case "crypto":
      return resolveCryptoError(err.crypto_reason);

    case "backoff_active":
      return {
        type: "message",
        text: `試行回数の上限に達しました。${err.wait_secs ?? "しばらく"}秒後に再試行してください`,
      };

    case "recovery_required":
      return {
        type: "message",
        text: "パスワードによるアンロックができません。recovery 語でアンロックしてください（Sub-D 対応予定）",
      };

    case "not_found":
      return {
        type: "message",
        text: "エントリが見つかりません（一覧を更新します）",
      };

    case "invalid_label":
      return {
        type: "message",
        text: "ラベルの形式が正しくありません",
      };

    case "encryption_unsupported":
      return {
        type: "message",
        text: "この環境では暗号化がサポートされていません",
      };

    case "protocol_downgrade":
      return {
        type: "message",
        text: "プロトコルのダウングレードが検出されました。接続を拒否しました",
      };

    case "persistence":
      return {
        type: "message",
        text: "データの保存に失敗しました。アプリを再起動してください",
      };

    case "domain":
    case "internal":
    default:
      return {
        type: "message",
        text: "予期しないエラーが発生しました",
      };
  }
}

/** crypto_reason サブ分岐 */
function resolveCryptoError(reason: string | undefined): MessageResult {
  switch (reason) {
    case "wrong-password":
      return { type: "message", text: "パスワードが一致しません" };
    case "weak-password":
      return { type: "message", text: "パスワードが脆弱すぎます" };
    case "nonce-limit-exceeded":
      return {
        type: "message",
        text: "vault の再暗号化が必要です。shikomi vault rekey を実行してください",
      };
    default:
      return { type: "message", text: "暗号化エラーが発生しました" };
  }
}

/**
 * invalid_input サブ分岐（invalid_input_code で分岐、message パース廃止）。
 *
 * Rust 側 `invalid_input_code_key()` が出力する安定識別子で switch する。
 * `message` フィールドをユーザー表示や条件分岐に使用してはならない（§2.2）。
 */
function resolveInvalidInput(err: GUIError): MessageResult {
  switch (err.invalid_input_code) {
    case "label_empty":
      return { type: "message", text: "ラベルを入力してください" };
    case "label_invalid":
      return { type: "message", text: "ラベルの形式が正しくありません" };
    case "value_empty":
      return { type: "message", text: "値を入力してください" };
    case "password_empty":
      return { type: "message", text: "パスワードを入力してください" };
    case "confirmation_required":
      return { type: "message", text: "確認チェックボックスを有効にしてください" };
    case "id_invalid":
      return { type: "message", text: "無効なエントリIDです" };
    case "hotkey_invalid":
      return { type: "message", text: "ホットキーの形式が正しくありません" };
    default:
      return { type: "message", text: "入力内容に誤りがあります" };
  }
}

// ---------------------------------------------------------------------------
// 便利関数 — コンポーネントがメッセージ文字列を直接欲しい場合
// ---------------------------------------------------------------------------

/** `resolveError` の戻り値が `message` 型の場合に text を返す。それ以外は null。 */
export function resolveMessage(err: GUIError): string | null {
  const result = resolveError(err);
  return result.type === "message" ? result.text : null;
}

/** エラーが vault_locked かどうかを判定する */
export function isVaultLocked(err: GUIError): boolean {
  return err.kind === "ipc_error" && err.ipc_code === "vault_locked";
}

/** エラーが切断系（daemon_not_running / not_connected / connection_failed）かどうか */
export function isDisconnectError(err: GUIError): boolean {
  return (
    err.kind === "daemon_not_running" ||
    err.kind === "not_connected" ||
    err.kind === "connection_failed"
  );
}
