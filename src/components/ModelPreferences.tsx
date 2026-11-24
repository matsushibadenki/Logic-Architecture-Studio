import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { translation } from "../i18n";
import { native, rpc, refresh, useStudio } from "../store";
import { PatchDiff, type PatchPreview } from "./PatchDiff";
import type { Operation } from "../types";
import { Hint } from "./ui";

type Provider = "open_ai" | "gemini" | "local";
type Proposal = {
  patch: Record<string, unknown> & {
    expected_revision: number;
    operations: Operation[];
  };
  preview: PatchPreview;
};

type CheckResult =
  | { error: string }
  | {
      report: {
        passed: number;
        total: number;
        failures: {
          cycle: number;
          signal: string;
          expected: string;
          actual: string;
        }[];
      };
    };

type Comparison = {
  revision: number;
  testbench_version: number;
  baseline: CheckResult;
  candidate: CheckResult;
};

type Exploration = {
  used: number;
  budget: number;
  stop: "attempts" | "time";
  candidates: (Proposal & { comparison: Comparison })[];
  rejected: { attempt: number; error: string }[];
};

export default function ModelPreferences({
  t,
  revision,
}: {
  t: ReturnType<typeof translation>;
  revision: number;
}) {
  const { busy, snapshot } = useStudio();
  const bench = snapshot?.testbench;
  const hasAssertions = bench?.rows.some(
    (row) => Object.keys(row.expected).length,
  );
  const [provider, setProvider] = useState<Provider>("local");
  const [endpoint, setEndpoint] = useState("http://127.0.0.1:11434/v1");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [instruction, setInstruction] = useState("");
  const [proposal, setProposal] = useState<Proposal>();
  const [budget, setBudget] = useState(3);
  const [exploration, setExploration] = useState<Exploration>();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [applying, setApplying] = useState(false);
  const [checking, setChecking] = useState(false);
  const [comparison, setComparison] = useState<Comparison>();
  const stale =
    proposal &&
    (revision !== proposal.patch.expected_revision ||
      (exploration && comparison?.testbench_version !== bench?.version));

  const currentComparison =
    comparison?.revision === revision &&
    comparison.testbench_version === bench?.version
      ? comparison
      : undefined;

  async function checkProposal() {
    if (!proposal || !bench) return;
    setChecking(true);
    setError("");
    setComparison(undefined);
    try {
      setComparison(
        await rpc<Comparison>("design.check_proposal", {
          patch: proposal.patch,
          expected_version: bench.version,
        }),
      );
    } catch (failure) {
      setError(String(failure));
    } finally {
      setChecking(false);
    }
  }

  async function generate(explore: boolean) {
    setPending(true);
    setError("");
    setProposal(undefined);
    setComparison(undefined);
    setExploration(undefined);
    try {
      const request = {
        provider,
        endpoint: provider === "local" ? endpoint : undefined,
        model,
        apiKey,
        instruction,
      };
      if (explore) {
        setExploration(
          await invoke<Exploration>("model_explore", {
            request,
            attempts: budget,
          }),
        );
      } else {
        setProposal(await invoke<Proposal>("model_propose", { request }));
      }
    } catch (failure) {
      setError(String(failure));
    } finally {
      setPending(false);
    }
  }

  return (
    <section
      className="preferences-pane"
      role="tabpanel"
      id="models-panel"
      aria-labelledby="models-tab"
    >
      <header className="preferences-heading">
        <h1>{t("modelTitle")}</h1>
        <Hint text={t("modelHint")} />
      </header>
      <div className="preferences-scroll">
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const submitter = (event.nativeEvent as SubmitEvent).submitter;
            void generate(submitter?.getAttribute("value") === "explore");
          }}
        >
          <fieldset
            className="model-fields"
            disabled={pending || applying || checking || !native}
            onChange={() => {
              setProposal(undefined);
              setExploration(undefined);
            }}
          >
            <label>
              {t("modelProvider")}
              <select
                value={provider}
                onChange={(event) => {
                  setProvider(event.target.value as Provider);
                  setApiKey("");
                  setModel("");
                }}
              >
                <option value="local">{t("modelLocal")}</option>
                <option value="open_ai">OpenAI</option>
                <option value="gemini">Google Gemini</option>
              </select>
            </label>
            {provider === "local" && (
              <label>
                {t("modelEndpoint")}
                <input
                  type="url"
                  value={endpoint}
                  onChange={(event) => setEndpoint(event.target.value)}
                  required
                />
              </label>
            )}
            <label>
              {t("modelId")}
              <input
                value={model}
                maxLength={256}
                onChange={(event) => setModel(event.target.value)}
                required
              />
            </label>
            <label>
              {provider === "local" ? t("modelOptionalKey") : "API key"}
              <input
                type="password"
                autoComplete="off"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                required={provider !== "local"}
              />
            </label>
            <label>
              {t("modelInstruction")}
              <textarea
                rows={4}
                maxLength={4000}
                value={instruction}
                onChange={(event) => setInstruction(event.target.value)}
                required
              />
            </label>
            <div className="model-fields">
              <button className="primary" type="submit">
                {t(pending ? "modelGenerating" : "modelSend")}
              </button>
              <label>
                {t("modelBudget")}
                <select
                  value={budget}
                  onChange={(event) => setBudget(Number(event.target.value))}
                >
                  <option value={2}>2</option>
                  <option value={3}>3</option>
                  <option value={4}>4</option>
                </select>
              </label>
              <button type="submit" value="explore" disabled={!hasAssertions}>
                {t("modelExplore")}
              </button>
              <Hint
                text={t(
                  hasAssertions ? "modelExploreHint" : "modelCheckNeedsStimuli",
                )}
              />
            </div>
          </fieldset>
        </form>
        {!native && <p>{t("modelDesktop")}</p>}
        {error && <p role="alert">{error}</p>}
        {exploration && (
          <section
            className="model-preview"
            aria-label={t("modelExploreResults")}
          >
            <p role="status">
              {t(
                exploration.stop === "time"
                  ? "modelStoppedTime"
                  : "modelStoppedBudget",
              )}
              {" · "}
              {exploration.used}/{exploration.budget}
            </p>
            {exploration.candidates.map((candidate, index) => {
              const result = candidate.comparison.candidate;
              return (
                <button
                  key={index}
                  aria-pressed={proposal === candidate}
                  disabled={applying}
                  onClick={() => {
                    setProposal(candidate);
                    setComparison(candidate.comparison);
                  }}
                >
                  {candidate.preview.label} ·{" "}
                  {"report" in result
                    ? `${result.report.passed}/${result.report.total}`
                    : t("modelCannotCompare")}
                </button>
              );
            })}
            {exploration.rejected.map(({ attempt, error }) => (
              <p key={attempt}>
                {t("modelRejected")} {attempt} · {error}
              </p>
            ))}
          </section>
        )}
        {proposal && (
          <div className="model-preview">
            <PatchDiff preview={proposal.preview} t={t} />
            {stale && <p role="status">{t("modelStale")}</p>}
            <button
              type="button"
              disabled={
                checking || applying || stale || !hasAssertions || !!exploration
              }
              onClick={() => void checkProposal()}
            >
              {t(checking ? "modelChecking" : "modelCheck")}
            </button>
            {!hasAssertions && <p>{t("modelCheckNeedsStimuli")}</p>}
            {currentComparison && (
              <div className="mcp-settings" role="status">
                {(["baseline", "candidate"] as const).map((side) => {
                  const result = currentComparison[side];
                  return (
                    <section key={side}>
                      <h3>
                        {t(
                          side === "baseline"
                            ? "modelBaseline"
                            : "modelCandidate",
                        )}
                      </h3>
                      {"error" in result ? (
                        <p>{result.error}</p>
                      ) : (
                        <>
                          <p>
                            {t("modelPassed")} · {result.report.passed}/
                            {result.report.total}
                          </p>
                          {result.report.failures.length > 0 && (
                            <ul>
                              {result.report.failures.map((failure) => (
                                <li key={`${failure.cycle}:${failure.signal}`}>
                                  {t("modelCycle")} {failure.cycle + 1} ·{" "}
                                  {failure.signal} · {t("modelExpected")}{" "}
                                  {failure.expected} · {t("modelActual")}{" "}
                                  {failure.actual}
                                </li>
                              ))}
                            </ul>
                          )}
                        </>
                      )}
                    </section>
                  );
                })}
              </div>
            )}
            <button
              className="primary"
              disabled={
                busy ||
                applying ||
                checking ||
                stale ||
                !proposal.patch.operations.length
              }
              onClick={async () => {
                setApplying(true);
                setError("");
                try {
                  if (exploration && comparison) {
                    await rpc("view.command", {
                      method: "design.apply_patch",
                      args: proposal.patch,
                      expected_testbench_version: comparison.testbench_version,
                    });
                  } else {
                    await rpc("design.apply_patch", proposal.patch);
                  }
                  setExploration(undefined);
                  setProposal(undefined);
                  await refresh();
                } catch (failure) {
                  setError(String(failure));
                } finally {
                  setApplying(false);
                }
              }}
            >
              {t("modelApply")}
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
