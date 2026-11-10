import { useSyncExternalStore } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Snapshot, Operation } from "./types";
import { applySync, type SyncResponse } from "./sync";
import { noteSync } from "./measurement";
export const native = isTauri();
type State = {
  snapshot: Snapshot | null;
  error: string | null;
  busy: boolean;
  connected: boolean;
};
let state: State = {
  snapshot: null,
  error: null,
  busy: false,
  connected: false,
};
const listeners = new Set<() => void>();
function update(next: Partial<State>) {
  state = { ...state, ...next };
  listeners.forEach((fn) => fn());
}
export async function rpc<T = unknown>(
  method: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (
    [
      "design.apply_patch",
      "design.undo",
      "design.redo",
      "project.import",
      "layout.auto_arrange",
      "view.focus_module",
      "view.preferences",
    ].includes(method)
  ) {
    args = { method, args };
    method = "view.command";
  }
  if (native) return invoke<T>("command", { method, args });
  const response = await fetch("/api/command", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ method, args }),
  });
  const data = await response.json();
  if (!data.ok) throw new Error(data.error);
  return data.result as T;
}
let cursor: string | null = null;
let refreshing: Promise<void> | null = null;
let refreshAgain = false;
export function refresh() {
  if (refreshing) {
    refreshAgain = true;
    return refreshing;
  }
  refreshing = (async () => {
    try {
      do {
        refreshAgain = false;
        const response = await rpc<SyncResponse>("project.sync", {
          cursor,
          scope: "web",
        });
        if (response.changed) {
          try {
            const snapshot = applySync(state.snapshot, cursor, response);
            cursor = response.cursor;
            noteSync(response, snapshot.revision);
            update({ snapshot, connected: true });
          } catch (e) {
            if (cursor === null) throw e;
            cursor = null;
            refreshAgain = true;
          }
        } else if (!state.connected) {
          update({ connected: true });
        }
      } while (refreshAgain);
    } catch (e) {
      update({ connected: false, error: String(e) });
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}
export function useStudio() {
  return useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    () => state,
  );
}
export function clearError() {
  update({ error: null });
}
export async function action<T = unknown>(
  method: string,
  args: Record<string, unknown> = {},
): Promise<T | undefined> {
  if (state.busy) return;
  update({ busy: true, error: null });
  try {
    const result = await rpc<T>(method, args);
    await refresh();
    return result;
  } catch (e) {
    update({ error: e instanceof Error ? e.message : String(e) });
  } finally {
    update({ busy: false });
  }
}
export function patch(operations: Operation[], label: string) {
  if (!state.snapshot) return Promise.resolve();
  return action("design.apply_patch", {
    expected_revision: state.snapshot.revision,
    label,
    operations,
  });
}
export function select(id: string) {
  return action("view.focus_module", { id });
}
export async function nativeAction(
  method: string,
  args: Record<string, unknown> = {},
) {
  try {
    await invoke(method, args);
    return true;
  } catch (e) {
    update({ error: String(e) });
    return false;
  }
}
if (native) {
  void listen("design-changed", () => {
    void refresh();
  }).catch((e) => {
    update({ error: String(e) });
  });
}
void refresh();
setInterval(() => {
  if (!state.busy) void refresh();
}, 800);
