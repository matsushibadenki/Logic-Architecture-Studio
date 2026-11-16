import { useRef, useState } from "react";
import { rpc } from "../store";
import { translation } from "../i18n";
import type { Locale } from "../types";

type Job = {
  id: string;
  status: "running" | "cancelling" | "cancelled" | "completed" | "failed";
  cancellable: boolean;
  result?: unknown;
  error?: string;
};

export function useEdaJob() {
  const active = useRef(false);
  const [busy, setBusy] = useState(false);
  const [job, setJob] = useState<Job>();
  const [error, setError] = useState("");
  async function run<T>(method: string, args: Record<string, unknown>) {
    if (active.current) return;
    active.current = true;
    setBusy(true);
    setError("");
    setJob(undefined);
    try {
      let current = await rpc<Job>("job.start", { method, args });
      setJob(current);
      while (current.status === "running" || current.status === "cancelling") {
        await new Promise((resolve) => setTimeout(resolve, 150));
        current = await rpc<Job>("job.status", { id: current.id });
        setJob(current);
      }
      if (current.status === "failed") setError(current.error ?? "");
      if (current.status === "completed") return current.result as T;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      active.current = false;
      setBusy(false);
    }
  }

  async function cancel() {
    if (!job?.cancellable) return;
    try {
      await rpc("job.cancel", { id: job.id });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  return { job, busy, error, run, cancel };
}

const statuses = {
  running: "jobRunning",
  cancelling: "jobCancelling",
  cancelled: "jobCancelled",
  completed: "jobCompleted",
  failed: "jobFailed",
} as const;

export function EdaJobStatus({
  state,
  locale,
}: {
  state: ReturnType<typeof useEdaJob>;
  locale: Locale;
}) {
  const t = translation(locale);
  return (
    <>
      {state.job && (
        <div className="trace-toolbar">
          <span role="status">{t(statuses[state.job.status])}</span>
          <code title={state.job.id}>{state.job.id}</code>
          {state.job.cancellable && (
            <button onClick={() => void state.cancel()}>
              {t("cancelJob")}
            </button>
          )}
        </div>
      )}
      {state.error && <p role="alert">{state.error}</p>}
    </>
  );
}
