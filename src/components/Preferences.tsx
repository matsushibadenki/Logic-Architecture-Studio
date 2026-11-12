import { useEffect, useState, type CSSProperties } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Palette,
  Settings,
  RotateCcw,
  Check,
  LoaderCircle,
  X,
} from "lucide-react";
import { useStudio, action, native, refresh, clearError } from "../store";
import { translation } from "../i18n";
import { kinds, symbols, type Kind, type Locale } from "../types";
import { defaultNodeColors, nodeColor } from "../appearance";
import { Hint, IconButton } from "./ui";

export default function Preferences() {
  const { snapshot, busy, error } = useStudio();
  // Form drafts only; applied preferences live in the shared Rust service.
  const [localeEdit, setLocaleEdit] = useState<Locale | null>(null);
  const [colorEdits, setColorEdits] = useState<Partial<Record<Kind, string>>>(
    {},
  );
  const locale = localeEdit ?? snapshot?.locale ?? "ja";
  const t = translation(locale);
  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : locale;
    document.title = t("preferences");
    if (native)
      void getCurrentWindow()
        .setTitle(t("preferences"))
        .catch(() => {});
  }, [locale]);
  if (!snapshot)
    return (
      <main className="preferences-loading">
        <Settings size={28} />
        <p>{t("loading")}</p>
        {error && <button onClick={() => void refresh()}>{t("retry")}</button>}
      </main>
    );
  const name = (kind: Kind) =>
    kind === "input"
      ? t("inputs")
      : kind === "output"
        ? t("outputs")
        : kind === "register"
          ? t("registerKind")
          : kind.toUpperCase();
  const valid = (value: string) => /^#[\da-f]{6}$/i.test(value);
  const changed = Object.fromEntries(
    Object.entries(colorEdits).filter(
      ([kind, value]) =>
        value?.toLowerCase() !== nodeColor(snapshot, kind as Kind),
    ),
  );
  const languageChanged = locale !== snapshot.locale;
  const dirty = languageChanged || Object.keys(changed).length > 0;
  const invalid = Object.values(colorEdits).some((color) => !valid(color));
  function discard() {
    setLocaleEdit(null);
    setColorEdits({});
    clearError();
  }
  async function save() {
    if (!dirty || invalid || busy) return;
    const result = await action("view.preferences", {
      ...(languageChanged ? { locale } : {}),
      node_colors: changed,
    });
    if (result !== undefined) {
      setLocaleEdit(null);
      setColorEdits({});
    }
  }
  return (
    <main className="preferences-shell">
      <aside className="preferences-sidebar">
        <header>
          <Settings size={19} />
          <strong>{t("preferences")}</strong>
        </header>
        <nav
          role="tablist"
          aria-label={t("preferences")}
          aria-orientation="vertical"
        >
          <button
            role="tab"
            id="appearance-tab"
            aria-selected="true"
            aria-controls="appearance-panel"
            className="preferences-tab"
          >
            <Palette size={18} />
            {t("appearance")}
          </button>
        </nav>
        <div className="preferences-workspace" title={snapshot.document.name}>
          {snapshot.document.name}
        </div>
      </aside>
      <form
        className="preferences-pane"
        role="tabpanel"
        id="appearance-panel"
        aria-labelledby="appearance-tab"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <header className="preferences-heading">
          <h1>{t("appearance")}</h1>
          <Hint text={t("preferencesScope")} />
        </header>
        <div className="preferences-scroll">
          <section className="preference-language">
            <label htmlFor="preference-language">{t("language")}</label>
            <select
              id="preference-language"
              value={locale}
              disabled={busy}
              onChange={(e) => setLocaleEdit(e.target.value as Locale)}
            >
              <option value="en">English</option>
              <option value="ja">日本語</option>
              <option value="zh">简体中文</option>
            </select>
          </section>
          <section className="preferences-colors" aria-label={t("nodeColors")}>
            <header>
              <h2>{t("nodeColors")}</h2>
              <button
                type="button"
                disabled={busy}
                className="preferences-reset"
                onClick={() => setColorEdits({ ...defaultNodeColors })}
              >
                <RotateCcw size={13} />
                {t("restoreColors")}
              </button>
            </header>
            {kinds.map((kind) => {
              const color = colorEdits[kind] ?? nodeColor(snapshot, kind);
              const preview = valid(color) ? color : nodeColor(snapshot, kind);
              const symbol =
                kind === "input"
                  ? "I"
                  : kind === "output"
                    ? "O"
                    : kind === "register"
                      ? "D/Q"
                      : symbols[kind];
              return (
                <div className="preference-color-row" key={kind}>
                  <span
                    className="preference-node-swatch"
                    style={{ "--preview-color": preview } as CSSProperties}
                    aria-hidden="true"
                  >
                    {symbol}
                  </span>
                  <label htmlFor={`hex-${kind}`} className="preference-kind">
                    {name(kind)}
                  </label>
                  <input
                    type="color"
                    aria-label={`${name(kind)} · ${t("colorPicker")}`}
                    value={preview}
                    disabled={busy}
                    onChange={(e) =>
                      setColorEdits((old) => ({
                        ...old,
                        [kind]: e.target.value,
                      }))
                    }
                  />
                  <input
                    id={`hex-${kind}`}
                    className="preference-hex"
                    aria-label={`${name(kind)} · ${t("colorHex")}`}
                    aria-invalid={!valid(color)}
                    value={color}
                    disabled={busy}
                    maxLength={7}
                    spellCheck={false}
                    autoComplete="off"
                    onChange={(e) =>
                      setColorEdits((old) => ({
                        ...old,
                        [kind]: e.target.value,
                      }))
                    }
                  />
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`${name(kind)} · ${t("restoreColor")}`}
                    title={t("restoreColor")}
                    disabled={busy || color === defaultNodeColors[kind]}
                    onClick={() =>
                      setColorEdits((old) => ({
                        ...old,
                        [kind]: defaultNodeColors[kind],
                      }))
                    }
                  >
                    <RotateCcw size={14} />
                  </button>
                </div>
              );
            })}
          </section>
        </div>
        {error && (
          <div role="alert" className="preferences-error">
            <span>{error}</span>
            <IconButton label={t("close")} icon={X} onClick={clearError} />
          </div>
        )}
        <footer className="preferences-footer">
          <span role="status" className={invalid ? "invalid" : ""}>
            {busy ? (
              <LoaderCircle className="spin" size={13} />
            ) : !dirty ? (
              <Check size={13} />
            ) : null}
            {invalid
              ? t("colorInvalid")
              : busy
                ? t("savingPreferences")
                : dirty
                  ? t("unsavedPreferences")
                  : t("savedPreferences")}
          </span>
          {dirty && (
            <button type="button" disabled={busy} onClick={discard}>
              {t("discardChanges")}
            </button>
          )}
          <button
            className="primary"
            type="submit"
            disabled={!dirty || invalid || busy}
          >
            {t("applyPreferences")}
          </button>
        </footer>
      </form>
    </main>
  );
}
