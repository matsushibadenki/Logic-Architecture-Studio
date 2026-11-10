import { WorkspaceOpener } from "./WorkspaceOpener";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArchiveRestore,
  FolderOpen,
  Check,
  Info,
  RotateCcw,
  ShieldCheck,
  TriangleAlert,
} from "lucide-react";
const words = {
  en: {
    title: "Recover workspace",
    language: "Language",
    current: "Latest checkpoint",
    selectBackup: "Choose backup",
    external: "Selected backup",
    salvage: "Recover valid moves",
    moves: "moves",
    excluded: "bytes excluded",
    previous: "Previous checkpoint",
    layout: "Reset panel layout",
    revision: "Revision",
    nodes: "nodes",
    restore: "Back up & restore",
    retry: "Retry opening",
    info: "Original files are preserved in .logic/recovery before restoration. Recover valid moves stops at the first damaged frame. Checkpoint-only restoration does not replay later circuit changes. Panel reset keeps the circuit and move journal.",
    choose: "Choose a restore point",
    noBackup:
      "No valid checkpoint is available.\nOriginal files are unchanged.\nChoose a backup or open another workspace.",
    ready: "Restored",
    backup: "Original files",
    restart: "Open recovered workspace",
    busy: "Restoring…",
    failed: "Recovery unavailable",
    details: "Details",
    refresh: "Refresh",
    layoutInfo: "Panel sizes and floating frames will return to defaults.",
  },
  ja: {
    title: "ワークスペースの復旧",
    language: "言語",
    current: "最新のチェックポイント",
    selectBackup: "バックアップを選ぶ",
    external: "選択したバックアップ",
    salvage: "正常な移動を救出",
    moves: "移動",
    excluded: "バイトを除外",
    previous: "前のチェックポイント",
    layout: "パネル配置をリセット",
    revision: "リビジョン",
    nodes: "ノード",
    restore: "退避して復元",
    retry: "開き直す",
    info: "復元前のファイルは .logic/recovery に保存します。移動の救出は最初の破損箇所で停止します。チェックポイントのみの復元では、それ以降の回路変更は再生しません。パネル配置のリセットでは回路と移動ログを保持します。",
    choose: "復元地点を選択",
    noBackup:
      "有効なチェックポイントがありません。\n元のファイルは変更していません。\nバックアップを選ぶか、別のワークスペースを開いてください。",
    ready: "復元しました",
    backup: "退避した元データ",
    restart: "復元したワークスペースを開く",
    busy: "復元中…",
    failed: "復旧処理を利用できません",
    details: "詳細",
    refresh: "更新",
    layoutInfo: "パネルの寸法と分離ウインドウの位置を既定値に戻します。",
  },
  zh: {
    title: "恢复工作区",
    language: "语言",
    current: "最新检查点",
    selectBackup: "选择备份",
    external: "所选备份",
    salvage: "恢复有效移动",
    moves: "次移动",
    excluded: "字节已排除",
    previous: "上一个检查点",
    layout: "重置面板布局",
    revision: "修订",
    nodes: "节点",
    restore: "备份并恢复",
    retry: "重新打开",
    info: "恢复前会将原文件保存到 .logic/recovery。恢复有效移动会在首个损坏帧处停止。仅恢复检查点时不会重放后续电路修改。重置面板布局会保留电路和移动日志。",
    choose: "选择恢复点",
    noBackup: "没有有效检查点。\n原文件未修改。\n请选择备份或打开其他工作区。",
    ready: "已恢复",
    backup: "原始文件备份",
    restart: "打开恢复后的工作区",
    busy: "正在恢复…",
    failed: "无法恢复",
    details: "详情",
    refresh: "刷新",
    layoutInfo: "面板尺寸和浮动窗口位置将恢复为默认值。",
  },
};
type Locale = keyof typeof words;
type Candidate = {
  id: string;
  name: string;
  revision: number;
  nodes: number;
  source?: string;
  salvage?: {
    moves: number;
    valid_bytes: number;
    discarded_bytes: number;
    stopped: string;
  };
};
type Data = {
  root: string;
  error: string;
  status: {
    token: string;
    candidates: Candidate[];
    can_reset_layout: boolean;
  };
};
export default function Recovery() {
  const [locale, setLocale] = useState<Locale>(
    navigator.language.startsWith("ja")
      ? "ja"
      : navigator.language.startsWith("zh")
        ? "zh"
        : "en",
  );
  const [data, setData] = useState<Data | null>(null);
  const [choice, setChoice] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [external, setExternal] = useState<Candidate | null>(null);
  const [selecting, setSelecting] = useState(false);
  const locked = busy || selecting;
  const [backup, setBackup] = useState("");
  const [generation, setGeneration] = useState(0);
  const t = words[locale];
  useEffect(() => {
    let active = true;
    setData(null);
    setChoice("");
    setExternal(null);
    setError("");
    void invoke<Data>("recovery_status")
      .then((value) => {
        if (active) setData(value);
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [generation]);
  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : locale;
  }, [locale]);
  async function restore() {
    if (!data || !choice || locked) return;
    setBusy(true);
    setError("");
    try {
      setBackup(
        await invoke<string>("recovery_apply", {
          token: data.status.token,
          choice,
        }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function chooseBackup() {
    if (locked) return;
    setSelecting(true);
    setError("");
    try {
      const selected = await invoke<Candidate | null>(
        "recovery_choose_backup",
        { locale },
      );
      if (selected) {
        setExternal(selected);
        setChoice("external");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setSelecting(false);
    }
  }
  function restart() {
    void invoke("recovery_restart").catch((e) => setError(String(e)));
  }
  return (
    <main className="recovery-screen">
      <header>
        <ShieldCheck size={22} />
        <h1>{t.title}</h1>
        <select
          aria-label={t.language}
          value={locale}
          onChange={(e) => setLocale(e.target.value as Locale)}
        >
          <option value="en">English</option>
          <option value="ja">日本語</option>
          <option value="zh">简体中文</option>
        </select>
      </header>
      {data && <code className="recovery-path">{data.root}</code>}
      {backup ? (
        <section className="recovery-success">
          <Check size={28} />
          <h2>{t.ready}</h2>
          <span>{t.backup}</span>
          <code className="recovery-path">{backup}</code>
          <button className="primary" onClick={restart}>
            <RotateCcw size={16} />
            {t.restart}
          </button>
        </section>
      ) : (
        <>
          <div className="recovery-caption">
            <h2>{t.choose}</h2>
            <span
              className="recovery-info"
              tabIndex={0}
              title={t.info}
              aria-label={t.info}
            >
              <Info size={16} />
            </span>
            {data && !data.status.can_reset_layout && (
              <button
                className="backup-choose"
                disabled={locked}
                onClick={() => void chooseBackup()}
              >
                <FolderOpen size={15} />
                {t.selectBackup}
              </button>
            )}
          </div>
          {[
            ...(data?.status.candidates ?? []),
            ...(external ? [external] : []),
          ].map((candidate) => (
            <label
              className={`recovery-option ${choice === candidate.id ? "selected" : ""}`}
              key={candidate.id}
            >
              <input
                type="radio"
                name="restore-point"
                value={candidate.id}
                checked={choice === candidate.id}
                disabled={locked}
                onChange={() => setChoice(candidate.id)}
              />
              <ArchiveRestore size={20} />
              <span>
                <strong>
                  {candidate.id === "external"
                    ? t.external
                    : candidate.id === "salvage"
                      ? t.salvage
                      : candidate.id === "checkpoint"
                        ? t.current
                        : t.previous}
                </strong>
                <small>
                  {candidate.name} · {t.revision} {candidate.revision} ·{" "}
                  {candidate.nodes} {t.nodes}
                </small>
                {candidate.salvage && (
                  <small title={candidate.salvage.stopped}>
                    {candidate.salvage.moves} {t.moves} ·{" "}
                    {candidate.salvage.discarded_bytes} {t.excluded}
                  </small>
                )}
                {candidate.source && (
                  <small className="recovery-path">{candidate.source}</small>
                )}
              </span>
            </label>
          ))}
          {data?.status.can_reset_layout && (
            <label
              className={`recovery-option ${choice === "layout" ? "selected" : ""}`}
            >
              <input
                type="radio"
                name="restore-point"
                value="layout"
                checked={choice === "layout"}
                disabled={locked}
                onChange={() => setChoice("layout")}
              />
              <RotateCcw size={20} />
              <span>
                <strong>{t.layout}</strong>
                <small>{t.layoutInfo}</small>
              </span>
            </label>
          )}
          {data &&
            data.status.candidates.length === 0 &&
            !external &&
            !data.status.can_reset_layout && (
              <p className="recovery-empty">{t.noBackup}</p>
            )}
          {error && (
            <p className="recovery-error" role="alert">
              <TriangleAlert size={16} />
              {t.failed}: {error}
            </p>
          )}
          {data && (
            <details className="recovery-details">
              <summary>{t.details}</summary>
              <code>{data.error}</code>
            </details>
          )}
          <footer>
            <WorkspaceOpener locale={locale} disabled={locked} />
            <button
              disabled={locked}
              onClick={() => setGeneration((value) => value + 1)}
            >
              {t.refresh}
            </button>
            <button disabled={locked} onClick={restart}>
              {t.retry}
            </button>
            <button
              className="primary"
              disabled={!choice || locked}
              onClick={() => void restore()}
            >
              <ArchiveRestore size={16} />
              {busy ? t.busy : t.restore}
            </button>
          </footer>
        </>
      )}
    </main>
  );
}
