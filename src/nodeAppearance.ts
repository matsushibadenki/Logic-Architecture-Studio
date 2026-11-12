// UNGE-inspired presentation; port anchors follow UNGE's port_anchor geometry.
// See THIRD_PARTY_NOTICES.md and vendor/unge/LICENSE-MIT.
import {
  inputPortNames,
  ports,
  type Node,
  type Kind,
  type Locale,
} from "./types";
export const NODE_WIDTH = 288;
export function nodeHeight(kind: Kind) {
  // Match the Rust layout: minimum square, 64 units per port row.
  return Math.max(
    NODE_WIDTH,
    (Math.max(1, inputPortNames(kind).length) + 1) * 64,
  );
}
export function portY(node: Node, index: number, output = false) {
  return (
    (nodeHeight(node.kind) * (index + 1)) /
    ((output ? 1 : ports(node).length) + 1)
  );
}
export function tone(kind: Kind) {
  return kind === "input"
    ? "mint"
    : kind === "output"
      ? "rose"
      : kind === "mux" || kind === "register"
        ? "amber"
        : ["add", "sub"].includes(kind)
          ? "blue"
          : "violet";
}
export function caption(node: Node, locale: Locale) {
  const expressions: Record<Kind, string> = {
    input: locale === "ja" ? "入力" : locale === "zh" ? "输入" : "Input",
    output: locale === "ja" ? "出力" : locale === "zh" ? "输出" : "Output",
    add: "y = a + b",
    sub: "y = a − b",
    and: "y = a & b",
    or: "y = a | b",
    xor: "y = a ^ b",
    not: "y = ~a",
    mux: "sel → d0 / d1 / d2",
    register:
      locale === "ja" ? "レジスタ" : locale === "zh" ? "寄存器" : "Register",
  };
  return expressions[node.kind];
}
export function cablePath(a: Node, b: Node, index: number) {
  const sx = a.x + NODE_WIDTH;
  const sy = a.y + portY(a, 0, true);
  const tx = b.x;
  const ty = b.y + portY(b, index);
  if (sx <= tx) {
    const mid = (sx + tx) / 2;
    return `M ${sx} ${sy} H ${mid} V ${ty} H ${tx}`;
  }
  const bottom =
    Math.max(a.y + nodeHeight(a.kind), b.y + nodeHeight(b.kind)) + 24;
  return `M ${sx} ${sy} H ${sx + 18} V ${bottom} H ${tx - 18} V ${ty} H ${tx}`;
}

// Match unge_core::grid_spacing and snap_to_grid (including negative ties).
export function gridSpacing(zoom: number) {
  return zoom < 0.3 ? 120 : 24;
}
export function snapToGrid(value: number, spacing: number) {
  return Math.round(value / spacing) * spacing;
}
