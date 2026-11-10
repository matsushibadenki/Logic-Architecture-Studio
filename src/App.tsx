import { WorkspaceOpener } from "./components/WorkspaceOpener";
import { ackFrame } from "./measurement";
import { Splitter } from "./components/LayoutGesture";
import { useEffect, useState } from "react";
import {
  Cpu,
  FolderOpen,
  Save,
  Undo2,
  Redo2,
  Download,
  Play,
  ChevronRight,
  PanelLeftClose,
  PanelRightClose,
  Monitor,
  RotateCcw,
  TriangleAlert,
  X,
  LoaderCircle,
  Globe,
  Activity,
} from "lucide-react";
import {
  useStudio,
  action,
  refresh,
  clearError,
  native,
  nativeAction,
} from "./store";
import { translation, type Key } from "./i18n";
import { Panel, IconButton, Hint, DropZone } from "./components/ui";
import { Hierarchy, Library } from "./components/Sidebar";
import { Inspector } from "./components/Inspector";
import { Results } from "./components/Results";
import { CircuitCanvas } from "./components/Canvas";
import { RtlView, Experiments, download } from "./components/Views";
const panelView = new URLSearchParams(location.search).get("panel");
export default function App() {
  const { snapshot, busy, error, connected } = useStudio();
  const [mode, setMode] = useState<"architecture" | "rtl" | "experiments">(
    "architecture",
  );
  const [left, setLeft] = useState(true),
    [right, setRight] = useState(true);
  const t = translation(snapshot?.locale ?? "ja");
  useEffect(() => {
    if (snapshot) ackFrame(snapshot.revision);
  }, [snapshot?.revision]);
  useEffect(() => {
    document.documentElement.lang =
      snapshot?.locale === "zh" ? "zh-CN" : (snapshot?.locale ?? "ja");
  }, [snapshot?.locale]);
  useEffect(() => {
    function keyboard(e: KeyboardEvent) {
      if (
        native &&
        (e.metaKey || e.ctrlKey) &&
        e.altKey &&
        ["ArrowLeft", "ArrowRight", "ArrowDown"].includes(e.key)
      ) {
        e.preventDefault();
        void nativeAction("panel_splitter_focus", {
          zone:
            e.key === "ArrowLeft"
              ? "left"
              : e.key === "ArrowRight"
                ? "right"
                : "bottom",
        });
        return;
      }
      if (
        !(e.metaKey || e.ctrlKey) ||
        ["INPUT", "TEXTAREA", "SELECT"].includes(
          (e.target as HTMLElement).tagName,
        )
      )
        return;
      if (e.key === "z") {
        e.preventDefault();
        if (snapshot)
          void action(e.shiftKey ? "design.redo" : "design.undo", {
            expected_revision: snapshot.revision,
          });
      }
    }
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  }, [snapshot]);
  const split = new URLSearchParams(location.search).get("split");
  if (split) return <Splitter zone={split} />;
  if (!snapshot)
    return (
      <main className="startup">
        <Cpu size={40} />
        <h1>Logic Architecture Studio</h1>
        <p>{error ? t("offline") : t("loading")}</p>
        {error ? (
          <>
            <code>{t("startCore")}</code>
            <button className="primary" onClick={() => void refresh()}>
              {t("retry")}
            </button>
          </>
        ) : (
          <LoaderCircle className="spin" />
        )}
      </main>
    );
  const contents: Record<string, React.ReactNode> = {
    library: <Library snapshot={snapshot} />,
    inspector: <Inspector snapshot={snapshot} />,
    results: <Results snapshot={snapshot} busy={busy} />,
  };
  const positions = {
    library: snapshot.panels.library ?? "left",
    inspector: snapshot.panels.inspector ?? "right",
    results: snapshot.panels.results ?? "bottom",
  };
  function panels(position: string) {
    return Object.entries(positions)
      .filter(([, p]) => p === position)
      .map(([id]) => (
        <Panel key={id} id={id} title={id as Key} locale={snapshot!.locale}>
          {contents[id]}
        </Panel>
      ));
  }
  const toast = error ? (
    <div className="toast" role="alert">
      <TriangleAlert size={17} />
      <div>
        <strong>{t("error")}</strong>
        <span>{error}</span>
      </div>
      <IconButton icon={X} label={t("close")} onClick={clearError} />
    </div>
  ) : null;
  if (new URLSearchParams(location.search).has("native"))
    return (
      <main className="native-toolbar">
        <WorkspaceOpener locale={snapshot.locale} showPath disabled={busy} />
        <strong>Logic Architecture Studio</strong>
        <button
          disabled={!snapshot.can_undo}
          onClick={() =>
            void action("design.undo", { expected_revision: snapshot.revision })
          }
        >
          {t("undo")}
        </button>
        <button
          disabled={!snapshot.can_redo}
          onClick={() =>
            void action("design.redo", { expected_revision: snapshot.revision })
          }
        >
          {t("redo")}
        </button>
        <button onClick={() => void action("verify.run_tests")}>
          {t("run")}
        </button>
        <span className="toolbar-spacer" />
        {["library", "inspector", "results"].map((id) => (
          <button
            key={id}
            aria-label={`${t(id as Key)} · ${t("view")}`}
            title={t(id as Key)}
            className={
              positions[id as keyof typeof positions] === "hidden"
                ? ""
                : "active"
            }
            onClick={() =>
              void nativeAction("panel_place", {
                panel: id,
                position:
                  positions[id as keyof typeof positions] === "hidden"
                    ? "restore"
                    : "hidden",
              })
            }
          >
            {t(id as Key)}
          </button>
        ))}
        <select
          aria-label={t("language")}
          value={snapshot.locale}
          onChange={(e) =>
            void action("view.preferences", { locale: e.target.value })
          }
        >
          <option value="en">English</option>
          <option value="ja">日本語</option>
          <option value="zh">简体中文</option>
        </select>
        {toast}
      </main>
    );
  if (panelView && contents[panelView])
    return (
      <main className="floating-shell">
        <Panel
          id={panelView}
          title={panelView as Key}
          locale={snapshot.locale}
          float
        >
          {panelView === "library" &&
          new URLSearchParams(location.search).has("docked") ? (
            <>
              <Hierarchy snapshot={snapshot} />
              {contents[panelView]}
            </>
          ) : (
            contents[panelView]
          )}
        </Panel>
        {toast}
      </main>
    );
  return (
    <main
      className={`studio ${left ? "" : "left-hidden"} ${right ? "" : "right-hidden"}`}
    >
      <header className="app-header">
        <div className="brand">
          <Cpu size={23} />
          <strong>Logic Architecture Studio</strong>
        </div>
        <span className="header-rule" />
        <span className="project-name">
          {snapshot.document.name.toLowerCase()}.logic
        </span>
        <span className="toolbar-spacer" />
        <div className="header-actions">
          <label
            className="icon-button"
            title={t("importProject")}
            style={{ cursor: "pointer" }}
          >
            <FolderOpen size={16} />
            <input
              type="file"
              accept=".logic,.json"
              aria-label={t("importProject")}
              style={{ display: "none" }}
              disabled={busy}
              onChange={async (e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (!file) return;
                if (file.size > 900000) {
                  await action("project.import", {
                    expected_revision: snapshot.revision,
                    project: { schema: 0 },
                  });
                  return;
                }
                let project: unknown;
                try {
                  project = JSON.parse(await file.text());
                } catch {
                  project = { schema: 0 };
                }
                await action("project.import", {
                  expected_revision: snapshot.revision,
                  project,
                });
              }}
            />
          </label>
          <IconButton
            icon={Save}
            label={t("exportProject")}
            disabled={busy}
            onClick={async () => {
              const project = await action("project.export");
              if (project)
                download(
                  JSON.stringify(project, null, 2),
                  `${snapshot.document.name}.logic`,
                );
            }}
          />

          <IconButton
            icon={Undo2}
            label={t("undo")}
            disabled={busy || !snapshot.can_undo}
            onClick={() =>
              void action("design.undo", {
                expected_revision: snapshot.revision,
              })
            }
          />
          <IconButton
            icon={Redo2}
            label={t("redo")}
            disabled={busy || !snapshot.can_redo}
            onClick={() =>
              void action("design.redo", {
                expected_revision: snapshot.revision,
              })
            }
          />
          <IconButton
            icon={Download}
            label={t("save")}
            disabled={busy}
            onClick={async () => {
              const r = await action<{ source: string }>("rtl.generate");
              if (r) download(r.source, `${snapshot.document.name}.v`);
            }}
          />
          <span className="separator" />
          <button
            className="primary run-button"
            disabled={busy || !connected}
            onClick={() => void action("verify.run_tests")}
          >
            {busy ? (
              <LoaderCircle size={15} className="spin" />
            ) : (
              <Play size={15} />
            )}
            <span>{t("run")}</span>
          </button>
        </div>
      </header>
      <nav className="mode-bar">
        <div className="mode-tabs" role="tablist">
          {(["architecture", "rtl", "experiments"] as const).map((m) => (
            <button
              role="tab"
              aria-selected={mode === m}
              className={mode === m ? "selected" : ""}
              key={m}
              onClick={() => setMode(m)}
            >
              {t(m)}
            </button>
          ))}
        </div>
        <span className="toolbar-spacer" />
        <div className="view-actions">
          <IconButton
            label={t("left")}
            icon={PanelLeftClose}
            active={left}
            onClick={() => setLeft(!left)}
          />
          <IconButton
            label={t("right")}
            icon={PanelRightClose}
            active={right}
            onClick={() => setRight(!right)}
          />
          <IconButton
            label={t("resetLayout")}
            icon={RotateCcw}
            onClick={async () => {
              for (const [panel, position] of Object.entries({
                library: "left",
                inspector: "right",
                results: "bottom",
              })) {
                await action("view.preferences", { panel, position });
                if (native) await nativeAction("dock_panel", { panel });
              }
              setLeft(true);
              setRight(true);
            }}
          />
          <IconButton
            label={t("gpu")}
            icon={Monitor}
            disabled={!native}
            onClick={() => void nativeAction("gpu_view")}
          />
          <Hint text={t("gpuHint")} />
        </div>
      </nav>
      <div className="workspace">
        <aside className="left-sidebar">
          <Hierarchy snapshot={snapshot} />
          {panels("left")}
        </aside>
        <section className="center-workspace">
          <div className="main-view">
            {mode === "architecture" ? (
              <CircuitCanvas snapshot={snapshot} />
            ) : mode === "rtl" ? (
              <RtlView snapshot={snapshot} />
            ) : (
              <Experiments snapshot={snapshot} busy={busy} />
            )}
          </div>
          {panels("bottom")}
        </section>
        <aside className="right-sidebar">{panels("right")}</aside>
        <DropZone position="left" locale={snapshot.locale} />
        <DropZone position="right" locale={snapshot.locale} />
        <DropZone position="bottom" locale={snapshot.locale} />
      </div>
      <footer className="status-bar">
        <span className={`status-dot ${connected ? "" : "unavailable"}`} />
        <span>{t("core")}</span>
        <ChevronRight size={11} />
        <code>r{String(snapshot.revision).padStart(3, "0")}</code>
        <span className="status-local">{t("local")}</span>
        <span className="toolbar-spacer" />
        {Object.entries(positions)
          .filter(([, p]) => p === "floating")
          .map(([id]) => (
            <button
              key={id}
              className="floating-indicator"
              onClick={() => void nativeAction("float_panel", { panel: id })}
            >
              {t(id as Key)} ↗
            </button>
          ))}
        <span className="mcp-label" title={t("readonlyHint")}>
          <Activity size={12} />
          MCP
        </span>
        <label className="language-picker">
          <Globe size={12} />
          <select
            aria-label={t("language")}
            value={snapshot.locale}
            onChange={(e) =>
              void action("view.preferences", { locale: e.target.value })
            }
          >
            <option value="en">English</option>
            <option value="ja">日本語</option>
            <option value="zh">简体中文</option>
          </select>
        </label>
      </footer>
      {toast}
    </main>
  );
}
