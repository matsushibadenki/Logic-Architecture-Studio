import type { translation } from "../i18n";

type EntityChange = {
  id: string;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
};
export type PatchPreview = {
  revision: number;
  label: string;
  nodes: EntityChange[];
  edges: EntityChange[];
  from_revision?: number;
  document_name?: { before: string; after: string } | null;
  modules?: {
    name: string;
    status: "added" | "modified" | "removed";
    nodes: EntityChange[];
    edges: EntityChange[];
  }[];
  diagnostics: { code: string; node: string | null; message: string }[];
};
type Translate = ReturnType<typeof translation>;

export function PatchDiff({
  preview,
  t,
}: {
  preview: PatchPreview;
  t: Translate;
}) {
  const groups = [
    { name: t("mcpDiffNodes"), entries: preview.nodes },
    { name: t("mcpDiffWires"), entries: preview.edges },
    ...(preview.modules ?? []).flatMap((module) => [
      { name: `${module.name} · ${t("mcpDiffNodes")}`, entries: module.nodes },
      { name: `${module.name} · ${t("mcpDiffWires")}`, entries: module.edges },
    ]),
  ];
  const revision = (value: number) => `r${String(value).padStart(3, "0")}`;
  const display = (value: unknown) =>
    value === undefined
      ? "—"
      : typeof value === "string"
        ? value
        : JSON.stringify(value);
  return (
    <div className="mcp-diff">
      <h3>
        {t("mcpPatchDiff")} ·{" "}
        {preview.from_revision === undefined
          ? revision(preview.revision)
          : `${revision(preview.from_revision)} → ${revision(preview.revision)}`}
      </h3>
      <p>{preview.label}</p>
      {preview.document_name && (
        <p>
          <del>{preview.document_name.before}</del> →{" "}
          <ins>{preview.document_name.after}</ins>
        </p>
      )}
      {preview.modules?.map((module) => (
        <p key={module.name}>
          {t("modules")} · {module.name} ·{" "}
          {t(
            module.status === "added"
              ? "mcpDiffAdded"
              : module.status === "removed"
                ? "mcpDiffRemoved"
                : "mcpDiffChanged",
          )}
        </p>
      ))}
      {!preview.document_name &&
        !preview.modules?.length &&
        groups.every(({ entries }) => entries.length === 0) && (
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
