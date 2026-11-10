import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";
import {
  WorkspaceOpener,
  workspaceWords,
  type WorkspaceLocale,
} from "./WorkspaceOpener";
export default function WorkspaceStart() {
  const [locale, setLocale] = useState<WorkspaceLocale>(
    navigator.language.startsWith("ja")
      ? "ja"
      : navigator.language.startsWith("zh")
        ? "zh"
        : "en",
  );
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void invoke<{ error: string | null }>("workspace_info")
      .then((info) => {
        if (active) setError(info.error ?? "");
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, []);
  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : locale;
  }, [locale]);
  const t = workspaceWords[locale];
  return (
    <main className="recovery-screen workspace-start">
      <header>
        <FolderOpen size={24} />
        <h1>{t.unavailable}</h1>
        <select
          aria-label={t.language}
          value={locale}
          onChange={(e) => setLocale(e.target.value as WorkspaceLocale)}
        >
          <option value="en">English</option>
          <option value="ja">日本語</option>
          <option value="zh">简体中文</option>
        </select>
      </header>
      <WorkspaceOpener locale={locale} />
      {error && (
        <details className="recovery-details">
          <summary>{t.details}</summary>
          <code>{error}</code>
        </details>
      )}
    </main>
  );
}
