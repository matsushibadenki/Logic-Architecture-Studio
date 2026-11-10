import { useRef, useEffect, useState } from "react";
import { nativeAction, useStudio } from "../store";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
let pending = Promise.resolve();
export function sendGesture(
  kind: string,
  panel: string,
  phase: string,
  x: number,
  y: number,
) {
  pending = pending.then(async () => {
    await nativeAction("panel_gesture", { kind, panel, phase, x, y });
  });
}
export function useLayoutDrag(kind: string, panel = "") {
  const active = useRef(false);
  useEffect(() => {
    const cancel = () => {
      if (active.current) {
        active.current = false;
        sendGesture(kind, panel, "cancel", 0, 0);
      }
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancel();
    };
    window.addEventListener("keydown", key);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", cancel);
      cancel();
    };
  }, [kind, panel]);

  return {
    onPointerDown: (e: React.PointerEvent<HTMLElement>) => {
      if (
        e.button !== 0 ||
        (e.target as HTMLElement).closest("button,summary,input,select")
      )
        return;
      active.current = true;
      e.currentTarget.setPointerCapture(e.pointerId);
      e.preventDefault();
      sendGesture(kind, panel, "start", e.screenX, e.screenY);
    },
    onPointerMove: (e: React.PointerEvent<HTMLElement>) => {
      if (active.current)
        sendGesture(kind, panel, "move", e.screenX, e.screenY);
    },
    onPointerUp: (e: React.PointerEvent<HTMLElement>) => {
      if (active.current) {
        active.current = false;
        sendGesture(kind, panel, "end", e.screenX, e.screenY);
      }
    },
    onLostPointerCapture: (e: React.PointerEvent<HTMLElement>) => {
      if (active.current) {
        active.current = false;
        sendGesture(kind, panel, "cancel", e.screenX, e.screenY);
      }
    },
    onPointerCancel: (e: React.PointerEvent<HTMLElement>) => {
      if (active.current) {
        active.current = false;
        sendGesture(kind, panel, "cancel", e.screenX, e.screenY);
      }
    },
  };
}
type SplitterMetrics = { value: number; min: number; max: number };
const splitterWords = {
  en: {
    left: "Left panel width",
    right: "Right panel width",
    bottom: "Bottom panel height",
    hint: "Arrow keys: 10 pt. Shift: 50 pt. Home/End: minimum/maximum. Cmd/Ctrl+Alt+Left/Right/Down: focus border.",
  },
  ja: {
    left: "左パネルの幅",
    right: "右パネルの幅",
    bottom: "下パネルの高さ",
    hint: "矢印キー: 10 pt、Shift: 50 pt。Home/End: 最小/最大。Cmd/Ctrl+Alt+左/右/下: 境界へ移動。",
  },
  zh: {
    left: "左侧面板宽度",
    right: "右侧面板宽度",
    bottom: "底部面板高度",
    hint: "方向键：10 pt。Shift：50 pt。Home/End：最小/最大。Cmd/Ctrl+Alt+左/右/下：聚焦边界。",
  },
};
export function Splitter({ zone }: { zone: string }) {
  const drag = useLayoutDrag(zone);
  const element = useRef<HTMLDivElement>(null);
  const { snapshot } = useStudio();
  const t = splitterWords[snapshot?.locale ?? "ja"];
  const [metrics, setMetrics] = useState<SplitterMetrics | null>(null);
  const [error, setError] = useState("");
  const queue = useRef(Promise.resolve());
  useEffect(() => {
    let disposed = false;
    const focus = getCurrentWebview().listen("splitter-focus", () =>
      element.current?.focus(),
    );
    const subscription = getCurrentWebview().listen<SplitterMetrics>(
      "splitter-state",
      (event) => {
        if (!disposed) setMetrics(event.payload);
      },
    );
    void subscription
      .then(async () => {
        const value = await invoke<SplitterMetrics>("panel_splitter_state", {
          zone,
        });
        if (!disposed) setMetrics(value);
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
      void focus.then((unlisten) => unlisten()).catch(() => {});
      void subscription.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [zone]);
  const name = t[zone as "left" | "right" | "bottom"];
  return (
    <div
      {...drag}
      ref={element}
      onPointerDown={(e) => {
        e.currentTarget.focus();
        drag.onPointerDown(e);
      }}
      onKeyDown={(e) => {
        if (
          e.altKey ||
          e.ctrlKey ||
          e.metaKey ||
          ![
            "Home",
            "End",
            ...(zone === "bottom"
              ? ["ArrowUp", "ArrowDown"]
              : ["ArrowLeft", "ArrowRight"]),
          ].includes(e.key)
        )
          return;
        e.preventDefault();
        e.stopPropagation();
        const key = e.key,
          coarse = e.shiftKey;
        queue.current = queue.current.then(async () => {
          try {
            setMetrics(
              await invoke<SplitterMetrics>("panel_splitter_key", {
                zone,
                key,
                coarse,
              }),
            );
            setError("");
          } catch (e) {
            setError(String(e));
          }
        });
      }}
      tabIndex={0}
      className={`native-splitter ${zone}`}
      role="separator"
      aria-label={name}
      aria-description={t.hint}
      title={`${name} — ${t.hint}`}
      aria-orientation={zone === "bottom" ? "horizontal" : "vertical"}
      aria-valuenow={metrics?.value}
      aria-valuemin={metrics?.min}
      aria-valuemax={metrics?.max}
      aria-valuetext={metrics ? `${Math.round(metrics.value)} pt` : undefined}
    >
      {error && (
        <span className="splitter-error" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
