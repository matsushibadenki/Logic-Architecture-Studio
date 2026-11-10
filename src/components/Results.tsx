import { useEffect, useState } from "react";
import {
  CheckCircle2,
  Clock3,
  AlertCircle,
  Play,
  RefreshCw,
  ArrowUpRight,
} from "lucide-react";
import { action, rpc, select } from "../store";
import { translation } from "../i18n";
import type { Snapshot } from "../types";
import { Hint, IconButton } from "./ui";
type RtlResult = {
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
  const [rtl, setRtl] = useState<RtlResult | null>(null);
  const [log, setLog] = useState("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [outputs, setOutputs] = useState<Record<string, string> | null>(null);
  const [outputRevision, setOutputRevision] = useState<number | null>(null);
  const r = snapshot.report;
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
              <div className="report-summary">
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
          {tools.map((tool) => (
            <div key={tool.name}>
              <span
                className={`status-dot ${tool.installed ? "" : "unavailable"}`}
              />
              <code>{tool.name}</code>
              <span>{t(tool.installed ? "installed" : "missing")}</span>
              {tool.name === "iverilog" ? (
                <IconButton
                  label={t("verifyRtl")}
                  icon={Play}
                  disabled={
                    !tool.installed ||
                    !tools.some(
                      (tool) => tool.name === "vvp" && tool.installed,
                    ) ||
                    busy
                  }
                  onClick={() => {
                    void action<RtlResult>("verify.rtl", {
                      expected_revision: snapshot.revision,
                    }).then((result) => {
                      if (result) setRtl(result);
                    });
                  }}
                />
              ) : null}
              {tool.name === "yosys" ? (
                <button
                  className="text-button"
                  disabled={!tool.installed || busy}
                  onClick={async () => {
                    const r = await action<{ log: string }>("synthesis.run");
                    if (r) setLog(r.log);
                  }}
                >
                  {t("synthesize")}
                </button>
              ) : null}
            </div>
          ))}
          {rtl && (
            <div className="rtl-test-result" role="status">
              <div className="rtl-test-summary">
                {rtl.success ? (
                  <CheckCircle2 size={16} />
                ) : (
                  <AlertCircle size={16} />
                )}
                <strong>{t(rtl.success ? "passed" : "failed")}</strong>
                <code>r{rtl.revision}</code>
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
                  <small>{n.width} bit</small>
                </label>
              ))}
            <IconButton
              label={t("evaluate")}
              icon={Play}
              disabled={busy}
              onClick={async () => {
                const inputs = Object.fromEntries(
                  snapshot.document.nodes
                    .filter((n) => n.kind === "input")
                    .map((n) => [n.name, values[n.name] ?? "0"]),
                );
                const result = await action<{
                  outputs: Record<string, string>;
                  revision: number;
                }>("simulation.evaluate", { inputs });
                if (result) {
                  setOutputs(result.outputs);
                  setOutputRevision(result.revision);
                }
              }}
            />
          </div>
          {outputs ? (
            <div className="simulation-outputs">
              {Object.entries(outputs).map(([name, value]) => (
                <span key={name}>
                  <code>{name}</code>
                  <strong>{value}</strong>
                </span>
              ))}
              <small
                className={
                  outputRevision !== snapshot.revision ? "warning" : ""
                }
              >
                r{outputRevision} ·{" "}
                {t(outputRevision === snapshot.revision ? "current" : "stale")}
              </small>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
