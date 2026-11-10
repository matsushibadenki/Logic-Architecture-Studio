import { useEffect, useState } from "react";
import {
  Copy,
  Download,
  FlaskConical,
  Play,
  Check,
  GitBranch,
  Info,
} from "lucide-react";
import { action, rpc } from "../store";
import { translation } from "../i18n";
import type { Snapshot } from "../types";
import { Hint, IconButton } from "./ui";
export function download(source: string, name: string) {
  const url = URL.createObjectURL(new Blob([source], { type: "text/plain" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export function RtlView({ snapshot }: { snapshot: Snapshot }) {
  const t = translation(snapshot.locale);
  const [source, setSource] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    let live = true;
    setSource("");
    setError("");
    void rpc<{ source: string }>("rtl.generate")
      .then((r) => {
        if (live) setSource(r.source);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [snapshot.revision]);
  return (
    <div className="rtl-view">
      <div className="view-header">
        <span className="file-icon">V</span>
        <strong>{snapshot.document.name}.v</strong>
        <span className="muted">r{snapshot.revision}</span>
        <span className="toolbar-spacer" />
        <IconButton
          label={copied ? t("copied") : t("copy")}
          icon={copied ? Check : Copy}
          disabled={!source}
          onClick={() => {
            void navigator.clipboard
              .writeText(source)
              .then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              })
              .catch(() => setError("Clipboard unavailable"));
          }}
        />
        <IconButton
          label={t("save")}
          icon={Download}
          disabled={!source}
          onClick={() => download(source, `${snapshot.document.name}.v`)}
        />
      </div>
      {error ? (
        <div className="empty">
          <Info size={24} />
          <p>{t("noRtl")}</p>
          <code>{error}</code>
        </div>
      ) : (
        <div className="code-editor">
          {source.split("\n").map((line, i) => (
            <div key={i}>
              <span className="line-number">{i + 1}</span>
              <code
                className={
                  line.startsWith("//")
                    ? "code-comment"
                    : line.trim().startsWith("assign")
                      ? "code-assign"
                      : ""
                }
              >
                {line || " "}
              </code>
            </div>
          ))}
        </div>
      )}
      <div className="view-footer">{t("source")} · Verilog-2005</div>
    </div>
  );
}
export function Experiments({
  snapshot,
  busy,
}: {
  snapshot: Snapshot;
  busy: boolean;
}) {
  const t = translation(snapshot.locale);
  async function create() {
    await action("experiment.create", {
      expected_revision: snapshot.revision,
      name: `ALU / r${snapshot.revision}`,
      variants: [
        { name: t("baseline"), operations: [] },
        {
          name: t("subtract"),
          operations: [{ op: "update_node", id: "add", kind: "sub" }],
        },
        {
          name: t("orVariant"),
          operations: [{ op: "update_node", id: "bit_xor", kind: "or" }],
        },
      ],
    });
  }
  const canCreate =
    snapshot.document.nodes.some((n) => n.id === "add") &&
    snapshot.document.nodes.some((n) => n.id === "bit_xor");
  return (
    <div className="experiments-view">
      <div className="view-header">
        <FlaskConical size={17} />
        <strong>{t("experiments")}</strong>
        <Hint text={t("experimentHint")} />
        <span className="toolbar-spacer" />
        <button
          className="text-button"
          disabled={busy || !canCreate}
          onClick={() => void create()}
        >
          <GitBranch size={14} />
          {t("createExperiment")}
        </button>
      </div>
      {!snapshot.experiments.length ? (
        <div className="empty experiment-empty">
          <FlaskConical size={36} />
          <h2>{t("noExperiments")}</h2>
          <span>{t("branch")}</span>
          <button
            className="primary"
            disabled={busy || !canCreate}
            onClick={() => void create()}
          >
            {t("createExperiment")}
          </button>
        </div>
      ) : (
        [...snapshot.experiments].reverse().map((exp) => (
          <section className="experiment" key={exp.id}>
            <header>
              <GitBranch size={16} />
              <h3>{exp.name}</h3>
              <code>r{exp.base_revision}</code>
              {exp.base_revision !== snapshot.revision ? (
                <span className="warning">{t("stale")}</span>
              ) : null}
              <span className="toolbar-spacer" />
              <button
                className="text-button"
                disabled={busy}
                onClick={() => void action("experiment.run", { id: exp.id })}
              >
                <Play size={13} />
                {t("runExperiment")}
              </button>
            </header>
            <div className="comparison-scroll">
              <table>
                <thead>
                  <tr>
                    <th>{t("candidate")}</th>
                    <th>{t("verification")}</th>
                    <th>{t("nodes")}</th>
                    <th>{t("area")}</th>
                    <th>{t("delay")}</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {exp.candidates.map((c) => (
                    <tr key={c.id}>
                      <td>
                        <strong>{c.name}</strong>
                        <small>{c.id}</small>
                      </td>
                      <td
                        className={
                          c.report
                            ? c.report.passed === c.report.total
                              ? "success"
                              : "warning"
                            : ""
                        }
                      >
                        {c.report ? (
                          <>
                            {t(
                              c.report.passed === c.report.total
                                ? "passed"
                                : "failed",
                            )}{" "}
                            <small>
                              {c.report.passed}/{c.report.total}
                            </small>
                          </>
                        ) : (
                          t("notRun")
                        )}
                      </td>
                      <td>{c.document.nodes.length}</td>
                      <td className="muted" title={t("notMeasured")}>
                        —
                      </td>
                      <td className="muted" title={t("notMeasured")}>
                        —
                      </td>
                      <td>
                        <button
                          className="text-button"
                          disabled={
                            busy ||
                            !c.report ||
                            c.report.passed !== c.report.total ||
                            exp.base_revision !== snapshot.revision
                          }
                          onClick={() =>
                            void action("experiment.adopt", {
                              id: exp.id,
                              candidate_id: c.id,
                              expected_revision: snapshot.revision,
                            })
                          }
                        >
                          {t("adopt")}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        ))
      )}
    </div>
  );
}
