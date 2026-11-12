import type { Document, Edge, Node, Snapshot } from "./types.ts";
export interface CollectionDelta<T> {
  upsert: T[];
  remove: string[];
  order?: string[];
}
export interface Delta {
  set: Partial<Omit<Snapshot, "document" | "history" | "experiments">>;
  document?: {
    name?: string;
    nodes?: CollectionDelta<Node>;
    edges?: CollectionDelta<Edge>;
  };
  history?: CollectionDelta<Snapshot["history"][number]>;
  experiments?: CollectionDelta<Snapshot["experiments"][number]>;
}
export interface SyncResponse {
  cursor: string;
  changed: boolean;
  kind?: "snapshot" | "delta";
  base?: string;
  snapshot?: Snapshot;
  deltas?: Delta[];
}
function collection<T>(
  old: T[],
  delta: CollectionDelta<T>,
  id: (v: T) => string,
): T[] {
  const values = new Map(old.map((v) => [id(v), v]));
  const seen = new Set<string>();
  for (const key of delta.remove) {
    if (!values.delete(key)) throw Error("Unknown delta removal");
  }
  for (const value of delta.upsert) {
    const key = id(value);
    if (typeof key !== "string" || seen.has(key))
      throw Error("Invalid delta entity");
    seen.add(key);
    values.set(key, value);
  }
  if (!delta.order) return [...values.values()];
  if (
    delta.order.length !== values.size ||
    new Set(delta.order).size !== values.size
  )
    throw Error("Invalid delta order");
  return delta.order.map((key) => {
    const value = values.get(key);
    if (!value) throw Error("Missing delta entity");
    return value;
  });
}
const setKeys = new Set([
  "revision",
  "can_undo",
  "can_redo",
  "report",
  "selected",
  "selection",
  "locale",
  "node_colors",
  "panels",
  "diagnostics",
]);
/** Apply atomically with structural sharing; never mutate the committed snapshot. */
export function applySync(
  current: Snapshot | null,
  cursor: string | null,
  response: SyncResponse,
): Snapshot {
  if (response.kind === "snapshot" && response.snapshot)
    return response.snapshot;
  if (
    !current ||
    !cursor ||
    response.kind !== "delta" ||
    response.base !== cursor ||
    !response.deltas
  )
    throw Error("Sync base mismatch");
  let next = current;
  for (const delta of response.deltas) {
    if (Object.keys(delta.set).some((k) => !setKeys.has(k)))
      throw Error("Unexpected delta field");
    next = { ...next, ...delta.set };
    if (delta.document) {
      const change = delta.document;
      const document: Document = { ...next.document };
      if (change.name !== undefined) document.name = change.name;
      if (change.nodes)
        document.nodes = collection(document.nodes, change.nodes, (n) => n.id);
      if (change.edges)
        document.edges = collection(document.edges, change.edges, (e) => e.id);
      next.document = document;
    }
    if (delta.history)
      next.history = collection(next.history, delta.history, (r) =>
        String(r.revision),
      );
    if (delta.experiments)
      next.experiments = collection(
        next.experiments,
        delta.experiments,
        (e) => e.id,
      );
  }
  return next;
}
