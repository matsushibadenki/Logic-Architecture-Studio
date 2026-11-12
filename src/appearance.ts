import type { Kind, Snapshot } from "./types";
// Keep the fallback swatches aligned with design_core::default_node_colors.
export const defaultNodeColors: Record<Kind, string> = {
  input: "#5fd5b8",
  output: "#ee8ba0",
  add: "#78a2ff",
  sub: "#78a2ff",
  and: "#b69df9",
  or: "#b69df9",
  xor: "#b69df9",
  not: "#b69df9",
  mux: "#f1bd68",
  register: "#f1bd68",
};
export function nodeColor(snapshot: Snapshot, kind: Kind) {
  return snapshot.node_colors?.[kind] ?? defaultNodeColors[kind];
}
