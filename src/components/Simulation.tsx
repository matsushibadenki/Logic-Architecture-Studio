import { useState } from "react";
import {
  Play,
  RefreshCw,
  Download,
  List,
  Activity,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { action, native, nativeAction } from "../store";
import {
  simulate,
  useSimulation,
  formatValue,
  sampleValue,
  type Simulation as Trace,
} from "../simulation";
import { translation } from "../i18n";
import type { Snapshot } from "../types";
import { Hint, IconButton } from "./ui";
import { download } from "./Views";

export function Simulation({
  snapshot,
  busy,
}: {
  snapshot: Snapshot;
  busy: boolean;
}) {
  const t = translation(snapshot.locale);
  const trace = useSimulation();
  const current = trace?.revision === snapshot.revision;
  const [values, setValues] = useState<Record<string, string>>({});
  const [mode, setMode] = useState<"table" | "wave">("table");
  const sequential = snapshot.document.nodes.some((n) => n.kind === "register");
  const inputs = () =>
    Object.fromEntries(
      snapshot.document.nodes
        .filter((n) => n.kind === "input")
        .map((n) => [n.name, values[n.name] ?? "0"]),
    );
  const clock = (reset: boolean) =>
    simulate("simulation.cycles", {
      expected_revision: snapshot.revision,
      cycles: [
        ...(reset ? [] : current ? trace!.cycles : []),
        { reset, inputs: inputs() },
      ],
    });
  const evaluate = () => simulate("simulation.evaluate", { inputs: inputs() });
  const choose = (index: number, radix = trace?.radix ?? 10) =>
    trace &&
    simulate("simulation.select", {
      expected_revision: snapshot.revision,
      trace_id: trace.id,
      index,
      radix,
    });
  async function rerun() {
    if (!trace) return;
    if (sequential)
      await simulate("simulation.cycles", {
        expected_revision: snapshot.revision,
        cycles: trace.cycles,
      });
    else await evaluate();
  }
  async function saveVcd() {
    if (!trace) return;
    if (native) {
      await nativeAction("save_vcd", {
        expectedRevision: snapshot.revision,
        traceId: trace.id,
      });
    } else {
      const result = await action<{ source: string }>("simulation.vcd", {
        expected_revision: snapshot.revision,
        trace_id: trace.id,
      });
      if (result) download(result.source, "simulation.vcd");
    }
  }
  return (
    <div className="simulation-form">
      <div className="simulation-inputs">
        {snapshot.document.nodes
          .filter((n) => n.kind === "input")
          .map((n) => (
            <label key={n.id}>
              {n.name}
              <input
                aria-label={`input ${n.name}`}
                value={values[n.name] ?? "0"}
                onChange={(e) =>
                  setValues((v) => ({ ...v, [n.name]: e.target.value }))
                }
              />
              <small>{n.width}u</small>
            </label>
          ))}
        {sequential ? (
          <>
            <IconButton
              label={t("resetTrace")}
              icon={RefreshCw}
              disabled={busy}
              onClick={() => void clock(true)}
            />
            <IconButton
              label={t("clockStep")}
              icon={Play}
              disabled={busy || !current || trace!.cycles.length >= 256}
              onClick={() => void clock(false)}
            />
          </>
        ) : (
          <IconButton
            label={t("evaluate")}
            icon={Play}
            disabled={busy}
            onClick={() => void evaluate()}
          />
        )}
        <Hint text={t("simulationSemantics")} />
      </div>
      {trace && (
        <>
          {!current && (
            <div className="stale-banner" role="status">
              <strong>{t("staleResults")}</strong>
              <button disabled={busy} onClick={() => void rerun()}>
                <RefreshCw size={14} />
                {t("rerun")}
              </button>
            </div>
          )}
          <div className="trace-toolbar">
            <IconButton
              label={t("cycleTrace")}
              icon={List}
              active={mode === "table"}
              onClick={() => setMode("table")}
            />
            <IconButton
              label={t("waveform")}
              icon={Activity}
              active={mode === "wave"}
              onClick={() => setMode("wave")}
            />
            <label className="radix-control">
              <span>{t("radix")}</span>
              <select
                aria-label={t("radix")}
                disabled={busy || !current}
                value={trace.radix}
                onChange={(e) =>
                  void choose(trace.selected, Number(e.target.value) as 10 | 16)
                }
              >
                <option value={10}>DEC</option>
                <option value={16}>HEX</option>
              </select>
            </label>
            <span className="trace-position">
              {t("sample")} {trace.selected + 1} / {trace.samples.length} · r
              {trace.revision}
            </span>
            <Hint text={t("postEdge")} />
            <IconButton
              label={t("exportVcd")}
              icon={Download}
              disabled={busy || !current}
              onClick={() => void saveVcd()}
            />
          </div>
          <div className={current ? "" : "stale-data"}>
            {mode === "table" ? (
              <div
                className="cycle-trace"
                role="region"
                aria-label={t("cycleTrace")}
                tabIndex={0}
              >
                <table>
                  <thead>
                    <tr>
                      <th scope="col">#</th>
                      {trace.signals.map((signal) => (
                        <th key={signal.id} scope="col">
                          {signal.name}
                          <small>
                            {signal.width}u ·{" "}
                            {signal.kind === "input"
                              ? t("inputs")
                              : signal.kind === "register"
                                ? "Q"
                                : t("outputs")}
                          </small>
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {trace.samples.map((sample, index) => (
                      <tr
                        key={index}
                        className={
                          trace.selected === index ? "selected-sample" : ""
                        }
                        onClick={() => {
                          if (current && !busy) void choose(index);
                        }}
                      >
                        <th scope="row">
                          <button
                            aria-label={`${t("sample")} ${index + 1}`}
                            aria-pressed={trace.selected === index}
                            disabled={busy || !current}
                            onClick={(event) => {
                              event.stopPropagation();
                              void choose(index);
                            }}
                          >
                            {index + 1}
                            {sample.reset ? " · RST" : ""}
                          </button>
                        </th>
                        {trace.signals.map((signal) => (
                          <td key={signal.id}>
                            {formatValue(
                              sampleValue(trace, index, signal),
                              trace.radix,
                              signal.width,
                            )}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <Waveform
                trace={trace}
                disabled={busy || !current}
                choose={(index) => void choose(index)}
                labels={{
                  sample: t("sample"),
                  signals: t("signals"),
                  zoom: t("zoomIn"),
                }}
              />
            )}
          </div>
        </>
      )}
    </div>
  );
}
function Waveform({
  trace,
  disabled,
  choose,
  labels,
}: {
  trace: Trace;
  disabled: boolean;
  choose: (index: number) => void;
  labels: { sample: string; signals: string; zoom: string };
}) {
  const [page, setPage] = useState(0);
  const [cell, setCell] = useState(112);
  const pages = Math.max(1, Math.ceil(trace.signals.length / 12));
  const start = Math.min(page, pages - 1) * 12;
  const signals = trace.signals.slice(start, start + 12);
  const width = trace.samples.length * cell;
  return (
    <>
      <div className="trace-toolbar">
        <span>
          {labels.signals} {start + 1}–{start + signals.length} /{" "}
          {trace.signals.length}
        </span>
        <IconButton
          label="←"
          icon={ChevronLeft}
          disabled={start === 0}
          onClick={() => setPage(Math.max(0, Math.min(page, pages - 1) - 1))}
        />
        <IconButton
          label="→"
          icon={ChevronRight}
          disabled={start + 12 >= trace.signals.length}
          onClick={() => setPage(Math.min(pages - 1, page + 1))}
        />
        <input
          type="range"
          min={72}
          max={240}
          step={8}
          value={cell}
          aria-label={labels.zoom}
          onChange={(e) => setCell(Number(e.target.value))}
        />
      </div>
      <div
        className="waveform"
        role="region"
        aria-label={labels.signals}
        tabIndex={0}
      >
        <div className="wave-row">
          <strong>#</strong>
          <div className="wave-cycles" style={{ width }}>
            {trace.samples.map((sample, index) => (
              <button
                key={index}
                style={{ width: cell }}
                disabled={disabled}
                aria-label={`${labels.sample} ${index + 1}`}
                aria-pressed={trace.selected === index}
                onClick={() => choose(index)}
              >
                {index + 1}
                {sample.reset ? " · RST" : ""}
              </button>
            ))}
          </div>
        </div>
        {signals.map((signal) => (
          <div className="wave-row" key={signal.id}>
            <strong title={`${signal.name} · ${signal.width}u`}>
              {signal.name}
              <small>{signal.width}u</small>
            </strong>
            <svg
              width={width}
              height={38}
              viewBox={`0 0 ${width} 38`}
              role="img"
              aria-label={signal.name}
              onClick={(event) => {
                if (!disabled) {
                  const x =
                    event.clientX -
                    event.currentTarget.getBoundingClientRect().left;
                  choose(
                    Math.min(
                      trace.samples.length - 1,
                      Math.max(0, Math.floor(x / cell)),
                    ),
                  );
                }
              }}
            >
              <rect
                className="wave-cursor"
                x={trace.selected * cell}
                width={cell}
                height={38}
              />
              {trace.samples.map((_, index) => {
                const value = sampleValue(trace, index, signal);
                const prior = index
                  ? sampleValue(trace, index - 1, signal)
                  : value;
                const x = index * cell;
                const y = BigInt(value) === 0n ? 29 : 9;
                return (
                  <g key={index}>
                    <title>
                      {signal.name} · {labels.sample} {index + 1} ·{" "}
                      {formatValue(value, trace.radix, signal.width)}
                    </title>
                    {signal.width === 1 ? (
                      <path
                        d={`M ${x} ${BigInt(prior) === 0n ? 29 : 9} V ${y} H ${x + cell}`}
                      />
                    ) : (
                      <>
                        <path
                          d={`M ${x} 19 L ${x + 5} 8 H ${x + cell - 5} L ${x + cell} 19 L ${x + cell - 5} 30 H ${x + 5} Z`}
                        />
                        <text
                          x={x + 9}
                          y={24}
                          textLength={Math.min(
                            cell - 18,
                            formatValue(value, trace.radix, signal.width)
                              .length * 7.5,
                          )}
                          lengthAdjust="spacingAndGlyphs"
                        >
                          {formatValue(value, trace.radix, signal.width)}
                        </text>
                      </>
                    )}
                  </g>
                );
              })}
            </svg>
          </div>
        ))}
      </div>
    </>
  );
}
