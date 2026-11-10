// UNGE-inspired presentation; port anchors follow UNGE's port_anchor geometry.
// See THIRD_PARTY_NOTICES.md and vendor/unge/LICENSE-MIT.
import { ports, type Node, type Kind, type Locale } from "./types";
export const NODE_WIDTH = 208;
export function nodeHeight(kind: Kind) {
  return kind === "mux"
    ? 208
    : kind === "input" || kind === "output"
      ? 128
      : 144;
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
      : kind === "mux"
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
  };
  return expressions[node.kind];
}
export function cablePath(a: Node, b: Node, index: number, nodes: Node[]) {
  const sx = a.x + NODE_WIDTH,
    sy = a.y + portY(a, 0, true),
    tx = b.x,
    ty = b.y + portY(b, index);
  const bend = Math.max(44, Math.abs(tx - sx) * 0.5);
  // Keep the direct UNGE-style cubic unless it intersects another node.
  let obstructed = false;
  for (let i = 1; i < 24; i++) {
    const t = i / 24,
      u = 1 - t;
    const x =
      u * u * u * sx +
      3 * u * u * t * (sx + bend) +
      3 * u * t * t * (tx - bend) +
      t * t * t * tx;
    const y =
      u * u * u * sy + 3 * u * u * t * sy + 3 * u * t * t * ty + t * t * t * ty;
    if (
      nodes.some(
        (n) =>
          n.id !== a.id &&
          n.id !== b.id &&
          x > n.x - 8 &&
          x < n.x + NODE_WIDTH + 8 &&
          y > n.y - 8 &&
          y < n.y + nodeHeight(n.kind) + 8,
      )
    ) {
      obstructed = true;
      break;
    }
  }
  if (obstructed && tx > sx + 65) {
    const bottom = Math.max(...nodes.map((n) => n.y + nodeHeight(n.kind))) + 36;
    return `M ${sx} ${sy} C ${sx + 24} ${sy} ${sx + 24} ${bottom} ${sx + 48} ${bottom} L ${tx - 48} ${bottom} C ${tx - 24} ${bottom} ${tx - 24} ${ty} ${tx} ${ty}`;
  }
  return `M ${sx} ${sy} C ${sx + bend} ${sy} ${tx - bend} ${ty} ${tx} ${ty}`;
}
