import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Eye, EyeOff, Play, Square, X } from "lucide-react";
import { translation } from "../i18n";
import { native } from "../store";
import { Hint } from "./ui";

type Policy = { publish: boolean; session: boolean; write: boolean };
type Request = { id: string; principal: string; tool: string };
type Status = { policy: Policy; endpoint: string | null; pending: Request[] };
type EntityChange = {
  id: string;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
};
type PatchPreview = {
  revision: number;
  label: string;
  nodes: EntityChange[];
  edges: EntityChange[];
  diagnostics: { code: string; node: string | null; message: string }[];
};
type ReviewDetails = {
  arguments: unknown;
  preview?: PatchPreview;
  preview_error?: string;
};
type Review = ReviewDetails & { request: Request };
type Translate = ReturnType<typeof translation>;

function PatchDiff({ preview, t }: { preview: PatchPreview; t: Translate }) {
  const groups = [
    { name: t("mcpDiffNodes"), entries: preview.nodes },
    { name: t("mcpDiffWires"), entries: preview.edges },
  ];
  const display = (value: unknown) =>
    value === undefined
      ? "—"
      : typeof value === "string"
        ? value
        : JSON.stringify(value);
  return (
    <div className="mcp-diff">
      <h3>
        {t("mcpPatchDiff")} · r{preview.revision}
      </h3>
      <p>{preview.label}</p>
      {groups.every(({ entries }) => entries.length === 0) && (
        <p>{t("mcpNoChanges")}</p>
      )}
      {groups
        .filter(({ entries }) => entries.length > 0)
        .map(({ name, entries }) => (
          <section key={name} aria-label={name}>
            <h4>
              {name} ({entries.length})
            </h4>
            {entries.map(({ id, before, after }) => (
              <div className="mcp-diff-entry" key={id}>
                <strong>
                  {id} ·{" "}
                  {t(
                    !before
                      ? "mcpDiffAdded"
                      : !after
                        ? "mcpDiffRemoved"
                        : "mcpDiffChanged",
                  )}
                </strong>
                <dl>
                  {[
                    ...new Set([
                      ...Object.keys(before ?? {}),
                      ...Object.keys(after ?? {}),
                    ]),
                  ]
                    .filter(
                      (field) =>
                        field !== "id" &&
                        JSON.stringify(before?.[field]) !==
                          JSON.stringify(after?.[field]),
                    )
                    .map((field) => (
                      <div key={field}>
                        <dt>{field}</dt>
                        <dd>
                          <del aria-label={t("mcpDiffBefore")}>
                            {display(before?.[field])}
                          </del>
                          <span aria-hidden="true"> → </span>
                          <ins aria-label={t("mcpDiffAfter")}>
                            {display(after?.[field])}
                          </ins>
                        </dd>
                      </div>
                    ))}
                </dl>
              </div>
            ))}
          </section>
        ))}
      {preview.diagnostics.length > 0 && (
        <details>
          <summary>
            {t("mcpPreviewDiagnostics")} ({preview.diagnostics.length})
          </summary>
          <ul>
            {preview.diagnostics.map(({ code, node, message }, index) => (
              <li key={index}>
                {node && `${node} · `}
                {code} · {message}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

export default function McpPreferences({
  documentName,
  t,
}: {
  documentName: string;
  t: ReturnType<typeof translation>;
}) {
  const [status, setStatus] = useState<Status>();
  const [token, setToken] = useState("");
  const [review, setReview] = useState<Review>();
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const actionInFlight = useRef(false);

  useEffect(() => {
    if (!native) return;
    let alive = true;
    let timer: number;
    async function poll() {
      if (!actionInFlight.current) {
        try {
          const value = await invoke<Status>("mcp_status");
          if (alive && !actionInFlight.current) setStatus(value);
        } catch (error) {
          if (alive) setError(String(error));
        }
      }
      if (alive) timer = window.setTimeout(poll, 750);
    }
    void poll();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, []);

  async function update(action: () => Promise<void>) {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await action();
      setStatus(await invoke<Status>("mcp_status"));
    } catch (failure) {
      setError(String(failure));
    } finally {
      actionInFlight.current = false;
      setBusy(false);
    }
  }

  const running = Boolean(status?.endpoint);
  const pending =
    review && status?.pending.some(({ id }) => id === review.request.id)
      ? review
      : undefined;
  const failure = error.startsWith("approval_")
    ? t("mcpApprovalUnavailable")
    : error;

  return (
    <section
      className="preferences-pane"
      role="tabpanel"
      id="mcp-panel"
      aria-labelledby="mcp-tab"
    >
      <header className="preferences-heading">
        <h1>MCP</h1>
        <Hint text={t("mcpHint")} />
      </header>
      <div className="preferences-scroll mcp-preferences">
        {!native && <p>{t("mcpDesktopOnly")}</p>}
        {native && !status && !error && <p>{t("loading")}</p>}
        {status && (
          <>
            <section className="mcp-settings" aria-label={t("mcpPublish")}>
              <label className="mcp-toggle">
                <input
                  type="checkbox"
                  checked={status.policy.publish}
                  disabled={running || busy}
                  onChange={(event) => {
                    const publish = event.currentTarget.checked;
                    void update(() =>
                      invoke("mcp_configure", {
                        policy: { ...status.policy, publish },
                      }),
                    );
                  }}
                />
                <span>
                  {t("mcpPublish")} <strong>{documentName}</strong>
                </span>
              </label>
              <fieldset disabled={running || busy || !status.policy.publish}>
                <legend>{t("mcpPermissions")}</legend>
                <label className="mcp-toggle">
                  <input
                    type="checkbox"
                    checked={status.policy.publish}
                    disabled
                  />
                  {t("mcpRead")}
                </label>
                {(["session", "write"] as const).map((scope) => (
                  <label className="mcp-toggle" key={scope}>
                    <input
                      type="checkbox"
                      checked={status.policy[scope]}
                      onChange={(event) => {
                        const enabled = event.currentTarget.checked;
                        void update(() =>
                          invoke("mcp_configure", {
                            policy: { ...status.policy, [scope]: enabled },
                          }),
                        );
                      }}
                    />
                    {t(scope === "session" ? "mcpSession" : "mcpWrite")}
                  </label>
                ))}
              </fieldset>
            </section>

            <section className="mcp-settings" aria-label={t("mcpConnection")}>
              <div className="mcp-row">
                <strong role="status">
                  {t(running ? "mcpRunning" : "mcpStopped")}
                </strong>
                <button
                  type="button"
                  className={running ? "" : "primary"}
                  disabled={busy || (!running && !status.policy.publish)}
                  onClick={() =>
                    void update(async () => {
                      await invoke(running ? "mcp_stop" : "mcp_start");
                      setToken("");
                      setReview(undefined);
                    })
                  }
                >
                  {running ? <Square size={14} /> : <Play size={14} />}
                  {t(running ? "mcpStop" : "mcpStart")}
                </button>
              </div>
              {status.endpoint && (
                <>
                  <label className="mcp-field">
                    <span>{t("mcpEndpoint")}</span>
                    <input
                      readOnly
                      spellCheck={false}
                      value={status.endpoint}
                      onFocus={(event) => event.currentTarget.select()}
                    />
                  </label>
                  <div className="mcp-field">
                    <label htmlFor="mcp-token">{t("mcpToken")}</label>
                    <div className="mcp-row">
                      <input
                        id="mcp-token"
                        readOnly
                        spellCheck={false}
                        type={token ? "text" : "password"}
                        value={token || "••••••••"}
                        onFocus={(event) => event.currentTarget.select()}
                      />
                      <button
                        type="button"
                        title={t(token ? "mcpHideToken" : "mcpShowToken")}
                        aria-label={t(token ? "mcpHideToken" : "mcpShowToken")}
                        disabled={busy}
                        onClick={() => {
                          if (token) setToken("");
                          else
                            void update(async () =>
                              setToken(await invoke<string>("mcp_token")),
                            );
                        }}
                      >
                        {token ? <EyeOff size={16} /> : <Eye size={16} />}
                      </button>
                    </div>
                  </div>
                </>
              )}
            </section>

            {running && status.policy.write && (
              <section className="mcp-settings">
                <div className="mcp-row">
                  <h2>
                    {t("mcpApprovals")} ({status.pending.length})
                  </h2>
                  <Hint text={t("mcpApprovalHint")} />
                </div>
                {status.pending.length === 0 && <p>{t("mcpNoApprovals")}</p>}
                {status.pending.map((request) => (
                  <button
                    type="button"
                    className="mcp-request"
                    key={request.id}
                    disabled={busy}
                    onClick={() =>
                      void update(async () => {
                        const details = await invoke<ReviewDetails>(
                          "mcp_review",
                          {
                            id: request.id,
                          },
                        );
                        setReview({ request, ...details });
                      })
                    }
                  >
                    <code>{request.tool}</code>
                    <span>
                      {t("mcpReview")} · {request.id.slice(0, 8)}
                    </span>
                  </button>
                ))}
                {pending && (
                  <div className="mcp-review">
                    <strong>{pending.request.tool}</strong>
                    <small>
                      {pending.request.principal} · {pending.request.id}
                    </small>
                    {pending.preview && (
                      <PatchDiff preview={pending.preview} t={t} />
                    )}
                    {pending.preview_error && (
                      <p role="alert">
                        {t("mcpPreviewUnavailable")} {pending.preview_error}
                      </p>
                    )}
                    <details open={!pending.preview || undefined}>
                      <summary>{t("mcpRawArguments")}</summary>
                      <pre>{JSON.stringify(pending.arguments, null, 2)}</pre>
                    </details>
                    <div className="mcp-actions">
                      {[false, true].map((approve) => (
                        <button
                          type="button"
                          key={String(approve)}
                          className={approve ? "primary" : ""}
                          disabled={
                            busy || (approve && Boolean(pending.preview_error))
                          }
                          onClick={() =>
                            void update(async () => {
                              await invoke("mcp_decide", {
                                id: pending.request.id,
                                approve,
                              });
                              setReview(undefined);
                              setMessage(
                                t(approve ? "mcpApproved" : "mcpDenied"),
                              );
                            })
                          }
                        >
                          {approve ? <Check size={16} /> : <X size={16} />}
                          {t(approve ? "mcpApprove" : "mcpDeny")}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
                {message && <p role="status">{message}</p>}
              </section>
            )}
          </>
        )}
        {failure && (
          <p role="alert" className="preferences-error">
            {failure}
          </p>
        )}
      </div>
    </section>
  );
}
