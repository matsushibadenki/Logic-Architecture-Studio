export type Kind =
  | "input"
  | "output"
  | "add"
  | "sub"
  | "and"
  | "or"
  | "xor"
  | "not"
  | "mux"
  | "register"
  | "constant"
  | "slice"
  | "concat"
  | "zero_extend"
  | "sign_extend";
export type Locale = "en" | "ja" | "zh";
export type NodeConfig =
  | { type: "none" }
  | { type: "constant"; value: string }
  | { type: "slice"; input_width: number; lsb: number }
  | { type: "zero_extend" | "sign_extend"; input_width: number }
  | { type: "concat"; high_width: number; low_width: number };
export function defaultConfig(kind: Kind, width: number): NodeConfig {
  if (kind === "zero_extend" || kind === "sign_extend")
    return { type: kind, input_width: Math.max(1, Math.floor(width / 2)) };
  if (kind === "constant") return { type: "constant", value: "0" };
  if (kind === "slice") return { type: "slice", input_width: width, lsb: 0 };
  if (kind === "concat")
    return {
      type: "concat",
      high_width: Math.floor(width / 2),
      low_width: Math.ceil(width / 2),
    };
  return { type: "none" };
}
export interface Node {
  id: string;
  name: string;
  kind: Kind;
  width: number;
  signed?: boolean;
  config?: NodeConfig;
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
  selection?: string[];
  locale: Locale;
  testbench?: Testbench;
  node_colors?: Partial<Record<Kind, string>>;
  panels: Record<string, string>;
  diagnostics: { code: string; node: string; message: string }[];
}
export type Operation = Record<string, unknown>;
export function inputPortNames(kind: Kind): string[] {
  if (kind === "input" || kind === "constant") return [];
  if (["slice", "zero_extend", "sign_extend"].includes(kind)) return ["a"];
  if (kind === "concat") return ["hi", "lo"];
  if (kind === "register") return ["d"];
  if (kind === "not" || kind === "output") return ["a"];
  if (kind === "mux") return ["d0", "d1", "d2", "sel"];
  return ["a", "b"];
}
export function ports(n: Node): [string, number][] {
  return inputPortNames(n.kind).map((name) => [
    name,
    n.config?.type === "slice" ||
    n.config?.type === "zero_extend" ||
    n.config?.type === "sign_extend"
      ? n.config.input_width
      : n.config?.type === "concat"
        ? name === "hi"
          ? n.config.high_width
          : n.config.low_width
        : name === "sel"
          ? 2
          : n.width,
  ]);
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
  "register",
  "constant",
  "slice",
  "concat",
  "zero_extend",
  "sign_extend",
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
  register: "D",
  constant: "#",
  slice: "[:]",
  concat: "{}",
  zero_extend: "ZX",
  sign_extend: "SX",
};

export interface Stimulus {
  reset: boolean;
  inputs: Record<string, string>;
  expected: Record<string, string>;
}
export interface Testbench {
  version: number;
  rows: Stimulus[];
}

/** Undefined accepts either signedness at explicit extension inputs. */
export function inputSigned(n: Node, port: string): boolean | undefined {
  if (port === "sel" && n.kind === "mux") return false;
  if (n.kind === "zero_extend" || n.kind === "sign_extend") return undefined;
  return !!n.signed;
}
