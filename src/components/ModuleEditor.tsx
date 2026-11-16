import { useMemo, useState } from "react";
import { CircuitCanvas } from "./Canvas";
import { Inspector } from "./Inspector";
import { Library } from "./Sidebar";
import { action } from "../store";
import { translation } from "../i18n";
import { ports, type Operation, type Snapshot } from "../types";

export function ModuleEditor({
  root,
  initial,
  back,
}: {
  root: Snapshot;
  initial: string;
  back: () => void;
}) {
  const [path, setPath] = useState([initial]);
  const [selection, select] = useState<string[]>([]);
  const library = root.document.modules;
  const name = path.at(-1)!;
  const definition = library?.[name];
  const graph = useMemo(
    () => definition && { ...definition, name, modules: library },
    [definition, name, library],
  );
  const diagnostics = useMemo(() => {
    if (!graph) return [];
    const connected = new Set(
      graph.edges.map((edge) => `${edge.to}.${edge.port}`),
    );
    return graph.nodes.flatMap((node) =>
      ports(node)
        .filter(([port]) => !connected.has(`${node.id}.${port}`))
        .map(([port]) => ({
          code: "unconnected",
          node: node.id,
          message: port,
        })),
    );
  }, [graph]);
  const t = translation(root.locale);

  function navigate(next: string[]) {
    select([]);
    setPath(next);
  }
  const edit = (operations: Operation[], _label: string) =>
    action("module.edit", {
      expected_revision: root.revision,
      definition: name,
      operations,
    });

  const ids = selection.filter((id) =>
    graph?.nodes.some((node) => node.id === id),
  );
  const snapshot = graph && {
    ...root,
    document: graph,
    diagnostics,
    selection: ids,
    selected: ids.at(-1) ?? null,
  };
  return (
    <main className="module-editor">
      <nav className="module-editor-breadcrumb" aria-label={t("modules")}>
        <button onClick={back}>{root.document.name}</button>
        {path.map((item, index) => (
          <span key={index}>
            {" / "}
            {index === path.length - 1 ? (
              <strong aria-current="location">{item}</strong>
            ) : (
              <button onClick={() => navigate(path.slice(0, index + 1))}>
                {item}
              </button>
            )}
          </span>
        ))}
        <span className="toolbar-spacer" />
        <button
          disabled={!root.can_undo}
          onClick={() =>
            void action("design.undo", { expected_revision: root.revision })
          }
        >
          {t("undo")}
        </button>
        <button
          disabled={!root.can_redo}
          onClick={() =>
            void action("design.redo", { expected_revision: root.revision })
          }
        >
          {t("redo")}
        </button>
      </nav>
      {snapshot ? (
        <div className="module-editor-columns">
          <aside aria-label={t("library")}>
            <Library snapshot={snapshot} edit={edit} scoped />
          </aside>
          <CircuitCanvas
            key={path.join("/")}
            snapshot={snapshot}
            definition={name}
            edit={edit}
            select={select}
            openModule={(child) => navigate([...path, child])}
          />
          <aside aria-label={t("inspector")}>
            <Inspector snapshot={snapshot} edit={edit} scoped />
          </aside>
        </div>
      ) : (
        <p role="status">{t("moduleUnavailable")}</p>
      )}
    </main>
  );
}
