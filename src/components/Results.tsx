import { EdaJobStatus, useEdaJob } from "./EdaJob";
import { useEffect, useId, useState } from "react";
import {
  CheckCircle2,
  Clock3,
  AlertCircle,
  Play,
  RefreshCw,
  ArrowUpRight,
  GitCompareArrows,
} from "lucide-react";
import { action, rpc, select } from "../store";
import { translation } from "../i18n";
import type { Snapshot } from "../types";
import { Hint, IconButton } from "./ui";
import { Simulation } from "./Simulation";
import { PatchDiff, type PatchPreview } from "./PatchDiff";

type FormalResult = {
  revision: number;
  status: "proved" | "counterexample" | "inconclusive";
  width: number;
  counterexample: Record<string, string> | null;
  artifact_directory: string;
  error: string | null;
  log: string;
};

type RtlResult = {
  backend: "icarus" | "verilator";
  revision: number;
  total: number;
  rtl_passed: number;
  contract_passed: number;
  success: boolean;
  stage: string;
  error?: string;
  log: string;
  failures: string[];
  rtl_failures?: string[];
  artifact_directory: string;
};
function RevisionComparison({
  revision,
  t,
}: {
  revision: number;
  t: ReturnType<typeof translation>;
}) {
  type Catalog = {
    revision: number;
    snapshots: { revision: number; label: string | null }[];
  };
  const comparisonId = useId();
  const [catalog, setCatalog] = useState<Catalog>();
  const [from, setFrom] = useState<number>();
  const [to, setTo] = useState(revision);
  const [preview, setPreview] = useState<PatchPreview>();
  const [message, setMessage] = useState("");
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let mounted = true;
    void rpc<Catalog>("revision.list_snapshots")
      .then((result) => {
        if (!mounted || result.revision !== revision) return;
        setCatalog(result);
        setFrom(result.snapshots[1]?.revision);
      })
      .catch((error) => {
        if (mounted) setMessage(String(error));
      });
    return () => {
      mounted = false;
    };
  }, [revision]);
  async function compare() {
    if (from === undefined) return;
    setLoading(true);
    setMessage("");
    setPreview(undefined);
    try {
      const result = await rpc<
        ({ available: true } & PatchPreview) | { available: false }
      >("revision.diff", {
        compact: true,
        expected_revision: revision,
        from_revision: from,
        to_revision: to,
      });
      if (result.available) setPreview(result);
      else setMessage(t("revisionSnapshotUnavailable"));
    } catch (error) {
      setMessage(String(error));
    } finally {
      setLoading(false);
    }
  }
  return (
    <section className="revision-comparison">
      {catalog && catalog.snapshots.length > 1 && (
        <div className="revision-comparison-controls">
          {(
            [
              ["revisionFrom", from, setFrom],
              ["revisionTo", to, setTo],
            ] as const
          ).map(([label, selected, set]) => (
            <div key={label}>
              <label htmlFor={`${comparisonId}-${label}`}>{t(label)}</label>
              <select
                id={`${comparisonId}-${label}`}
                value={selected}
                disabled={loading}
                onChange={(event) => {
                  set(Number(event.currentTarget.value));
                  setPreview(undefined);
                  setMessage("");
                }}
              >
                {catalog.snapshots.map(({ revision, label }) => (
                  <option key={revision} value={revision}>
                    r{String(revision).padStart(3, "0")}
                    {label ? ` · ${label}` : ""}
                  </option>
                ))}
              </select>
            </div>
          ))}
          <button
            type="button"
            disabled={loading || from === undefined || from === to}
            onClick={() => void compare()}
          >
            <GitCompareArrows size={14} />
            {t("compareRevisions")}
          </button>
        </div>
      )}
      {catalog?.snapshots.length === 1 && (
        <p>{t("revisionSnapshotUnavailable")}</p>
      )}
      {message && <p role="status">{message}</p>}
      {preview && <PatchDiff preview={preview} t={t} />}
    </section>
  );
}
export function Results({
  snapshot,
  busy,
}: {
  snapshot: Snapshot;
  busy: boolean;
}) {
  const t = translation(snapshot.locale);
  const [tab, setTab] = useState<
    "verification" | "history" | "tools" | "simulation"
  >("verification");
  const [tools, setTools] = useState<{ name: string; installed: boolean }[]>(
    [],
  );
  const [formal, setFormal] = useState<FormalResult | null>(null);
  const [rtl, setRtl] = useState<RtlResult | null>(null);
  const [log, setLog] = useState("");
  const r = snapshot.report;
  const job = useEdaJob();
  useEffect(() => {
    if (tab === "tools")
      void rpc<typeof tools>("tools.status")
        .then(setTools)
        .catch(() => setTools([]));
  }, [tab]);
  return (
    <div className="results-content">
      <div className="result-tabs" role="tablist">
        {(["verification", "history", "simulation", "tools"] as const).map(
          (x) => (
            <button
              role="tab"
              aria-selected={tab === x}
              key={x}
              onClick={() => setTab(x)}
            >
              {t(x)}
              {x === "verification" && snapshot.diagnostics.length ? (
                <span className="tab-count">{snapshot.diagnostics.length}</span>
              ) : null}
            </button>
          ),
        )}
        <span className="toolbar-spacer" />
        <Hint text={t("contract")} />
      </div>
      {tab === "verification" ? (
        <div className="verification-content">
          {r ? (
            <>
              {r.revision !== snapshot.revision && (
                <div className="stale-banner" role="status">
                  <strong>{t("staleResults")}</strong>
                  <button
                    disabled={busy}
                    onClick={() => void action("verify.run_tests")}
                  >
                    <RefreshCw size={14} />
                    {t("rerun")}
                  </button>
                </div>
              )}
              <div
                className={`report-summary ${r.revision !== snapshot.revision ? "stale-data" : ""}`}
              >
                {r.passed === r.total ? (
                  <CheckCircle2 size={22} className="success" />
                ) : (
                  <AlertCircle size={22} className="warning" />
                )}
                <strong>{t(r.passed === r.total ? "passed" : "failed")}</strong>
                <code>
                  {r.passed} / {r.total}
                </code>
                <span>{t("tests")}</span>
                <span
                  className={`freshness ${r.revision === snapshot.revision ? "" : "warning"}`}
                >
                  {t(r.revision === snapshot.revision ? "current" : "stale")} ·
                  r{r.revision}
                </span>
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => void action("verify.run_tests")}
                >
                  <RefreshCw size={13} />
                  {t("run")}
                </button>
              </div>
              <div className="report-engine">
                {t("behavioral")} · Rust · ALU / explicit vectors
              </div>
              {r.failures.length ? (
                <pre className="failure-log">{r.failures.join("\n")}</pre>
              ) : null}
            </>
          ) : (
            <div className="not-run">
              <Clock3 size={23} />
              <div>
                <strong>{t("notRun")}</strong>
                <small>
                  {t("behavioral")} · {snapshot.document.name}
                </small>
              </div>
              <button
                className="text-button"
                disabled={busy}
                onClick={() => void action("verify.run_tests")}
              >
                <Play size={13} />
                {t("run")}
              </button>
            </div>
          )}
          {snapshot.diagnostics.length ? (
            <div className="diagnostics">
              {snapshot.diagnostics.map((d, i) => (
                <button key={i} onClick={() => void select(d.node)}>
                  <AlertCircle size={13} />
                  <code>{d.message}</code>
                  <span>{t("issues")}</span>
                  <ArrowUpRight size={12} />
                </button>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      {tab === "history" ? (
        <div className="history-list">
          {snapshot.revision > 0 && (
            <RevisionComparison
              key={snapshot.revision}
              revision={snapshot.revision}
              t={t}
            />
          )}
          {snapshot.history.length ? (
            [...snapshot.history].reverse().map((h) => (
              <div key={h.revision}>
                <code>r{String(h.revision).padStart(3, "0")}</code>
                <span>{h.label}</span>
                <small>{h.actor}</small>
              </div>
            ))
          ) : (
            <p className="muted">{t("noHistory")}</p>
          )}
        </div>
      ) : null}
      {tab === "tools" ? (
        <div className="tool-list">
          <EdaJobStatus state={job} locale={snapshot.locale} />
          {tools.map((tool) => (
            <div key={tool.name}>
              <span
                className={`status-dot ${tool.installed ? "" : "unavailable"}`}
              />
              <code>{tool.name}</code>
              <span>{t(tool.installed ? "installed" : "missing")}</span>
              {["iverilog", "verilator"].includes(tool.name) ? (
                <IconButton
                  label={`${tool.name} · ${t("verifyRtl")}`}
                  icon={Play}
                  disabled={
                    !tool.installed ||
                    (tool.name === "iverilog" &&
                      !tools.some(
                        (entry) => entry.name === "vvp" && entry.installed,
                      )) ||
                    busy ||
                    job.busy
                  }
                  onClick={() => {
                    void job
                      .run<RtlResult>("verify.rtl", {
                        expected_revision: snapshot.revision,
                        engine:
                          tool.name === "verilator" ? "verilator" : "icarus",
                      })
                      .then((result) => {
                        if (result) setRtl(result);
                      });
                  }}
                />
              ) : null}
              {tool.name === "yosys" && (
                <IconButton
                  label={t("proveAlu")}
                  icon={CheckCircle2}
                  disabled={!tool.installed || busy || job.busy}
                  onClick={async () => {
                    const result = await job.run<FormalResult>(
                      "verify.formal",
                      {
                        expected_revision: snapshot.revision,
                      },
                    );
                    if (result) setFormal(result);
                  }}
                />
              )}
              {tool.name === "yosys" ? (
                <button
                  className="text-button"
                  disabled={!tool.installed || busy || job.busy}
                  onClick={async () => {
                    const r = await job.run<{ log: string }>("synthesis.run", {
                      expected_revision: snapshot.revision,
                    });
                    if (r) setLog(r.log);
                  }}
                >
                  {t("synthesize")}
                </button>
              ) : null}
            </div>
          ))}
          {formal && (
            <div className="rtl-test-result" role="status">
              <div className="rtl-test-summary">
                <strong>
                  {t(
                    formal.status === "proved"
                      ? "proved"
                      : formal.status === "counterexample"
                        ? "counterexample"
                        : "inconclusive",
                  )}
                </strong>
                <code>Yosys SAT · r{formal.revision}</code>
                {formal.revision !== snapshot.revision && (
                  <span className="stale">{t("stale")}</span>
                )}
                <Hint text={t("formalInfo")} />
              </div>
              <span>ALU · {formal.width} bit</span>
              {formal.counterexample && (
                <pre>
                  {Object.entries(formal.counterexample)
                    .map(([name, value]) => `${name} = ${value}`)
                    .join("\n")}
                </pre>
              )}
              <details>
                <summary>{t("details")}</summary>
                <code className="recovery-path">
                  {formal.artifact_directory}
                </code>
                <pre>
                  {[formal.error, formal.log].filter(Boolean).join("\n")}
                </pre>
              </details>
            </div>
          )}
          {rtl && (
            <div className="rtl-test-result" role="status">
              <div className="rtl-test-summary">
                {rtl.success ? (
                  <CheckCircle2 size={16} />
                ) : (
                  <AlertCircle size={16} />
                )}
                <strong>{t(rtl.success ? "passed" : "failed")}</strong>
                <code>
                  {rtl.backend === "verilator" ? "Verilator" : "Icarus"} · r
                  {rtl.revision}
                </code>
                {rtl.revision !== snapshot.revision && (
                  <span className="stale">{t("stale")}</span>
                )}
                <Hint text={t("rtlInfo")} />
              </div>
              <span>
                {t("rtlMatch")} {rtl.rtl_passed}/{rtl.total} ·{" "}
                {t("rtlContract")} {rtl.contract_passed}/{rtl.total}
              </span>
              <details>
                <summary>{t("details")}</summary>
                <code className="recovery-path">{rtl.artifact_directory}</code>
                <pre>
                  {[
                    rtl.error,
                    ...rtl.failures,
                    ...(rtl.rtl_failures ?? []),
                    rtl.log,
                  ]
                    .filter(Boolean)
                    .join("\n")}
                </pre>
              </details>
            </div>
          )}
          {log ? <pre>{log}</pre> : null}
        </div>
      ) : null}
      {tab === "simulation" ? (
        <Simulation snapshot={snapshot} busy={busy} />
      ) : null}
    </div>
  );
}
