import { useRef, useState } from "react";
import {
  MousePointer2,
  Waypoints,
  Scan,
  ZoomIn,
  ZoomOut,
  Network,
  Info,
} from "lucide-react";
import { patch, action, select } from "../store";
import { translation } from "../i18n";
import { ports, symbols, type Snapshot, type Node } from "../types";
import {
  NODE_WIDTH as W,
  nodeHeight,
  portY,
  tone,
  caption,
  cablePath,
} from "../nodeAppearance";
import { Hint, IconButton } from "./ui";
export function CircuitCanvas({ snapshot }: { snapshot: Snapshot }) {
  const t = translation(snapshot.locale),
    svg = useRef<SVGSVGElement>(null);
  const [mode, setMode] = useState<"select" | "connect">("select");
  const [source, setSource] = useState<string | null>(null);
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const [camera, setCamera] = useState({ x: -10, y: -20, w: 1160, h: 740 });
  const [drag, setDrag] = useState<{ id: string; x: number; y: number } | null>(
    null,
  );
  const gesture = useRef<{
    id: string | null;
    x: number;
    y: number;
    nx: number;
    ny: number;
    moved: boolean;
    camera: typeof camera;
  } | null>(null);
  const pos = (n: Node) =>
    drag?.id === n.id ? { ...n, x: drag.x, y: drag.y } : n;
  const nodes = snapshot.document.nodes.map(pos);
  const sourceNode = nodes.find((n) => n.id === source);
  const detailed = camera.w < 2000;
  function point(clientX: number, clientY: number) {
    return new DOMPoint(clientX, clientY).matrixTransform(
      svg.current!.getScreenCTM()!.inverse(),
    );
  }
  function fit() {
    if (!nodes.length) return;
    const x = Math.min(...nodes.map((n) => n.x)) - 50,
      y = Math.min(...nodes.map((n) => n.y)) - 55,
      w = Math.max(...nodes.map((n) => n.x + W)) - x + 60,
      h = Math.max(...nodes.map((n) => n.y + nodeHeight(n.kind))) - y + 95;
    setCamera({ x, y, w, h });
  }
  function zoom(factor: number) {
    setCamera((c) => {
      const w = Math.max(300, Math.min(6000, c.w * factor)),
        h = (c.h * w) / c.w;
      return { x: c.x + (c.w - w) / 2, y: c.y + (c.h - h) / 2, w, h };
    });
  }
  function start(e: React.PointerEvent, id: string | null) {
    if (e.button !== 0 || mode !== "select") return;
    e.stopPropagation();
    const p = point(e.clientX, e.clientY);
    const n = nodes.find((n) => n.id === id);
    gesture.current = {
      id,
      x: p.x,
      y: p.y,
      nx: n?.x ?? camera.x,
      ny: n?.y ?? camera.y,
      moved: false,
      camera: { ...camera },
    };
    e.currentTarget.setPointerCapture(e.pointerId);
  }
  async function portClick(id: string, port?: string) {
    if (!port) {
      setSource(id);
      setMode("connect");
      setPointer(null);
    } else if (source) {
      await patch(
        [
          {
            op: "connect",
            edge: {
              id: `e_${Date.now().toString(36)}`,
              from: source,
              to: id,
              port,
            },
          },
        ],
        `Connect ${source} → ${id}.${port}`,
      );
      setSource(null);
      setPointer(null);
    }
  }
  return (
    <div className="canvas-wrap">
      <div className="canvas-toolbar">
        <IconButton
          label={t("select")}
          icon={MousePointer2}
          active={mode === "select"}
          onClick={() => {
            setMode("select");
            setSource(null);
          }}
        />
        <IconButton
          label={t("connect")}
          icon={Waypoints}
          active={mode === "connect"}
          onClick={() => setMode("connect")}
        />
        <span className="separator" />
        <IconButton
          label={t("arrange")}
          icon={Network}
          onClick={() =>
            void action("layout.auto_arrange", {
              expected_revision: snapshot.revision,
            })
          }
        />
        <IconButton label={t("fit")} icon={Scan} onClick={fit} />
        <span className="separator" />
        <IconButton
          label={t("zoomOut")}
          icon={ZoomOut}
          onClick={() => zoom(1.2)}
        />
        <span className="zoom-value">
          {Math.round((1160 / camera.w) * 100)}%
        </span>
        <IconButton
          label={t("zoomIn")}
          icon={ZoomIn}
          onClick={() => zoom(1 / 1.2)}
        />
        <span className="toolbar-spacer" />
        <Hint text={t("connectHint")} />
      </div>
      <div className="canvas-breadcrumb">
        <span>{snapshot.document.name}</span>
        <span>/</span>
        <strong>{t("architecture")}</strong>
        <span className="canvas-count">
          {nodes.length} {t("nodes")}
        </span>
      </div>
      <svg
        ref={svg}
        className={`circuit rich-circuit ${mode} ${detailed ? "" : "overview"}`}
        viewBox={`${camera.x} ${camera.y} ${camera.w} ${camera.h}`}
        aria-label={t("architecture")}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            setSource(null);
            setPointer(null);
            setMode("select");
          }
        }}
        onPointerDown={(e) => start(e, null)}
        onPointerMove={(e) => {
          const p = point(e.clientX, e.clientY);
          if (source) setPointer({ x: p.x, y: p.y });
          const g = gesture.current;
          if (!g) return;
          let dx = p.x - g.x,
            dy = p.y - g.y;
          if (Math.abs(dx) + Math.abs(dy) > 3) g.moved = true;
          if (g.id) setDrag({ id: g.id, x: g.nx + dx, y: g.ny + dy });
          else {
            dx =
              ((e.movementX || 0) * camera.w) / (svg.current?.clientWidth || 1);
            dy =
              ((e.movementY || 0) * camera.h) /
              (svg.current?.clientHeight || 1);
            setCamera((c) => ({ ...c, x: c.x - dx, y: c.y - dy }));
          }
        }}
        onPointerUp={() => {
          const g = gesture.current;
          gesture.current = null;
          if (!g) return;
          if (g.id && g.moved && drag)
            void patch(
              [
                {
                  op: "update_node",
                  id: g.id,
                  x: Math.round(drag.x / 10) * 10,
                  y: Math.round(drag.y / 10) * 10,
                },
              ],
              `Move ${g.id}`,
            );
          else if (g.id) void select(g.id);
          setDrag(null);
        }}
        onPointerCancel={() => {
          gesture.current = null;
          setDrag(null);
        }}
      >
        <defs>
          <pattern
            id="grid"
            width={detailed ? 24 : 120}
            height={detailed ? 24 : 120}
            patternUnits="userSpaceOnUse"
          >
            <circle cx="1" cy="1" r=".8" className="grid-dot" />
          </pattern>
        </defs>
        <rect
          x={camera.x}
          y={camera.y}
          width={camera.w}
          height={camera.h}
          fill="url(#grid)"
        />
        {snapshot.document.edges.map((e) => {
          const a = nodes.find((n) => n.id === e.from)!,
            b = nodes.find((n) => n.id === e.to)!;
          const index = ports(b).findIndex(([p]) => p === e.port);
          const d = cablePath(a, b, index, nodes);
          return (
            <g
              key={e.id}
              className={`wire tone-${tone(a.kind)} ${snapshot.selected === e.from || snapshot.selected === e.to ? "highlighted" : ""}`}
            >
              <path className="wire-shadow" d={d} />
              <path className="wire-core" d={d} />
              <path
                className="wire-hit"
                d={d}
                onPointerDown={(ev) => ev.stopPropagation()}
                onClick={(ev) => {
                  ev.stopPropagation();
                  if (mode === "connect")
                    void patch(
                      [{ op: "disconnect", id: e.id }],
                      `Disconnect ${e.id}`,
                    );
                }}
              />
              <title>
                {e.from}.y → {e.to}.{e.port} · {a.width} bit
              </title>
            </g>
          );
        })}
        {sourceNode && pointer ? (
          <path
            className={`connection-preview tone-${tone(sourceNode.kind)}`}
            d={`M ${sourceNode.x + W} ${sourceNode.y + portY(sourceNode, 0, true)} C ${sourceNode.x + W + 60} ${sourceNode.y + portY(sourceNode, 0, true)} ${pointer.x - 60} ${pointer.y} ${pointer.x} ${pointer.y}`}
          />
        ) : null}
        {nodes.map((n) => {
          const h = nodeHeight(n.kind);
          const missing = snapshot.diagnostics.filter(
            (d) => d.node === n.id,
          ).length;
          const selected = snapshot.selected === n.id;
          return (
            <g
              key={n.id}
              data-testid={`node-${n.id}`}
              className={`circuit-node tone-${tone(n.kind)} ${selected ? "selected" : ""} ${source === n.id ? "connecting" : ""} ${drag?.id === n.id ? "dragged" : ""}`}
              transform={`translate(${n.x},${n.y})`}
              tabIndex={0}
              role="button"
              aria-label={`${n.name}, ${n.kind}, ${n.width} bit`}
              onKeyDown={(e) => {
                if (e.key === "Enter") void select(n.id);
                if (e.key === "Delete" || e.key === "Backspace")
                  void patch(
                    [{ op: "remove_node", id: n.id }],
                    `Remove ${n.name}`,
                  );
              }}
              onPointerDown={(e) => start(e, n.id)}
            >
              <rect
                className="node-shadow"
                x="0"
                y="5"
                width={W}
                height={h}
                rx="10"
              />
              {selected ? (
                <rect
                  className="selection-halo"
                  x="-5"
                  y="-5"
                  width={W + 10}
                  height={h + 10}
                  rx="14"
                />
              ) : null}
              <rect className="node-body" width={W} height={h} rx="9" />
              <path
                className="node-header-fill"
                d={`M 9 1 H ${W - 9} Q ${W - 1} 1 ${W - 1} 9 V 34 H 1 V 9 Q 1 1 9 1`}
              />
              <path className="node-accent-line" d={`M 12 1.5 H ${W - 12}`} />
              <rect
                className="node-symbol-badge"
                x="9"
                y="7"
                width="22"
                height="22"
                rx="4"
              />
              <text
                className="node-header-symbol"
                x="20"
                y="23"
                textAnchor="middle"
              >
                {n.kind === "mux" ? "M" : symbols[n.kind]}
              </text>
              <text className="node-title" x="40" y="23">
                {n.name.length > 17 ? `${n.name.slice(0, 15)}…` : n.name}
              </text>
              {missing ? (
                <g>
                  <circle
                    className="node-warning"
                    cx={W - 13}
                    cy="18"
                    r="3.5"
                  />
                  <title>
                    {missing} {t("issues")}
                  </title>
                </g>
              ) : null}
              <text className="node-caption" x="12" y={h - 14}>
                {caption(n, snapshot.locale)}
              </text>
              <text
                className="node-bit-badge"
                x={W - 12}
                y={h - 14}
                textAnchor="end"
              >
                {n.width} bit
              </text>
              {ports(n).map(([p, width], i) => {
                const connected = snapshot.document.edges.some(
                  (e) => e.to === n.id && e.port === p,
                );
                const compatible =
                  sourceNode &&
                  sourceNode.width === width &&
                  sourceNode.id !== n.id &&
                  !connected;
                return (
                  <g
                    key={p}
                    className={`port ${connected ? "is-connected" : ""} ${sourceNode ? (compatible ? "compatible" : "incompatible") : ""}`}
                    role="button"
                    tabIndex={0}
                    aria-label={`${n.name}.${p} ${width} bit`}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      void portClick(n.id, p);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.stopPropagation();
                        void portClick(n.id, p);
                      }
                    }}
                  >
                    <circle
                      className="port-hit"
                      cx="0"
                      cy={portY(n, i)}
                      r="12"
                    />
                    <circle
                      className="port-ring"
                      cx="0"
                      cy={portY(n, i)}
                      r="5.5"
                    />
                    {connected ? (
                      <circle
                        className="port-center"
                        cx="0"
                        cy={portY(n, i)}
                        r="2.5"
                      />
                    ) : null}
                    <text x="12" y={portY(n, i) + 4}>
                      {p}
                      <tspan className="port-width"> · {width}</tspan>
                    </text>
                    <title>
                      {p} · {width} bit
                    </title>
                  </g>
                );
              })}
              {n.kind !== "output" ? (
                <g
                  className="port output-port is-connected"
                  role="button"
                  tabIndex={0}
                  aria-label={`${n.name}.y ${n.width} bit`}
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation();
                    void portClick(n.id);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.stopPropagation();
                      void portClick(n.id);
                    }
                  }}
                >
                  <circle
                    className="port-hit"
                    cx={W}
                    cy={portY(n, 0, true)}
                    r="12"
                  />
                  <circle
                    className="port-ring"
                    cx={W}
                    cy={portY(n, 0, true)}
                    r="5.5"
                  />
                  <circle
                    className="port-center"
                    cx={W}
                    cy={portY(n, 0, true)}
                    r="2.5"
                  />
                  <text x={W - 12} y={portY(n, 0, true) + 4} textAnchor="end">
                    y
                  </text>
                </g>
              ) : null}
              <title>
                {n.name} · {n.kind.toUpperCase()} · {n.width} bit
              </title>
            </g>
          );
        })}
      </svg>
      <div className="canvas-bottom">
        <span>
          <span className="status-dot" />
          {snapshot.diagnostics.length
            ? `${snapshot.diagnostics.length} ${t("issues")}`
            : t("noIssues")}
        </span>
        {source ? (
          <span className="connection-prompt">
            <Info size={12} />
            {source}.y → …
          </span>
        ) : (
          <span>IR / {snapshot.document.name}</span>
        )}
      </div>
    </div>
  );
}
