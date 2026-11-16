// UNGE-inspired presentation; port anchors follow UNGE's port_anchor geometry.
// See THIRD_PARTY_NOTICES.md and vendor/unge/LICENSE-MIT.
import {
  inputPortNames,
  outputs,
  ports,
  type Node,
  type Kind,
  type Locale,
} from "./types";
export const NODE_WIDTH = 288;
export function nodeHeight(value: Kind | Node) {
  if (typeof value !== "string")
    return Math.max(
      NODE_WIDTH,
      (Math.max(ports(value).length, outputs(value).length, 1) + 1) * 64,
    );
  const kind = value;
  // Match the Rust layout: minimum square, 64 units per port row.
  return Math.max(
    NODE_WIDTH,
    (Math.max(1, inputPortNames(kind).length) + 1) * 64,
  );
}
export function portY(node: Node, index: number, output = false) {
  return (
    (nodeHeight(node) * (index + 1)) /
    ((output ? outputs(node).length : ports(node).length) + 1)
  );
}
export function tone(kind: Kind) {
  return kind === "input" || kind === "constant"
    ? "mint"
    : kind === "output"
      ? "rose"
      : kind === "mux" || kind === "register" || kind === "sign_extend"
        ? "amber"
        : ["add", "sub"].includes(kind)
          ? "blue"
          : "violet";
}
export function caption(node: Node, locale: Locale) {
  const expressions: Record<Kind, string> = {
    module: node.config?.type === "module" ? node.config.definition : "Module",
    input: locale === "ja" ? "入力" : locale === "zh" ? "输入" : "Input",
    output: locale === "ja" ? "出力" : locale === "zh" ? "输出" : "Output",
    constant: `y = ${node.config?.type === "constant" ? node.config.value : "0"}`,
    slice:
      node.config?.type === "slice"
        ? `y = a[${node.config.lsb + node.width - 1}:${node.config.lsb}]`
        : "[:]",
    concat: "y = {hi, lo}",
    zero_extend:
      node.config?.type === "zero_extend"
        ? `u${node.config.input_width} → ${node.signed ? "s" : "u"}${node.width} · 0`
        : "ZX",
    sign_extend:
      node.config?.type === "sign_extend"
        ? `s${node.config.input_width} → ${node.signed ? "s" : "u"}${node.width} · MSB`
        : "SX",
    add: "y = a + b",
    sub: "y = a − b",
    and: "y = a & b",
    nand: "y = ~(a & b)",
    nor: "y = ~(a | b)",
    xnor: "y = ~(a ^ b)",
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
  const bottom = Math.max(a.y + nodeHeight(a), b.y + nodeHeight(b)) + 24;
  // Match native routing: upper inputs use outer return lanes so separate
  // signals do not share the bottom run or the vertical approach to a node.
  const laneOffset = Math.max(0, ports(b).length - index - 1) * 24;
  const clearance = 18 + laneOffset;
  return `M ${sx} ${sy} H ${sx + clearance} V ${bottom + laneOffset} H ${tx - clearance} V ${ty} H ${tx}`;
}

// Match unge_core::grid_spacing and snap_to_grid (including negative ties).
export function gridSpacing(zoom: number) {
  return zoom < 0.3 ? 120 : 24;
}
export function snapToGrid(value: number, spacing: number) {
  return Math.round(value / spacing) * spacing;
}

// Match unge-render::wire_width for nets, internal leads, symbols and previews.
export function wireWidth(zoom: number) {
  return Math.max(3, 1.5 / Math.max(zoom, 0.02));
}
