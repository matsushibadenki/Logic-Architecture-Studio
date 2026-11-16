import { isSequential } from "../types";
import { StimulusTable } from "./StimulusTable";
import { useEffect, useRef, useState } from "react";
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
  const [inputMode, setInputMode] = useState<"manual" | "stimuli">("stimuli");
  const [values, setValues] = useState<Record<string, string>>({});
  const [mode, setMode] = useState<"table" | "wave">("table");
  const [wavePage, setWavePage] = useState(0);
  const [jump, setJump] = useState<{ traceId: string; signalId: string }>();
  const sequential = isSequential(snapshot.document);
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
  async function jumpToFailure(failure: AssertionFailure) {
    if (!trace || !current) return;
    const signalIndex = trace.signals.findIndex(
      (signal) => signal.kind === "output" && signal.name === failure.signal,
    );
    if (signalIndex < 0) return;
    const result = await choose(failure.cycle);
    if (result?.id !== trace.id || result.selected !== failure.cycle) return;
    setWavePage(Math.floor(signalIndex / SIGNALS_PER_PAGE));
    setJump({ traceId: trace.id, signalId: trace.signals[signalIndex].id });
    setMode("wave");
  }
  async function rerun() {
    if (!trace) return;
    if (trace.testbench_version != null) {
      await simulate(
        trace.external ? "simulation.rtl_testbench" : "simulation.testbench",
        {
          ...(trace.external ? { engine: trace.external.engine } : {}),
          expected_revision: snapshot.revision,
          expected_version: snapshot.testbench?.version ?? 0,
        },
      );
    } else if (sequential)
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
      <div className="trace-toolbar" role="tablist" aria-label={t("inputs")}>
        {(["stimuli", "manual"] as const).map((m) => (
          <button
            key={m}
            role="tab"
            aria-selected={inputMode === m}
            className={inputMode === m ? "active" : ""}
            onClick={() => setInputMode(m)}
          >
            {t(m === "stimuli" ? "stimuli" : "manualInputs")}
          </button>
        ))}
      </div>
      <div hidden={inputMode !== "stimuli"}>
        <StimulusTable snapshot={snapshot} busy={busy} />
      </div>
      {inputMode === "manual" && (
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
                <small>
                  {n.width}
                  {n.signed ? "s" : "u"}
                </small>
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
      )}
      {trace && (
        <>
          {(!current ||
            (trace.testbench_version != null &&
              trace.testbench_version !== snapshot.testbench?.version)) && (
            <div className="stale-banner" role="status">
              <strong>{t(!current ? "staleResults" : "stimuliChanged")}</strong>
              <button disabled={busy} onClick={() => void rerun()}>
                <RefreshCw size={14} />
                {t("rerun")}
              </button>
            </div>
          )}
          <div
            className={`stimulus-checks ${trace.external && !trace.external.success ? "has-failures" : ""}`}
          >
            <strong>
              {trace.external
                ? trace.external.engine === "icarus"
                  ? "Icarus Verilog"
                  : "Verilator"
                : "Rust"}
              {trace.external &&
                ` · ${t("rustComparison")} ${trace.external.passed}/${trace.external.total}`}
            </strong>
            <Hint
              text={t(
                trace.external
                  ? "externalSimulationHint"
                  : "simulationSemantics",
              )}
            />
            {trace.external &&
              trace.external.passed !== trace.external.total && (
                <strong>{t("comparisonFailed")}</strong>
              )}
            {!!trace.external?.failures.length && (
              <div className="stimulus-failures">
                {trace.external.failures.map((f) => (
                  <button
                    key={`${f.cycle}-${f.signal}`}
                    disabled={busy || !current}
                    onClick={() => void choose(f.cycle)}
                  >
                    {t("sample")} {f.cycle + 1} · {f.signal}: Rust {f.expected}{" "}
                    / RTL {f.actual}
                  </button>
                ))}
              </div>
            )}
          </div>
          {trace.checks && (
            <div
              className={`stimulus-checks ${trace.checks.failures.length ? "has-failures" : ""}`}
            >
              <strong>
                {t("checks")} ·{" "}
                {trace.checks.total
                  ? `${trace.checks.passed}/${trace.checks.total} ${t("passed")}`
                  : t("noChecks")}
              </strong>
              {trace.checks.failures.length > 0 && (
                <AssertionFailures
                  key={trace.id}
                  report={trace.checks}
                  disabled={busy || !current}
                  t={t}
                  jump={(failure) => void jumpToFailure(failure)}
                />
              )}
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
            <Hint
              text={t(trace.external ? "externalSimulationHint" : "postEdge")}
            />
            <IconButton
              label={t(trace.external ? "exportRtlVcd" : "exportVcd")}
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
                            {signal.width}
                            {signal.signed ? "s" : "u"} ·{" "}
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
                              signal.signed,
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
                page={wavePage}
                setPage={setWavePage}
                jump={current && jump?.traceId === trace.id ? jump : undefined}
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
type AssertionReport = NonNullable<Trace["checks"]>;
type AssertionFailure = AssertionReport["failures"][number];
const SIGNALS_PER_PAGE = 12;

function AssertionFailures({
  report,
  disabled,
  t,
  jump,
}: {
  report: AssertionReport;
  disabled: boolean;
  t: ReturnType<typeof translation>;
  jump: (failure: AssertionFailure) => void;
}) {
  const [signal, setSignal] = useState("");
  const [cycle, setCycle] = useState("");
  const [page, setPage] = useState(0);
  const pageSize = 25;
  const signals = [
    ...new Set(report.failures.map((failure) => failure.signal)),
  ].sort();
  const cycles = [
    ...new Set(report.failures.map((failure) => failure.cycle)),
  ].sort((a, b) => a - b);
  const matches = report.failures.filter(
    (failure) =>
      (!signal || failure.signal === signal) &&
      (!cycle || failure.cycle === Number(cycle)),
  );
  const start = page * pageSize;
  const end = Math.min(start + pageSize, matches.length);
  return (
    <>
      <div className="failure-filters">
        <label>
          {t("signals")}
          <select
            value={signal}
            onChange={(event) => {
              setSignal(event.target.value);
              setPage(0);
            }}
          >
            <option value="">{t("allFailureSignals")}</option>
            {signals.map((name) => (
              <option key={name}>{name}</option>
            ))}
          </select>
        </label>
        <label>
          {t("sample")}
          <select
            value={cycle}
            onChange={(event) => {
              setCycle(event.target.value);
              setPage(0);
            }}
          >
            <option value="">{t("allFailureCycles")}</option>
            {cycles.map((index) => (
              <option key={index} value={index}>
                {index + 1}
              </option>
            ))}
          </select>
        </label>
        <span role="status">
          {matches.length} / {report.total - report.passed}
        </span>
      </div>
      {matches.length > pageSize && (
        <div className="failure-filters">
          <IconButton
            label={t("previousFailurePage")}
            icon={ChevronLeft}
            disabled={page === 0}
            onClick={() => setPage(page - 1)}
          />
          <span role="status">
            {start + 1}–{end} / {matches.length}
          </span>
          <IconButton
            label={t("nextFailurePage")}
            icon={ChevronRight}
            disabled={end >= matches.length}
            onClick={() => setPage(page + 1)}
          />
        </div>
      )}
      {matches.length === 0 ? (
        <small>{t("noMatchingFailures")}</small>
      ) : (
        <div className="stimulus-failures">
          {matches.slice(start, end).map((failure) => (
            <button
              key={`${failure.cycle}-${failure.signal}`}
              disabled={disabled}
              onClick={() => jump(failure)}
            >
              {t("sample")} {failure.cycle + 1} · {failure.signal} ·{" "}
              {t("expectedValue")} {failure.expected} / {t("actualValue")}{" "}
              {failure.actual}
            </button>
          ))}
        </div>
      )}
    </>
  );
}

function Waveform({
  trace,
  page,
  setPage,
  jump,
  disabled,
  choose,
  labels,
}: {
  trace: Trace;
  page: number;
  setPage: (page: number) => void;
  jump?: { signalId: string };
  disabled: boolean;
  choose: (index: number) => void;
  labels: { sample: string; signals: string; zoom: string };
}) {
  const [cell, setCell] = useState(112);
  const pages = Math.max(1, Math.ceil(trace.signals.length / SIGNALS_PER_PAGE));
  const start = Math.min(page, pages - 1) * SIGNALS_PER_PAGE;
  const signals = trace.signals.slice(start, start + SIGNALS_PER_PAGE);
  const width = trace.samples.length * cell;
  const viewport = useRef<HTMLDivElement>(null);
  const highlightedRow = useRef<HTMLDivElement>(null);
  useEffect(() => {
    highlightedRow.current?.scrollIntoView({
      block: "nearest",
      inline: "nearest",
    });
  }, [jump]);
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const labelWidth =
      el.querySelector(".wave-row > strong")?.getBoundingClientRect().width ??
      120;
    const visible = Math.max(cell, el.clientWidth - labelWidth);
    const x = trace.selected * cell;
    if (x < el.scrollLeft) el.scrollLeft = x;
    else if (x + cell > el.scrollLeft + visible)
      el.scrollLeft = x + cell - visible;
  }, [trace.selected, cell, trace.id, jump, start]);

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
          disabled={start + SIGNALS_PER_PAGE >= trace.signals.length}
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
        ref={viewport}
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
          <div
            className={`wave-row${jump?.signalId === signal.id ? " wave-row-highlight" : ""}`}
            ref={jump?.signalId === signal.id ? highlightedRow : undefined}
            key={signal.id}
          >
            <strong
              title={`${signal.name} · ${signal.width}${signal.signed ? "s" : "u"}`}
            >
              {signal.name}
              <small>
                {signal.width}
                {signal.signed ? "s" : "u"}
              </small>
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
                      {formatValue(
                        value,
                        trace.radix,
                        signal.width,
                        signal.signed,
                      )}
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
                            formatValue(
                              value,
                              trace.radix,
                              signal.width,
                              signal.signed,
                            ).length * 7.5,
                          )}
                          lengthAdjust="spacingAndGlyphs"
                        >
                          {formatValue(
                            value,
                            trace.radix,
                            signal.width,
                            signal.signed,
                          )}
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
