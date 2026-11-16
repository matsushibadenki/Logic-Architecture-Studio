import { ModuleTree, ModuleActions } from "./Modules";
import type { CSSProperties } from "react";
import { nodeColor, symbolFontSize } from "../appearance";
import { Trash2, Unplug, Box, FolderOpen } from "lucide-react";
import { patch, action, native, nativeAction } from "../store";
import {
  defaultConfig,
  inputSigned,
  ports,
  outputs,
  kinds,
  symbols,
  type Kind,
  type NodeConfig,
  type Snapshot,
} from "../types";
import { translation } from "../i18n";
import { Hint, IconButton } from "./ui";
export function Inspector({
  snapshot,
  edit = patch,
  scoped = false,
}: {
  snapshot: Snapshot;
  edit?: typeof patch;
  scoped?: boolean;
}) {
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
  const moduleDefinition =
    node.config?.type === "module" ? node.config.definition : null;
  const boundary = scoped && (node.kind === "input" || node.kind === "output");
  return (
    <div
      className="inspector-content"
      style={
        { "--inspector-tone": nodeColor(snapshot, node.kind) } as CSSProperties
      }
      key={`${node.id}-${node.name}-${node.kind}-${node.width}-${node.signed}-${JSON.stringify(node.config)}`}
    >
      <div className="node-heading">
        <span
          className="large-symbol"
          style={{ fontSize: symbolFontSize(symbols[node.kind], 26, 22) }}
        >
          {symbols[node.kind]}
        </span>
        <div>
          <strong>{node.name}</strong>
          <small>
            {node.config?.type === "module"
              ? node.config.definition
              : `${node.kind.toUpperCase()} · ${node.width} bit`}
          </small>
        </div>
        <IconButton
          icon={Trash2}
          label={t("delete")}
          disabled={boundary}
          onClick={() =>
            void edit(
              [{ op: "remove_node", id: node.id }],
              `Remove ${node.name}`,
            )
          }
        />
      </div>
      {node.kind !== "module" && (
        <>
          <div className="numeric-semantics">
            <strong>
              {t(node.signed ? "signedType" : "numericSemantics")}
            </strong>
            <Hint
              text={t(
                ["zero_extend", "sign_extend"].includes(node.kind)
                  ? "extensionHint"
                  : "simulationSemantics",
              )}
            />
            {["add", "sub"].includes(node.kind) && (
              <small>{t("wrapSemantics")}</small>
            )}
            {node.kind === "register" && <small>{t("resetSemantics")}</small>}
          </div>
        </>
      )}
      <fieldset className="fields module-inspector-fields" disabled={boundary}>
        {node.kind !== "module" && (
          <>
            <label>
              <span>
                {t("signalType")} <Hint text={t("propagateSignedHint")} />
              </span>
              <select
                aria-label={t("signalType")}
                value={node.signed ? "signed" : "unsigned"}
                onChange={(e) =>
                  scoped
                    ? void edit(
                        [
                          {
                            op: "update_node",
                            id: node.id,
                            signed: e.target.value === "signed",
                          },
                        ],
                        t("signalType"),
                      )
                    : void action("design.set_signed", {
                        id: node.id,
                        signed: e.target.value === "signed",
                        expected_revision: snapshot.revision,
                      })
                }
              >
                <option value="unsigned">{t("unsignedType")}</option>
                <option value="signed">{t("signedType")}</option>
              </select>
            </label>
          </>
        )}
        <label>
          {t("name")}
          <input
            aria-label={t("name")}
            defaultValue={node.name}
            onBlur={async (e) => {
              const input = e.currentTarget;
              if (input.value !== node.name) {
                const result = await edit(
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
            disabled={node.kind === "module"}
            aria-label={t("kind")}
            value={node.kind}
            onChange={(e) =>
              void edit(
                [
                  {
                    op: "update_node",
                    id: node.id,
                    kind: e.target.value,
                    width:
                      e.target.value === "concat"
                        ? Math.max(2, node.width)
                        : node.width,
                    config: defaultConfig(
                      e.target.value as Kind,
                      e.target.value === "concat"
                        ? Math.max(2, node.width)
                        : node.width,
                    ),
                  },
                ],
                `Change ${node.name}`,
              )
            }
          >
            {node.kind === "module" && (
              <option value="module">{t("moduleKind")}</option>
            )}
            {kinds.map((k) => (
              <option key={k} value={k}>
                {[
                  "constant",
                  "slice",
                  "concat",
                  "zero_extend",
                  "sign_extend",
                  "register",
                ].includes(k)
                  ? t(`${k}Kind` as "constantKind")
                  : k.toUpperCase()}
              </option>
            ))}
          </select>
        </label>
        {node.kind !== "module" && (
          <>
            <label>
              {t("width")}
              <div className="unit-input">
                <input
                  aria-label={t("width")}
                  type="number"
                  min={1}
                  max={64}
                  readOnly={node.kind === "concat"}
                  defaultValue={node.width}
                  onBlur={async (e) => {
                    const input = e.currentTarget;
                    const width = Number(input.value);
                    if (width !== node.width) {
                      const result = await edit(
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
          </>
        )}
      </fieldset>
      {node.config?.type === "module" && (
        <section className="inspector-section">
          <div className="section-title">
            <strong>{node.config.definition}</strong>
            {native && !scoped && moduleDefinition && (
              <IconButton
                icon={FolderOpen}
                label={t("openModule")}
                onClick={() =>
                  void nativeAction("open_module", {
                    definition: moduleDefinition,
                  })
                }
              />
            )}
            {!scoped && <ModuleActions snapshot={snapshot} />}
          </div>
          <Hint text={t("moduleHint")} />
          <ModuleTree
            name={node.config.definition}
            library={snapshot.document.modules ?? {}}
          />
        </section>
      )}
      {node.config &&
        node.config.type !== "none" &&
        node.config.type !== "module" && (
          <div className="fields node-config-fields">
            <Hint
              text={t(
                ["zero_extend", "sign_extend"].includes(node.kind)
                  ? "extensionHint"
                  : "bitNodeHint",
              )}
            />
            {Object.entries(node.config)
              .filter(([key]) => key !== "type")
              .map(([key, value]) => {
                const label = t(
                  (
                    {
                      value: "constantValue",
                      input_width: "inputWidth",
                      lsb: "sliceLsb",
                      high_width: "highWidth",
                      low_width: "lowWidth",
                    } as const
                  )[key as "value"],
                );
                return (
                  <label key={key}>
                    {label}
                    <input
                      aria-label={label}
                      type={key === "value" ? "text" : "number"}
                      min={key === "lsb" ? 0 : 1}
                      max={key === "lsb" ? 63 : 64}
                      maxLength={20}
                      defaultValue={value}
                      spellCheck={false}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") e.currentTarget.blur();
                      }}
                      onBlur={async (e) => {
                        const input = e.currentTarget;
                        const next =
                          key === "value" ? input.value : Number(input.value);
                        if (next === value) return;
                        const config = {
                          ...node.config,
                          [key]: next,
                        } as NodeConfig;
                        const width =
                          config.type === "concat"
                            ? config.high_width + config.low_width
                            : node.width;
                        const result = await edit(
                          [{ op: "update_node", id: node.id, config, width }],
                          `Configure ${node.name}`,
                        );
                        if (!result) input.value = String(value);
                      }}
                    />
                  </label>
                );
              })}
          </div>
        )}
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
                <code>
                  {port}
                  {inputSigned(node, port) ? " · s" : ""}
                </code>
                <span>{width}</span>
                {edge ? (
                  <IconButton
                    icon={Unplug}
                    label={`${t("disconnect")} ${port}`}
                    onClick={() =>
                      void edit(
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
              {t("outputs")} <span>{outputs(node).length}</span>
            </div>
            <div className="port-table">
              {outputs(node).map(([port, width, signed]) => (
                <div key={port}>
                  <span className="port-dot connected" />
                  <code>
                    {port}
                    {signed ? " · s" : ""}
                  </code>
                  <span>{width}</span>
                </div>
              ))}
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
