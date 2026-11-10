import { memo, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, LoaderCircle } from "lucide-react";
export type WorkspaceLocale = "en" | "ja" | "zh";
export const workspaceWords = {
  en: {
    open: "Open workspace folder",
    unavailable: "Workspace unavailable",
    details: "Details",
    language: "Language",
  },
  ja: {
    open: "ワークスペースを開く",
    unavailable: "ワークスペースを開けません",
    details: "詳細",
    language: "言語",
  },
  zh: {
    open: "打开工作区文件夹",
    unavailable: "工作区不可用",
    details: "详情",
    language: "语言",
  },
};
export const WorkspaceOpener = memo(function WorkspaceOpener({
  locale,
  showPath = false,
  disabled = false,
}: {
  locale: WorkspaceLocale;
  showPath?: boolean;
  disabled?: boolean;
}) {
  const [path, setPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const t = workspaceWords[locale];
  useEffect(() => {
    if (!showPath) return;
    let active = true;
    void invoke<{ path: string | null }>("workspace_info")
      .then((info) => {
        if (active) setPath(info.path);
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [showPath]);
  async function choose() {
    setBusy(true);
    setError("");
    try {
      await invoke("workspace_pick", { locale });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const name = path?.split(/[\\/]/).filter(Boolean).at(-1);
  return (
    <span className="workspace-opener">
      <button
        aria-label={t.open}
        title={path ? `${t.open}\n${path}` : t.open}
        disabled={disabled || busy}
        onClick={() => void choose()}
      >
        {busy ? (
          <LoaderCircle size={15} className="spin" />
        ) : (
          <FolderOpen size={15} />
        )}
        {showPath && name ? (
          <span className="workspace-name">{name}</span>
        ) : (
          <span>{t.open}</span>
        )}
      </button>
      {error && (
        <span className="workspace-open-error" role="alert" title={error}>
          {error}
        </span>
      )}
    </span>
  );
});
