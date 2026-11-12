import { useEffect, useSyncExternalStore } from "react";
import { action, rpc } from "./store";
import type { Kind } from "./types";
export type CycleInput = { reset: boolean; inputs: Record<string, string> };
export type CycleSample = {
  reset: boolean;
  outputs: Record<string, string>;
  registers: Record<string, string>;
};
export type Simulation = {
  id: string;
  revision: number;
  selected: number;
  radix: 10 | 16;
  cycles: CycleInput[];
  samples: CycleSample[];
  signals: { id: string; name: string; kind: Kind; width: number }[];
  annotation: {
    revision: number;
    values: Record<string, string>;
    register_d: Record<string, string>;
  } | null;
};
type Response = {
  changed: boolean;
  cursor: string;
  session?: Simulation | null;
};
let simulation: Simulation | null = null;
let cursor: string | null = null;
const listeners = new Set<() => void>();
let polling = false;
let pending = false;
let requestEpoch = 0;
function accept(result: Response) {
  if (!result.changed) return;
  cursor = result.cursor;
  simulation = result.session ?? null;
  listeners.forEach((listener) => listener());
}
async function refreshSimulation() {
  if (polling || pending) return;
  polling = true;
  try {
    const epoch = requestEpoch;
    const response = await rpc<Response>("simulation.state", { cursor });
    // A newer explicit selection must not be overwritten by an older poll.
    if (!pending && epoch === requestEpoch) accept(response);
  } catch {
    /* Connection status is owned by the main store. */
  } finally {
    polling = false;
  }
}
export async function simulate(method: string, args: Record<string, unknown>) {
  if (pending) return;
  pending = true;
  requestEpoch += 1;
  try {
    const result = await action<Response>(method, args);
    if (result) accept(result);
    return result?.session;
  } finally {
    pending = false;
  }
}
function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
export function useSimulation() {
  const trace = useSyncExternalStore(subscribe, () => simulation);
  useEffect(() => {
    void refreshSimulation();
    const timer = setInterval(() => void refreshSimulation(), 800);
    return () => clearInterval(timer);
  }, []);
  return trace;
}
export function sampleValue(
  trace: Simulation,
  index: number,
  signal: Simulation["signals"][number],
) {
  if (signal.kind === "input") return trace.cycles[index].inputs[signal.name];
  return (
    signal.kind === "register"
      ? trace.samples[index].registers
      : trace.samples[index].outputs
  )[signal.name];
}
export function formatValue(value: string, radix: number, width?: number) {
  const number = BigInt(value);
  return radix === 16
    ? `0x${number
        .toString(16)
        .toUpperCase()
        .padStart(width ? Math.ceil(width / 4) : 1, "0")}`
    : number.toString();
}
