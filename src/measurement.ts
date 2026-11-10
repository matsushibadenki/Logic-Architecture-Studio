import { invoke, isTauri } from "@tauri-apps/api/core";
import type { SyncResponse } from "./sync";
let enabled = false;
if (isTauri())
  void invoke<boolean>("measurement_enabled")
    .then((v) => {
      enabled = v;
    })
    .catch(() => {
      enabled = false;
    });
const transfers = new Map<number, { bytes: number; kind: string }>();
export function noteSync(response: SyncResponse, revision: number) {
  if (!enabled || transfers.has(revision)) return;
  transfers.set(revision, {
    bytes: new TextEncoder().encode(JSON.stringify(response)).length,
    kind: response.kind ?? "unchanged",
  });
  if (transfers.size > 100) transfers.delete(transfers.keys().next().value!);
}
/** Two animation callbacks acknowledge a React commit and a paint opportunity,
 * not physical display scan-out. Cross-process clocks are never subtracted. */
export function ackFrame(revision: number) {
  if (!enabled) return;
  const query = new URLSearchParams(location.search);
  if (query.has("split")) return;
  const label = query.get("panel") ?? "controls";
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      void invoke("web_frame_ack", {
        label,
        revision,
        ...(transfers.get(revision) ?? { bytes: 0, kind: "unknown" }),
      });
    }),
  );
}
