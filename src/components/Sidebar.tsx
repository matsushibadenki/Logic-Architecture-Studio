import { nodeColor } from "../appearance";
import { memo, useState } from "react";
import { Search, ChevronDown, Cpu, Plus } from "lucide-react";
import { patch, select } from "../store";
import { translation } from "../i18n";
import { kinds, symbols, type Snapshot } from "../types";
const HierarchyRow = memo(function HierarchyRow({
  id,
  name,
  kind,
  width,
  selected,
  color,
}: {
  id: string;
  name: string;
  kind: import("../types").Kind;
  width: number;
  selected: boolean;
  color: string;
}) {
  return (
    <button
      className={selected ? "selected" : ""}
      onClick={() => void select(id)}
    >
      <span className="node-icon" style={{ color }}>
        {symbols[kind]}
      </span>
      <span>{name}</span>
      <small>{width}</small>
    </button>
  );
});
export function Hierarchy({ snapshot }: { snapshot: Snapshot }) {
  const [query, setQuery] = useState("");
  const t = translation(snapshot.locale);
  return (
    <section className="hierarchy">
      <div className="section-title">
        <h2>{t("hierarchy")}</h2>
        <span>{snapshot.document.nodes.length}</span>
      </div>
      <label className="search">
        <Search size={14} />
        <input
          aria-label={t("search")}
          placeholder={t("search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <kbd>⌕</kbd>
      </label>
      <div className="tree-root">
        <ChevronDown size={14} />
        <Cpu size={15} />
        <strong>{snapshot.document.name}</strong>
        <span className="mini-tag">32</span>
      </div>
      <div className="tree">
        {snapshot.document.nodes
          .filter((n) => n.name.toLowerCase().includes(query.toLowerCase()))
          .map((n) => (
            <HierarchyRow
              key={n.id}
              id={n.id}
              name={n.name}
              kind={n.kind}
              width={n.width}
              color={nodeColor(snapshot, n.kind)}
              selected={snapshot.selected === n.id}
            />
          ))}
      </div>
      <div className="hierarchy-footer">
        <span className="status-dot" />
        {t("maturity")}
      </div>
    </section>
  );
}
export function Library({ snapshot }: { snapshot: Snapshot }) {
  const t = translation(snapshot.locale);
  return (
    <div className="library-list">
      {kinds.map((kind) => (
        <button
          key={kind}
          title={`${t("newNode")}: ${kind === "register" ? t("registerKind") : kind}`}
          onClick={() => {
            const id = `n_${crypto.randomUUID().replaceAll("-", "")}`;
            const prefix =
              kind === "input"
                ? "in"
                : kind === "output"
                  ? "out"
                  : kind === "register"
                    ? "reg"
                    : kind;
            const used = new Set(
              snapshot.document.nodes.map((node) => node.name),
            );
            let index = 0;
            while (used.has(`${prefix}${index}`)) index += 1;
            const name = `${prefix}${index}`;
            void patch(
              [
                {
                  op: "add_node",
                  node: {
                    id,
                    name,
                    kind,
                    width: 32,
                    x: 200 + (snapshot.document.nodes.length % 4) * 60,
                    y: 100 + (snapshot.document.nodes.length % 5) * 80,
                  },
                },
              ],
              `Add ${kind}`,
            );
          }}
        >
          <span
            className="library-symbol"
            style={{ color: nodeColor(snapshot, kind) }}
          >
            {symbols[kind]}
          </span>
          <span>
            {kind === "input"
              ? t("inputs")
              : kind === "output"
                ? t("outputs")
                : kind === "register"
                  ? t("registerKind")
                  : kind.toUpperCase()}
          </span>
          <Plus size={13} />
        </button>
      ))}
    </div>
  );
}
