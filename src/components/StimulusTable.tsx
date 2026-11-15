import { useRef, useState } from "react";
import {
  Plus,
  Copy,
  Trash2,
  Save,
  Upload,
  Download,
  Play,
  Undo2,
  FilePlus2,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { action, native, nativeAction } from "../store";
import { simulate } from "../simulation";
import type { Snapshot, Stimulus } from "../types";
import { translation } from "../i18n";
import { Hint, IconButton } from "./ui";
import { download } from "./Views";
const emptyTable = { version: 0, rows: [] as Stimulus[] };
export function StimulusTable({
  snapshot,
  busy,
}: {
  snapshot: Snapshot;
  busy: boolean;
}) {
  const t = translation(snapshot.locale);
  const saved = snapshot.testbench ?? emptyTable;
  // Only uncommitted form edits are local. Applied rows/version belong to Rust.
  const [draft, setDraft] = useState<{
    version: number;
    revision: number;
    rows: Stimulus[];
  } | null>(null);
  const [running, setRunning] = useState(false);
  const [engine, setEngine] = useState("rust");
  const [page, setPage] = useState(0);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const file = useRef<HTMLInputElement>(null);
  const rows = draft?.rows ?? saved.rows;
  const inputs = snapshot.document.nodes.filter((n) => n.kind === "input");
  const outputs = snapshot.document.nodes.filter((n) => n.kind === "output");
  const sequential = snapshot.document.nodes.some((n) => n.kind === "register");
  const conflict =
    !!draft &&
    (draft.version !== saved.version || draft.revision !== snapshot.revision);
  const mismatch = rows.some(
    (r) =>
      Object.keys(r.inputs).length !== inputs.length ||
      inputs.some((n) => !(n.name in r.inputs)) ||
      Object.keys(r.expected).some(
        (name) => !outputs.some((n) => n.name === name),
      ),
  );
  const disabled = busy || loading || running;
  const start = Math.min(page, Math.max(0, Math.ceil(rows.length / 8) - 1)) * 8;
  function edit(next: Stimulus[]) {
    setDraft({
      version: draft?.version ?? saved.version,
      revision: draft?.revision ?? snapshot.revision,
      rows: next,
    });
    setError("");
  }
  function change(index: number, next: Stimulus) {
    edit(rows.map((r, i) => (i === index ? next : r)));
  }
  function add() {
    edit([
      ...rows,
      {
        reset: sequential && rows.length === 0,
        inputs: Object.fromEntries(
          inputs.map((n) => [n.name, rows.at(-1)?.inputs[n.name] ?? "0"]),
        ),
        expected: {},
      },
    ]);
    setPage(Math.floor(rows.length / 8));
  }
  async function save() {
    if (!draft) return;
    const result = await action("testbench.set", {
      expected_revision: draft.revision,
      expected_version: draft.version,
      rows: draft.rows,
    });
    if (result) setDraft(null);
  }
  async function importCsv(selected: File) {
    setLoading(true);
    setError("");
    try {
      if (selected.size > 256 * 1024) throw Error(t("csvTooLarge"));
      const source = new TextDecoder("utf-8", { fatal: true }).decode(
        await selected.arrayBuffer(),
      );
      const result = await action("testbench.import_csv", {
        expected_revision: snapshot.revision,
        expected_version: saved.version,
        source,
      });
      if (result) {
        setDraft(null);
        setPage(0);
      }
    } catch (e) {
      setError(e instanceof TypeError ? t("csvTooLarge") : String(e));
    } finally {
      setLoading(false);
    }
  }
  async function exportCsv() {
    if (native)
      await nativeAction("save_testbench_csv", {
        expectedRevision: snapshot.revision,
        expectedVersion: saved.version,
      });
    else {
      const result = await action<{ source: string }>("testbench.export_csv", {
        expected_revision: snapshot.revision,
        expected_version: saved.version,
      });
      if (result) download(result.source, "stimuli.csv");
    }
  }
  return (
    <section className="stimulus-editor" aria-label={t("stimuli")}>
      <div className="trace-toolbar stimulus-toolbar">
        <IconButton
          label={t("addCycle")}
          icon={Plus}
          disabled={disabled || rows.length >= 256 || conflict || mismatch}
          onClick={add}
        />
        <IconButton
          label={t("saveStimuli")}
          icon={Save}
          disabled={disabled || !draft || conflict || !rows.length || mismatch}
          onClick={() => void save()}
        />
        <IconButton
          label={t("discardChanges")}
          icon={Undo2}
          disabled={disabled || !draft}
          onClick={() => {
            setDraft(null);
            setError("");
          }}
        />
        <IconButton
          label={t("newStimuli")}
          icon={FilePlus2}
          disabled={disabled || !!draft}
          onClick={() =>
            edit([
              {
                reset: sequential,
                inputs: Object.fromEntries(inputs.map((n) => [n.name, "0"])),
                expected: {},
              },
            ])
          }
        />
        <span className="toolbar-separator" />
        <IconButton
          label={t("importStimuli")}
          icon={Upload}
          disabled={disabled || !!draft}
          onClick={() => file.current?.click()}
        />
        <IconButton
          label={t("exportStimuli")}
          icon={Download}
          disabled={disabled || !!draft || !rows.length || mismatch}
          onClick={() => void exportCsv()}
        />
        <input
          ref={file}
          hidden
          type="file"
          accept=".csv,text/csv"
          aria-label={t("importStimuli")}
          onChange={(e) => {
            const selected = e.target.files?.[0];
            e.target.value = "";
            if (selected) void importCsv(selected);
          }}
        />
        <label className="radix-control">
          <span>{t("simulator")}</span>
          <select
            aria-label={t("simulator")}
            value={engine}
            disabled={disabled}
            onChange={(e) => setEngine(e.target.value)}
          >
            <option value="rust">Rust</option>
            <option value="icarus">Icarus Verilog</option>
            <option value="verilator">Verilator</option>
          </select>
        </label>
        <Hint text={t("externalSimulationHint")} />
        <button
          className="primary"
          disabled={disabled || !!draft || !rows.length || mismatch}
          onClick={async () => {
            setRunning(true);
            try {
              await simulate(
                engine === "rust"
                  ? "simulation.testbench"
                  : "simulation.rtl_testbench",
                {
                  ...(engine === "rust" ? {} : { engine }),
                  expected_revision: snapshot.revision,
                  expected_version: saved.version,
                },
              );
            } finally {
              setRunning(false);
            }
          }}
        >
          <Play size={14} />
          {t(running ? "runningSimulation" : "runStimuli")}
        </button>
        <span className="stimulus-status" role="status">
          {rows.length}/256 ·{" "}
          {draft ? t("unsavedPreferences") : t("savedPreferences")}
        </span>
        <Hint text={t("stimuliHint")} />
      </div>
      {(conflict || mismatch || error) && (
        <div role="alert" className="stimulus-warning">
          {error || (conflict ? t("stimuliConflict") : t("stimuliSchema"))}
        </div>
      )}
      {!rows.length ? (
        <button className="stimulus-empty" onClick={add} disabled={disabled}>
          <Plus size={16} />
          {t("stimuliEmpty")}
        </button>
      ) : (
        <>
          <div
            className="stimulus-scroll"
            role="region"
            aria-label={t("stimuli")}
            tabIndex={0}
          >
            <table>
              <thead>
                <tr>
                  <th>#</th>
                  <th>{t("resetSignal")}</th>
                  {inputs.map((n) => (
                    <th key={n.id}>
                      {n.name}
                      <small>
                        {n.width}
                        {n.signed ? "s" : "u"}
                      </small>
                    </th>
                  ))}
                  {outputs.map((n) => (
                    <th key={n.id}>
                      {n.name}
                      <small>
                        {t("expectedValue")} · {n.width}
                        {n.signed ? "s" : "u"}
                      </small>
                    </th>
                  ))}
                  <th />
                </tr>
              </thead>
              <tbody>
                {rows.slice(start, start + 8).map((row, offset) => {
                  const i = start + offset;
                  return (
                    <tr key={i}>
                      <th scope="row">{i + 1}</th>
                      <td>
                        <input
                          type="checkbox"
                          checked={row.reset}
                          disabled={disabled || conflict}
                          aria-label={`${t("sample")} ${i + 1} · ${t("resetSignal")}`}
                          onChange={(e) =>
                            change(i, { ...row, reset: e.target.checked })
                          }
                        />
                      </td>
                      {inputs.map((n) => (
                        <td key={n.id}>
                          <input
                            aria-label={`${t("sample")} ${i + 1} · ${n.name}`}
                            value={row.inputs[n.name] ?? ""}
                            maxLength={20}
                            disabled={disabled || conflict}
                            spellCheck={false}
                            onChange={(e) =>
                              change(i, {
                                ...row,
                                inputs: {
                                  ...row.inputs,
                                  [n.name]: e.target.value,
                                },
                              })
                            }
                          />
                        </td>
                      ))}
                      {outputs.map((n) => (
                        <td key={n.id}>
                          <input
                            aria-label={`${t("sample")} ${i + 1} · ${t("expectedValue")} ${n.name}`}
                            placeholder="—"
                            title={t("noExpectation")}
                            value={row.expected[n.name] ?? ""}
                            maxLength={20}
                            disabled={disabled || conflict}
                            spellCheck={false}
                            onChange={(e) => {
                              const expected = { ...row.expected };
                              if (e.target.value)
                                expected[n.name] = e.target.value;
                              else delete expected[n.name];
                              change(i, { ...row, expected });
                            }}
                          />
                        </td>
                      ))}
                      <td className="stimulus-actions">
                        <IconButton
                          label={`${t("duplicateCycle")} ${i + 1}`}
                          icon={Copy}
                          disabled={disabled || conflict || rows.length >= 256}
                          onClick={() =>
                            edit([
                              ...rows.slice(0, i + 1),
                              structuredClone(row),
                              ...rows.slice(i + 1),
                            ])
                          }
                        />
                        <IconButton
                          label={`${t("deleteCycle")} ${i + 1}`}
                          icon={Trash2}
                          disabled={disabled || conflict || rows.length === 1}
                          onClick={() => edit(rows.filter((_, j) => i !== j))}
                        />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {rows.length > 8 && (
            <div className="trace-toolbar">
              <IconButton
                label="←"
                icon={ChevronLeft}
                disabled={start === 0}
                onClick={() => setPage(start / 8 - 1)}
              />
              <span>
                {start + 1}–{Math.min(start + 8, rows.length)} / {rows.length}
              </span>
              <IconButton
                label="→"
                icon={ChevronRight}
                disabled={start + 8 >= rows.length}
                onClick={() => setPage(start / 8 + 1)}
              />
            </div>
          )}
        </>
      )}
    </section>
  );
}
