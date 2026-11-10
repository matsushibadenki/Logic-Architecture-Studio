export type Kind =
  "input" | "output" | "add" | "sub" | "and" | "or" | "xor" | "not" | "mux";
export type Locale = "en" | "ja" | "zh";
export interface Node {
  id: string;
  name: string;
  kind: Kind;
  width: number;
  x: number;
  y: number;
}
export interface Edge {
  id: string;
  from: string;
  to: string;
  port: string;
}
export interface Document {
  name: string;
  nodes: Node[];
  edges: Edge[];
}
export interface Report {
  revision: number;
  passed: number;
  total: number;
  failures: string[];
  engine: string;
}
export interface Candidate {
  id: string;
  name: string;
  document: Document;
  report: Report | null;
}
export interface Experiment {
  id: string;
  name: string;
  base_revision: number;
  candidates: Candidate[];
}
export interface Snapshot {
  document: Document;
  revision: number;
  history: { revision: number; label: string; actor: string }[];
  can_undo: boolean;
  can_redo: boolean;
  report: Report | null;
  experiments: Experiment[];
  selected: string | null;
  locale: Locale;
  panels: Record<string, string>;
  diagnostics: { code: string; node: string; message: string }[];
}
export type Operation = Record<string, unknown>;
export function ports(n: Node): [string, number][] {
  if (n.kind === "input") return [];
  if (n.kind === "not" || n.kind === "output") return [["a", n.width]];
  if (n.kind === "mux")
    return [
      ["d0", n.width],
      ["d1", n.width],
      ["d2", n.width],
      ["sel", 2],
    ];
  return [
    ["a", n.width],
    ["b", n.width],
  ];
}
export const kinds: Kind[] = [
  "input",
  "output",
  "add",
  "sub",
  "and",
  "or",
  "xor",
  "not",
  "mux",
];
export const symbols: Record<Kind, string> = {
  input: "→",
  output: "→",
  add: "+",
  sub: "−",
  and: "&",
  or: "≥1",
  xor: "⊕",
  not: "¬",
  mux: "MUX",
};
