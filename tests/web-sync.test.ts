import { test } from "node:test";
import assert from "node:assert/strict";
import { applySync, type Delta } from "../src/sync.ts";
import type { Snapshot } from "../src/types.ts";
import { readFileSync } from "node:fs";
const initial: Snapshot = {
  document: {
    name: "demo",
    nodes: [
      { id: "a", name: "a", kind: "input", width: 32, x: 0, y: 0 },
      { id: "b", name: "b", kind: "output", width: 32, x: 200, y: 0 },
    ],
    edges: [{ id: "e", from: "a", to: "b", port: "a" }],
  },
  revision: 0,
  history: [],
  can_undo: false,
  can_redo: false,
  report: null,
  experiments: [],
  selected: "a",
  locale: "en",
  panels: { library: "left" },
  diagnostics: [],
};
function apply(deltas: Delta[]) {
  return applySync(initial, "session:0", {
    changed: true,
    cursor: "session:1",
    base: "session:0",
    kind: "delta",
    deltas,
  });
}
test("single node update shares untouched nodes, edges and results", () => {
  const moved = { ...initial.document.nodes[0], x: 42 };
  const result = apply([
    {
      set: { revision: 1, can_undo: true },
      document: { nodes: { upsert: [moved], remove: [] } },
    },
  ]);
  assert.equal(result.document.nodes[0].x, 42);
  assert.equal(initial.document.nodes[0].x, 0);
  assert.equal(result.document.nodes[1], initial.document.nodes[1]);
  assert.equal(result.document.edges, initial.document.edges);
  assert.equal(result.experiments, initial.experiments);
});
test("ordered multi-delta replay supports delete, recreate, reorder and null", () => {
  const b = { ...initial.document.nodes[1], name: "renamed" };
  const next = apply([
    {
      set: { selected: null },
      document: {
        edges: { upsert: [], remove: ["e"] },
        nodes: { upsert: [], remove: ["b"] },
      },
    },
    {
      set: { revision: 2 },
      document: { nodes: { upsert: [b], remove: [], order: ["b", "a"] } },
    },
  ]);
  assert.deepEqual(
    next.document.nodes.map((n) => n.id),
    ["b", "a"],
  );
  assert.equal(next.selected, null);
  assert.equal(next.document.edges.length, 0);
});
test("history append and trim, experiment updates and full reset", () => {
  const history = { revision: 1, label: "move", actor: "gui" };
  const next = apply([{ set: {}, history: { upsert: [history], remove: [] } }]);
  const restored = applySync(next, "session:1", {
    kind: "snapshot",
    cursor: "new:0",
    changed: true,
    snapshot: initial,
  });
  assert.equal(restored, initial);
  assert.equal(next.history.length, 1);
});
test("experiment replacement preserves unrelated experiments", () => {
  const first = { id: "one", name: "First", base_revision: 0, candidates: [] };
  const second = {
    id: "two",
    name: "Second",
    base_revision: 0,
    candidates: [],
  };
  const snapshot = { ...initial, experiments: [first, second] };
  const response = {
    changed: true,
    cursor: "session:1",
    base: "session:0",
    kind: "delta" as const,
    deltas: [
      {
        set: {},
        experiments: { upsert: [{ ...first, name: "Edited" }], remove: [] },
      },
    ],
  };
  const next = applySync(snapshot, "session:0", response);
  assert.equal(next.experiments[0].name, "Edited");
  assert.equal(next.experiments[1], second);
  assert.equal(snapshot.experiments[0].name, "First");
});
test("bad base and invalid entity order are rejected without partial mutation", () => {
  assert.throws(() =>
    applySync(initial, "wrong", {
      kind: "delta",
      base: "session:0",
      cursor: "session:1",
      changed: true,
      deltas: [],
    }),
  );
  assert.throws(() =>
    apply([
      { set: { revision: 1 } },
      {
        set: {},
        document: { nodes: { upsert: [], remove: [], order: ["a", "a"] } },
      },
    ]),
  );
  assert.equal(initial.revision, 0);
  assert.equal(initial.document.nodes.length, 2);
});
test(
  "actual Rust-generated deltas reproduce full snapshots",
  { skip: !process.env.LAS_WEB_DELTA_FIXTURE },
  () => {
    const fixture = JSON.parse(
      readFileSync(process.env.LAS_WEB_DELTA_FIXTURE!, "utf8"),
    );
    let snapshot = fixture.initial;
    let sequence = 0;
    for (const c of fixture.cases) {
      const base = `session:${sequence++}`;
      snapshot = applySync(snapshot, base, {
        base,
        cursor: `session:${sequence}`,
        changed: true,
        kind: "delta",
        deltas: [c.delta],
      });
      assert.deepEqual(snapshot, c.expected);
    }
  },
);
