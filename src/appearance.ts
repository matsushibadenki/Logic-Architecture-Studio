import type { Kind, Snapshot } from "./types";
// Keep the fallback swatches aligned with design_core::default_node_colors.
export const defaultNodeColors: Record<Kind, string> = {
  module: "#b69df9",
  zero_extend: "#b69df9",
  sign_extend: "#f1bd68",
  constant: "#5fd5b8",
  slice: "#b69df9",
  concat: "#b69df9",
  input: "#5fd5b8",
  output: "#ee8ba0",
  add: "#78a2ff",
  sub: "#78a2ff",
  and: "#b69df9",
  nand: "#b69df9",
  nor: "#b69df9",
  xnor: "#b69df9",
  or: "#b69df9",
  xor: "#b69df9",
  not: "#b69df9",
  mux: "#f1bd68",
  register: "#f1bd68",
};
export function storedNodeColor(snapshot: Snapshot, kind: Kind) {
  return snapshot.node_colors?.[kind] ?? defaultNodeColors[kind];
}

// Keep multi-letter gate names inside compact badges, with room for side bearings.
export function symbolFontSize(
  symbol: string,
  availableWidth: number,
  preferredSize: number,
) {
  return Math.min(
    preferredSize,
    availableWidth / (Array.from(symbol).length * 0.75 || 1),
  );
}

export function nodeColor(snapshot: Snapshot, kind: Kind) {
  const color = storedNodeColor(snapshot, kind);
  if (
    snapshot.theme !== "light" ||
    color.toLowerCase() !== defaultNodeColors[kind]
  )
    return color;
  if (["input", "constant"].includes(kind)) return "#14806b";
  if (kind === "output") return "#b74267";
  if (["add", "sub"].includes(kind)) return "#2d63d5";
  if (["mux", "register", "sign_extend"].includes(kind)) return "#a16917";
  return "#7e54c0";
}
