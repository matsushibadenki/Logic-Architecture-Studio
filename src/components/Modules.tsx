import { useState } from "react";
import { Group, Ungroup, Plus, Box } from "lucide-react";
import { action } from "../store";
import { translation } from "../i18n";
import { symbols, type Snapshot, type ModuleDefinition } from "../types";
import { IconButton, Hint } from "./ui";
function selected(snapshot: Snapshot) {
  const ids =
    snapshot.selection ?? (snapshot.selected ? [snapshot.selected] : []);
  return snapshot.document.nodes.filter(
    (n) => ids.includes(n.id) && !["input", "output"].includes(n.kind),
  );
}
function moduleName(snapshot: Snapshot) {
  const base =
    (selected(snapshot)[0]?.name ?? "Circuit").slice(0, 36) + "_module";
  let name = base;
  let i = 1;
  while (snapshot.document.modules?.[name]) name = `${base}${i++}`;
  return name;
}
export function ModuleActions({
  snapshot,
  name,
}: {
  snapshot: Snapshot;
  name?: string;
}) {
  const t = translation(snapshot.locale);
  const nodes = selected(snapshot);
  return (
    <>
      <IconButton
        icon={Group}
        label={t("createModule")}
        disabled={!nodes.length}
        onClick={() =>
          void action("module.group", {
            expected_revision: snapshot.revision,
            name: name?.trim() || moduleName(snapshot),
            ids: nodes.map((n) => n.id),
          })
        }
      />
      <IconButton
        icon={Ungroup}
        label={t("expandModule")}
        disabled={nodes.length !== 1 || nodes[0].kind !== "module"}
        onClick={() =>
          void action("module.explode", {
            expected_revision: snapshot.revision,
            id: nodes[0].id,
          })
        }
      />
    </>
  );
}
export function ModuleLibrary({ snapshot }: { snapshot: Snapshot }) {
  const t = translation(snapshot.locale);
  const [name, setName] = useState("");
  return (
    <section className="module-library">
      <div className="section-title">
        <h3>{t("modules")}</h3>
        <Hint text={t("moduleHint")} />
      </div>
      <div className="module-create">
        <input
          aria-label={t("moduleName")}
          placeholder={t("moduleName")}
          value={name}
          maxLength={64}
          onChange={(e) => setName(e.target.value)}
        />
        <ModuleActions snapshot={snapshot} name={name} />
      </div>
      {Object.entries(snapshot.document.modules ?? {}).map(
        ([key, definition]) => (
          <button
            className="module-item"
            key={key}
            title={`${t("instantiateModule")}: ${key}`}
            onClick={() => {
              const x = Math.min(
                99000,
                Math.max(0, ...snapshot.document.nodes.map((n) => n.x)) + 420,
              );
              void action("module.instantiate", {
                expected_revision: snapshot.revision,
                name: key,
                x,
                y: Math.min(0, ...snapshot.document.nodes.map((n) => n.y)),
              });
            }}
          >
            <Box size={16} />
            <span>{key}</span>
            <small>
              {
                definition.nodes.filter(
                  (n) => !["input", "output"].includes(n.kind),
                ).length
              }
            </small>
            <Plus size={14} />
          </button>
        ),
      )}
    </section>
  );
}
export function ModuleTree({
  name,
  library,
  trail = [],
}: {
  name: string;
  library: Record<string, ModuleDefinition>;
  trail?: string[];
}) {
  const definition = library[name];
  if (!definition || trail.includes(name) || trail.length >= 16) return null;
  return (
    <ul className="module-tree">
      {definition.nodes.map((n) => (
        <li key={n.id}>
          {n.config?.type === "module" ? (
            <details>
              <summary>
                <Box size={13} />
                {n.name} <small>{n.config.definition}</small>
              </summary>
              <ModuleTree
                name={n.config.definition}
                library={library}
                trail={[...trail, name]}
              />
            </details>
          ) : (
            <span>
              <b>{symbols[n.kind]}</b>
              {n.name}
              <small>
                {n.width}
                {n.signed ? "s" : "u"}
              </small>
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}
