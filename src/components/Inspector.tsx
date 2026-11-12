import type { CSSProperties } from "react";
import { nodeColor } from "../appearance";
import { Trash2, Unplug, Box } from "lucide-react";
import { patch } from "../store";
import { ports, kinds, symbols, type Snapshot } from "../types";
import { translation } from "../i18n";
import { Hint, IconButton } from "./ui";
export function Inspector({ snapshot }: { snapshot: Snapshot }) {
  const t = translation(snapshot.locale);
  const node = snapshot.document.nodes.find((n) => n.id === snapshot.selected);
  if (!node)
    return (
      <div className="empty">
        <Box size={24} />
        <p>{t("noSelection")}</p>
      </div>
    );
  const inputs = ports(node);
  return (
    <div
      className="inspector-content"
      style={
        { "--inspector-tone": nodeColor(snapshot, node.kind) } as CSSProperties
      }
      key={`${node.id}-${node.name}-${node.kind}-${node.width}`}
    >
      <div className="node-heading">
        <span className="large-symbol">{symbols[node.kind]}</span>
        <div>
          <strong>{node.name}</strong>
          <small>
            {node.kind.toUpperCase()} · {node.width} bit
          </small>
        </div>
        <IconButton
          icon={Trash2}
          label={t("delete")}
          onClick={() =>
            void patch(
              [{ op: "remove_node", id: node.id }],
              `Remove ${node.name}`,
            )
          }
        />
      </div>
      <div className="numeric-semantics">
        <strong>{t("numericSemantics")}</strong>
        <Hint text={t("simulationSemantics")} />
        {["add", "sub"].includes(node.kind) && (
          <small>{t("wrapSemantics")}</small>
        )}
        {node.kind === "register" && <small>{t("resetSemantics")}</small>}
      </div>
      <div className="fields">
        <label>
          {t("name")}
          <input
            aria-label={t("name")}
            defaultValue={node.name}
            onBlur={async (e) => {
              const input = e.currentTarget;
              if (input.value !== node.name) {
                const result = await patch(
                  [{ op: "update_node", id: node.id, name: e.target.value }],
                  `Rename ${node.name}`,
                );
                if (!result) input.value = node.name;
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
            }}
          />
        </label>
        <label>
          {t("kind")}
          <select
            aria-label={t("kind")}
            value={node.kind}
            onChange={(e) =>
              void patch(
                [{ op: "update_node", id: node.id, kind: e.target.value }],
                `Change ${node.name}`,
              )
            }
          >
            {kinds.map((k) => (
              <option key={k} value={k}>
                {k === "register" ? t("registerKind") : k.toUpperCase()}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("width")}
          <div className="unit-input">
            <input
              aria-label={t("width")}
              type="number"
              min={1}
              max={64}
              defaultValue={node.width}
              onBlur={async (e) => {
                const input = e.currentTarget;
                const width = Number(input.value);
                if (width !== node.width) {
                  const result = await patch(
                    [{ op: "update_node", id: node.id, width }],
                    `Resize ${node.name}`,
                  );
                  if (!result) input.value = String(node.width);
                }
              }}
            />
            <span>bit</span>
          </div>
        </label>
      </div>
      <div className="inspector-section">
        <h3>{t("ports")}</h3>
        <div className="table-label">
          {t("inputs")} <span>{inputs.length}</span>
        </div>
        <div className="port-table">
          {inputs.map(([port, width]) => {
            const edge = snapshot.document.edges.find(
              (e) => e.to === node.id && e.port === port,
            );
            return (
              <div key={port}>
                <span className={`port-dot ${edge ? "connected" : ""}`} />
                <code>{port}</code>
                <span>{width}</span>
                {edge ? (
                  <IconButton
                    icon={Unplug}
                    label={`${t("disconnect")} ${port}`}
                    onClick={() =>
                      void patch(
                        [{ op: "disconnect", id: edge.id }],
                        `Disconnect ${node.name}.${port}`,
                      )
                    }
                  />
                ) : (
                  <span className="warning-dot" />
                )}
              </div>
            );
          })}
        </div>
        {node.kind !== "output" ? (
          <>
            <div className="table-label">
              {t("outputs")} <span>1</span>
            </div>
            <div className="port-table">
              <div>
                <span className="port-dot connected" />
                <code>y</code>
                <span>{node.width}</span>
              </div>
            </div>
          </>
        ) : null}
      </div>
      <div className="inspector-section">
        <h3>{t("revision")}</h3>
        <dl>
          <dt>{t("revision")}</dt>
          <dd>r{String(snapshot.revision).padStart(3, "0")}</dd>
          <dt>ID</dt>
          <dd>{node.id}</dd>
          <dt>{t("connections")}</dt>
          <dd>
            {
              snapshot.document.edges.filter(
                (e) => e.to === node.id || e.from === node.id,
              ).length
            }
          </dd>
        </dl>
      </div>
    </div>
  );
}
