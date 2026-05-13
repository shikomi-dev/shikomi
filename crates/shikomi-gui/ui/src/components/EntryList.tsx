/**
 * EntryList — エントリ一覧テーブル + ホットキーバッジ + 削除確認（REQ-UI-02, REQ-UI-06）。
 *
 * 設計根拠: docs/features/shikomi-gui/ui/detailed-design/components.md §1.4
 */

import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import type { GUIError, RecordSummary } from "../lib/ipc";
import { deleteEntry } from "../lib/ipc";
import { handleCommandError, refreshEntries } from "../store/vault";
import { resolveMessage } from "../lib/errors";

interface Props {
  entries: RecordSummary[];
  onEdit: (id: string) => void;
  onAdd: () => void;
}

const KIND_LABEL: Record<string, string> = {
  text:   "テキスト",
  secret: "シークレット",
};

const EntryList: Component<Props> = (props) => {
  const [confirmDeleteId, setConfirmDeleteId] = createSignal<string | null>(null);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  const doDelete = async (id: string) => {
    setErrorMsg(null);
    const retryFn = () => doDelete(id);
    try {
      await deleteEntry(id);
      setConfirmDeleteId(null);
      await refreshEntries();
    } catch (e) {
      const err = e as GUIError;
      if (handleCommandError(err, retryFn)) return;
      if (err.kind === "ipc_error" && err.ipc_code === "not_found") {
        // 存在しないエントリ → 一覧を更新して終了
        await refreshEntries();
        return;
      }
      setErrorMsg(resolveMessage(err) ?? "削除に失敗しました");
    }
  };

  return (
    <div>
      {errorMsg() && <div class="inline-error">{errorMsg()}</div>}
      <table class="entry-list-table">
        <thead>
          <tr>
            <th>ラベル</th>
            <th>種別</th>
            <th>ホットキー</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.entries}>
            {(entry) => (
              <tr>
                <td>{entry.label}</td>
                <td>{KIND_LABEL[entry.kind] ?? entry.kind}</td>
                <td>
                  {entry.hotkey ? (
                    <span class="hotkey-badge">{entry.hotkey}</span>
                  ) : (
                    <span style="color: var(--color-text-secondary);">─</span>
                  )}
                </td>
                <td class="entry-actions">
                  <Show
                    when={confirmDeleteId() === entry.id}
                    fallback={
                      <>
                        <button
                          class="btn btn-secondary"
                          onClick={() => props.onEdit(entry.id)}
                        >
                          編集
                        </button>
                        <button
                          class="btn btn-danger"
                          onClick={() => {
                            setConfirmDeleteId(entry.id);
                            setErrorMsg(null);
                          }}
                        >
                          削除
                        </button>
                      </>
                    }
                  >
                    {/* 削除確認ダイアログ（インライン） */}
                    <span style="font-size: var(--font-size-small); color: var(--color-text-secondary);">
                      「{entry.label}」を削除しますか？
                    </span>
                    <button
                      class="btn btn-danger"
                      onClick={() => doDelete(entry.id)}
                    >
                      削除する
                    </button>
                    <button
                      class="btn btn-secondary"
                      onClick={() => setConfirmDeleteId(null)}
                    >
                      キャンセル
                    </button>
                  </Show>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <div class="entry-add-row">
        <button class="btn btn-primary" onClick={props.onAdd}>
          + エントリを追加
        </button>
      </div>
    </div>
  );
};

export default EntryList;
