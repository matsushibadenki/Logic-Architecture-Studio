export type Kind =
  | "input"
  | "output"
  | "add"
  | "sub"
  | "nand"
  | "nor"
  | "xnor"
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
  | "sign_extend"
  | "module";
export type Locale = "en" | "ja" | "zh";
export interface ModulePort {
  name: string;
  width: number;
  signed: boolean;
}
export interface ModuleDefinition {
  nodes: Node[];
  edges: Edge[];
}
export type NodeConfig =
  | {
      type: "module";
      definition: string;
      inputs: ModulePort[];
      outputs: ModulePort[];
    }
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
  source_port?: string;
  to: string;
  port: string;
}
export interface Document {
  name: string;
  nodes: Node[];
  edges: Edge[];
  modules?: Record<string, ModuleDefinition>;
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
  gate_symbols?: boolean;
  hide_gate_frames?: boolean;
  theme?: "dark" | "light";
  node_colors?: Partial<Record<Kind, string>>;
  panels: Record<string, string>;
  diagnostics: { code: string; node: string; message: string }[];
}
export type Operation = Record<string, unknown>;
export function inputPortNames(kind: Kind): string[] {
  if (kind === "input" || kind === "constant" || kind === "module") return [];
  if (["slice", "zero_extend", "sign_extend"].includes(kind)) return ["a"];
  if (kind === "concat") return ["hi", "lo"];
  if (kind === "register") return ["d"];
  if (kind === "not" || kind === "output") return ["a"];
  if (kind === "mux") return ["d0", "d1", "d2", "sel"];
  return ["a", "b"];
}
export function ports(n: Node): [string, number][] {
  if (n.config?.type === "module")
    return n.config.inputs.map((p) => [p.name, p.width]);
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
export function outputs(n: Node): [string, number, boolean][] {
  if (n.config?.type === "module")
    return n.config.outputs.map((p) => [p.name, p.width, p.signed]);
  return n.kind === "output" ? [] : [["y", n.width, !!n.signed]];
}
export const kinds: Kind[] = [
  "input",
  "output",
  "add",
  "sub",
  "and",
  "nand",
  "nor",
  "xnor",
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
  module: "M",
  input: "→",
  output: "→",
  add: "+",
  sub: "−",
  and: "&",
  nand: "NAND",
  nor: "NOR",
  xnor: "XNOR",
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
  if (n.config?.type === "module")
    return n.config.inputs.find((p) => p.name === port)?.signed;
  if (port === "sel" && n.kind === "mux") return false;
  if (n.kind === "zero_extend" || n.kind === "sign_extend") return undefined;
  return !!n.signed;
}

export function isSequential(document: Document): boolean {
  const visited = new Set<string>();
  const check = (nodes: Node[]): boolean =>
    nodes.some((n) => {
      if (n.kind === "register") return true;
      if (n.config?.type !== "module" || visited.has(n.config.definition))
        return false;
      visited.add(n.config.definition);
      return check(document.modules?.[n.config.definition]?.nodes ?? []);
    });
  return check(document.nodes);
}
