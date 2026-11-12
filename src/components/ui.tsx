import { useLayoutDrag } from "./LayoutGesture";
import type { ReactNode, ComponentType } from "react";
import {
  Info,
  PanelLeft,
  PanelRight,
  PanelBottom,
  ExternalLink,
  ArrowDownLeft,
  X,
} from "lucide-react";
import { action, native, nativeAction } from "../store";
import { translation, type Key } from "../i18n";
import type { Locale } from "../types";
export function IconButton({
  label,
  icon: Icon,
  onClick,
  active = false,
  disabled = false,
}: {
  label: string;
  icon: ComponentType<{ size?: number }>;
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className={`icon-button ${active ? "active" : ""}`}
      title={label}
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
    >
      <Icon size={16} />
    </button>
  );
}
export function Hint({ text }: { text: string }) {
  return (
    <span className="hint" tabIndex={0} role="note" aria-label={text}>
      <Info size={13} />
      <span className="tooltip">{text}</span>
    </span>
  );
}
export function Panel({
  id,
  title,
  locale,
  children,
  float = false,
}: {
  id: string;
  title: Key;
  locale: Locale;
  children: ReactNode;
  float?: boolean;
}) {
  const t = translation(locale);
  const drag = useLayoutDrag("dock", id);
  async function dock(position: string) {
    if (native) await nativeAction("panel_place", { panel: id, position });
    else await action("view.preferences", { panel: id, position });
  }
  return (
    <section className={`panel panel-${id}`} aria-label={t(title)}>
      <header
        className="panel-header"
        {...(native ? drag : {})}
        draggable={!native}
        onDragStart={(e) => {
          e.dataTransfer.setData("text/panel", id);
          document.body.classList.add("dragging-panel");
        }}
        onDragEnd={() => document.body.classList.remove("dragging-panel")}
      >
        <h2>{t(title)}</h2>
        <div className="panel-actions">
          <details>
            <summary aria-label={t("view")} title={t("panelHint")}>
              ···
            </summary>
            <div className="panel-menu">
              {(["left", "right", "bottom"] as const).map((p, i) => {
                const I = [PanelLeft, PanelRight, PanelBottom][i];
                return (
                  <button key={p} onClick={() => void dock(p)}>
                    <I size={14} />
                    {t(p)}
                  </button>
                );
              })}
              <button
                disabled={!native}
                title={!native ? t("nativeOnly") : undefined}
                onClick={async () => {
                  await nativeAction("panel_place", {
                    panel: id,
                    position: "floating",
                  });
                }}
              >
                <ExternalLink size={14} />
                {t("float")}
              </button>
            </div>
          </details>
          <IconButton
            label={t("close")}
            icon={X}
            onClick={() => {
              if (native)
                void nativeAction("panel_place", {
                  panel: id,
                  position: "hidden",
                });
              else
                void action("view.preferences", {
                  panel: id,
                  position: "hidden",
                });
            }}
          />
          {float ? (
            <IconButton
              label={t("dock")}
              icon={ArrowDownLeft}
              onClick={() =>
                void nativeAction("panel_place", {
                  panel: id,
                  position: "restore",
                })
              }
            />
          ) : null}
        </div>
      </header>
      <div className="panel-body">{children}</div>
    </section>
  );
}
export function DropZone({
  position,
  locale,
}: {
  position: "left" | "right" | "bottom";
  locale: Locale;
}) {
  return (
    <div
      className={`drop-zone drop-${position}`}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        const panel = e.dataTransfer.getData("text/panel");
        document.body.classList.remove("dragging-panel");
        if (panel) void action("view.preferences", { panel, position });
      }}
    >
      {translation(locale)(position)}
    </div>
  );
}
