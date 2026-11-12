import { nodeColor } from "../appearance";
import type { CSSProperties } from "react";
import { useRef, useState, useLayoutEffect } from "react";
import {
  MousePointer2,
  Waypoints,
  Scan,
  ZoomIn,
  ZoomOut,
  Network,
  Info,
} from "lucide-react";
import { patch, action, selectNodes } from "../store";
import { translation } from "../i18n";
import { ports, symbols, type Snapshot, type Node } from "../types";
import {
  NODE_WIDTH as W,
  nodeHeight,
  portY,
  tone,
  caption,
  cablePath,
  gridSpacing,
  snapToGrid,
} from "../nodeAppearance";
import { Hint, IconButton } from "./ui";
import { useSimulation } from "../simulation";
export function CircuitCanvas({ snapshot }: { snapshot: Snapshot }) {
  const trace = useSimulation();
  const annotation =
    trace?.revision === snapshot.revision ? trace.annotation : null;
  const t = translation(snapshot.locale),
    svg = useRef<SVGSVGElement>(null);
  const [mode, setMode] = useState<"select" | "connect">("select");
  const [source, setSource] = useState<string | null>(null);
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const [camera, setCamera] = useState({ x: -10, y: -20, w: 1160, h: 740 });
  const selectedIds =
    snapshot.selection ?? (snapshot.selected ? [snapshot.selected] : []);
  const [previewSelection, setPreviewSelection] = useState<string[] | null>(
    null,
  );
  const selection = previewSelection ?? selectedIds;
  const [drag, setDrag] = useState<{
    ids: string[];
    revision: number;
    dx: number;
    dy: number;
  } | null>(null);
  const [marquee, setMarquee] = useState<{
    x: number;
    y: number;
    width: number;
    height: number;
  } | null>(null);
  const gesture = useRef<{
    id: string | null;
    x: number;
    y: number;
    moved: boolean;
    pan: boolean;
    ids: string[];
    original: string[];
    additive: boolean;
    toggleOff: boolean;
    revision: number;
    anchor: { x: number; y: number };
    spacing: number;
  } | null>(null);
  const pos = (n: Node) =>
    drag?.revision === snapshot.revision && drag.ids.includes(n.id)
      ? { ...n, x: n.x + drag.dx, y: n.y + drag.dy }
      : n;
  const nodes = snapshot.document.nodes.map(pos);
  function cancelGesture() {
    gesture.current = null;
    setDrag(null);
    setMarquee(null);
    setPreviewSelection(null);
  }
  function labelY(n: Node, index: number, output = false) {
    const anchor = portY(n, index, output),
      center = nodeHeight(n.kind) / 2;
    const top = !annotation
      ? anchor - 15
      : anchor <= center
        ? Math.min(anchor - 15, center - 43)
        : Math.max(anchor - 15, center + 24);
    return top + 23;
  }
  const sourceNode = nodes.find((n) => n.id === source);
  const detailed = camera.w < 2000;
  const [canvasSize, setCanvasSize] = useState({ width: 1160, height: 740 });
  useLayoutEffect(() => {
    const element = svg.current;
    if (!element) return;
    const update = () =>
      setCanvasSize({
        width: element.clientWidth,
        height: element.clientHeight,
      });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const scale = Math.min(
    canvasSize.width / camera.w,
    canvasSize.height / camera.h,
  );
  const spacing = gridSpacing(scale);
  const tiny = scale < 0.35;
  const safeScale = Math.max(scale, 0.02);
  const wireWidth = (bus: boolean) => Math.max(bus ? 4.5 : 2.5, 1 / safeScale);
  const portRadius = Math.max(6, (tiny ? 1.5 : 2.5) / safeScale);
  const portRim = Math.max(1.5, 1 / safeScale);
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
    if (![0, 1, 2].includes(e.button) || (mode !== "select" && e.button === 0))
      return;
    e.stopPropagation();
    const p = point(e.clientX, e.clientY);
    const additive = e.shiftKey || e.metaKey || e.ctrlKey;
    const pan = e.button !== 0;
    const toggleOff = !!id && additive && selectedIds.includes(id);
    const ids = pan
      ? selectedIds
      : id
        ? toggleOff
          ? selectedIds.filter((i) => i !== id)
          : additive || selectedIds.includes(id)
            ? [...new Set([...selectedIds, id])]
            : [id]
        : additive
          ? selectedIds
          : [];
    if (ids.length > 512) return;
    gesture.current = {
      id,
      x: p.x,
      y: p.y,
      moved: false,
      pan,
      ids,
      original: selectedIds,
      additive,
      toggleOff,
      revision: snapshot.revision,
      anchor: nodes.find((n) => n.id === id) ?? { x: 0, y: 0 },
      spacing: spacing,
    };
    if (!pan) setPreviewSelection(ids);
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
        <Hint text={t("multiSelectHint")} />
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
        className={`circuit rich-circuit ${mode} ${detailed ? "" : "overview"} ${tiny ? "tiny" : ""}`}
        style={
          {
            "--port-rim": portRim,
            "--preview-width": wireWidth(false),
          } as CSSProperties
        }
        viewBox={`${camera.x} ${camera.y} ${camera.w} ${camera.h}`}
        aria-label={t("architecture")}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            cancelGesture();
            setSource(null);
            setPointer(null);
            setMode("select");
          }
        }}
        onContextMenu={(e) => e.preventDefault()}
        onPointerDown={(e) => start(e, null)}
        onPointerMove={(e) => {
          const p = point(e.clientX, e.clientY);
          if (source) setPointer({ x: p.x, y: p.y });
          const g = gesture.current;
          if (!g) return;
          const dx = p.x - g.x,
            dy = p.y - g.y;
          g.moved = Math.hypot(dx, dy) * scale >= 3;
          if (g.pan) {
            setCamera((c) => ({
              ...c,
              x:
                c.x -
                (e.movementX * camera.w) / (svg.current?.clientWidth || 1),
              y:
                c.y -
                (e.movementY * camera.h) / (svg.current?.clientHeight || 1),
            }));
          } else if (g.id && !g.toggleOff) {
            setDrag(
              g.moved
                ? {
                    ids: g.ids,
                    revision: g.revision,
                    dx: snapToGrid(g.anchor.x + dx, g.spacing) - g.anchor.x,
                    dy: snapToGrid(g.anchor.y + dy, g.spacing) - g.anchor.y,
                  }
                : null,
            );
          } else if (!g.id) {
            const box = {
              x: Math.min(g.x, p.x),
              y: Math.min(g.y, p.y),
              width: Math.abs(dx),
              height: Math.abs(dy),
            };
            setMarquee(box);
            const inside = snapshot.document.nodes
              .filter(
                (n) =>
                  n.x <= box.x + box.width &&
                  n.x + W >= box.x &&
                  n.y <= box.y + box.height &&
                  n.y + nodeHeight(n.kind) >= box.y,
              )
              .map((n) => n.id);
            g.ids = [
              ...new Set([...(g.additive ? g.original : []), ...inside]),
            ].slice(0, 512);
            setPreviewSelection(g.ids);
          }
        }}
        onPointerUp={async () => {
          const g = gesture.current;
          gesture.current = null;
          if (!g) return;
          try {
            if (!g.pan) {
              const result = await action("view.select_nodes", {
                expected_revision: g.revision,
                ids: g.ids,
              });
              if (
                result !== undefined &&
                g.id &&
                !g.toggleOff &&
                g.moved &&
                drag &&
                (drag.dx || drag.dy)
              ) {
                await action("design.apply_patch", {
                  expected_revision: g.revision,
                  label: `Move ${g.ids.length} nodes`,
                  operations: snapshot.document.nodes
                    .filter((n) => g.ids.includes(n.id))
                    .map((n) => ({
                      op: "update_node",
                      id: n.id,
                      x: n.x + drag.dx,
                      y: n.y + drag.dy,
                    })),
                });
              }
            }
          } finally {
            cancelGesture();
          }
        }}
        onPointerCancel={cancelGesture}
      >
        <defs>
          <pattern
            id="grid"
            x={-spacing / 2}
            y={-spacing / 2}
            width={spacing}
            height={spacing}
            patternUnits="userSpaceOnUse"
          >
            <circle
              cx={spacing / 2}
              cy={spacing / 2}
              r={0.8 / Math.max(scale, 0.02)}
              className="grid-dot"
            />
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
          const d = cablePath(a, b, index);
          return (
            <g
              key={e.id}
              style={
                {
                  "--node-tone": nodeColor(snapshot, a.kind),
                  "--wire-width": `${wireWidth(a.width > 1)}px`,
                } as CSSProperties
              }
              className={`wire tone-${tone(a.kind)} ${selection.includes(e.from) || selection.includes(e.to) ? "highlighted" : ""}`}
            >
              <path className="wire-shadow" d={d} />
              <path
                className="wire-core"
                d={d}
                style={{ strokeWidth: wireWidth(a.width > 1) }}
              />
              {detailed && (
                <text
                  className="wire-value"
                  x={a.x + W + 8}
                  y={a.y + portY(a, 0, true) - 12}
                >
                  {a.width > 1 ? `/${a.width}` : ""}
                  {annotation?.values[a.id] != null
                    ? ` · ${annotation.values[a.id]}`
                    : ""}
                </text>
              )}
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
            style={
              {
                "--node-tone": nodeColor(snapshot, sourceNode.kind),
              } as CSSProperties
            }
            className={`connection-preview tone-${tone(sourceNode.kind)}`}
            d={`M ${sourceNode.x + W} ${sourceNode.y + portY(sourceNode, 0, true)} C ${sourceNode.x + W + 60} ${sourceNode.y + portY(sourceNode, 0, true)} ${pointer.x - 60} ${pointer.y} ${pointer.x} ${pointer.y}`}
          />
        ) : null}
        {nodes.map((n) => {
          const h = nodeHeight(n.kind);
          const missing = snapshot.diagnostics.filter(
            (d) => d.node === n.id,
          ).length;
          const selected = selection.includes(n.id);
          return (
            <g
              key={n.id}
              data-testid={`node-${n.id}`}
              style={
                { "--node-tone": nodeColor(snapshot, n.kind) } as CSSProperties
              }
              className={`circuit-node tone-${tone(n.kind)} ${selected ? "selected" : ""} ${source === n.id ? "connecting" : ""} ${drag?.ids.includes(n.id) ? "dragged" : ""}`}
              transform={`translate(${n.x},${n.y})`}
              tabIndex={0}
              role="button"
              aria-pressed={selected}
              aria-label={`${n.name}, ${n.kind}, ${n.width} bit`}
              onKeyDown={(e) => {
                if (e.key === "Enter")
                  void selectNodes(
                    e.shiftKey || e.metaKey || e.ctrlKey
                      ? selectedIds.includes(n.id)
                        ? selectedIds.filter((id) => id !== n.id)
                        : [...selectedIds, n.id]
                      : [n.id],
                  );
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
              {annotation?.values[n.id] != null && (
                <>
                  <text
                    className="node-live-value"
                    x={14}
                    y={h / 2 + 10}
                    fontSize={annotation.values[n.id].length > 12 ? 12 : 20}
                  >
                    {n.kind === "register" ? "Q = " : ""}
                    {annotation.values[n.id]}
                  </text>
                  {annotation.register_d[n.id] != null && (
                    <text className="node-live-d" x={14} y={h / 2 + 29}>
                      D = {annotation.register_d[n.id]}
                    </text>
                  )}
                </>
              )}
              {ports(n).map(([p, width], i) => {
                const connected = snapshot.document.edges.some(
                  (e) => e.to === n.id && e.port === p,
                );
                const compatible =
                  sourceNode &&
                  sourceNode.width === width &&
                  (sourceNode.id !== n.id || n.kind === "register") &&
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
                      r={portRadius - (tiny ? 0 : portRim / 2)}
                    />
                    {connected ? (
                      <circle
                        className="port-center"
                        cx="0"
                        cy={portY(n, i)}
                        r={Math.max(2.5, 0.8 / safeScale)}
                      />
                    ) : null}
                    <text x="12" y={labelY(n, i)}>
                      {p}
                      <tspan className="port-width">
                        {width > 1 ? `[${width - 1}:0]` : ""}
                      </tspan>
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
                    r={portRadius - (tiny ? 0 : portRim / 2)}
                  />
                  <circle
                    className="port-center"
                    cx={W}
                    cy={portY(n, 0, true)}
                    r={Math.max(2.5, 0.8 / safeScale)}
                  />
                  <text x={W - 12} y={labelY(n, 0, true)} textAnchor="end">
                    {n.kind === "register" ? "q" : "y"}
                    {n.width > 1 ? `[${n.width - 1}:0]` : ""}
                  </text>
                </g>
              ) : null}
              {tiny && (
                <text
                  className="node-overview-symbol"
                  x={W / 2}
                  y={h / 2}
                  textAnchor="middle"
                  dominantBaseline="central"
                  fontSize={
                    n.kind === "mux" || n.kind === "register"
                      ? 100
                      : n.kind === "or"
                        ? 140
                        : 167
                  }
                >
                  {n.kind === "input"
                    ? "I"
                    : n.kind === "output"
                      ? "O"
                      : n.kind === "register"
                        ? "D/Q"
                        : symbols[n.kind]}
                </text>
              )}
              <title>
                {n.name} · {n.kind.toUpperCase()} · {n.width} bit
              </title>
            </g>
          );
        })}
        {marquee && <rect className="selection-marquee" {...marquee} />}
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
