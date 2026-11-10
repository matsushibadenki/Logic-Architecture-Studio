# Sparse circuit moves and presentation latency

Measured 2026-11-09, Apple M4 / Metal, macOS 27.2, release app at 1440×900, fresh 2,000-node / 3,996-edge fixtures. Compare the same first 120 actual native header drags, alternating 20×15 screen pixels; two warmups excluded, 118 observations per build. All included revisions had durable commit, native presentation-request return and all four WebView acknowledgments. [Raw samples](performance/apple-m4-sparse-moves.json).

| Final pointer event → stage | Before p50 / p95 | After p50 / p95 |
|---|---:|---:|
| Durable commit response | 8.89 / 11.50 ms | 7.10 / 10.08 ms |
| Native draw/present-call return | 33.25 / 33.93 ms | 16.72 / 17.94 ms |
| Last of four WebView paint opportunities | 59.38 / 60.90 ms | 42.82 / 43.98 ms |

Native median fell 49.7%, WebView median 27.9%, commit median 20.1%. The comparison combines sparse storage/detection and stale-frame suppression; it does not attribute individual milliseconds to either change. Rendering still uses the existing presentation mode.

The baseline captured 140 moves. The optimized run completed 120 before its camera changed during UI automation; only the matching first 120 revisions from each build are compared. The camera change occurred after the last included sample. No missing samples are filled in. This window intentionally measures normal commits and does not cross the 128-move checkpoint threshold. Checkpoint optimization measurements are separately recorded in [HISTORY-PERFORMANCE.md](HISTORY-PERFORMANCE.md).

## Implemented path

- Canonical node/edge arrays now use ordered copy-on-write blocks of at most 32 entities, with a shared ID → index map. Document clones retain block references. Updating one node copies its block and a small block-reference table; unchanged nodes/edges remain shared. The 2,000-node case has 63 node blocks. Structural operations retain validated general paths and reconstruct identity/order as needed.
- Core position patches locate IDs through the index. Each block caches its encoded JSON length; Undo/Redo budget accounting recalculates changed blocks rather than serializing a whole circuit on every move. Array JSON, document schema, ordering and history caps remain compatible.
- Service deltas compare shared block identities, examining records only in changed blocks. Diagnostics reuse the topology on position-only changes. Native replay updates known IDs directly; structural remove/add/order changes use the general transactional reconstruction path.
- Both retained renderer projections use shared blocks to recognize coordinate-only changes and send only node move commands to UNGE/SceneIndex. The node/render-ID map is shared rather than recopied on every gesture. Fresh loads, topology/name/width changes and imports retain the general comparison path.
- After a successful native circuit commit, drawing waits for its acknowledged canonical revision. The last presented drag preview stays visible while the cache catches up; it no longer submits an old canonical frame followed by a new one. Service epoch changes clear this gate so restart cannot leave it waiting on an obsolete session.

This is bounded block copying/comparison, not a claim of O(1) for all rendering. Block-reference tables still require O(N/32) work; UNGE transactions, visible scene assembly and GPU preparation may still traverse their own data. Structural edits, imports, checkpoints and newly decoded history still have full-work paths.

## Verification

- Core tests check that a 2,000-node move changes one block, keeps old snapshots and edges shared, preserves lookup after ID/iterator/index mutations, maintains exact serialized byte counts and JSON roundtrips.
- Multi-block updates at indices 0, 31, 32, 1000 and 1999 produce the same wire delta as independent full comparison, and native replay reconstructs the same document. Existing structural reorder/remove/add, invalid-base/order, atomic patch/conflict, Undo/Redo and crash-recovery tests pass.
- Retained large-circuit tests exercise the indexed move path and match full rebuilds. Real GPU tests compare sparse-move rendering/buffer updates against fresh buffers, including later semantic changes and reallocations, and render English/Japanese/Chinese.
- Actual release-app QA: selection/move, disconnect and port drag reconnect, rename/Undo/Redo, add/select/delete; all three splitters; library dragged right → bottom → left; each panel floated/re-docked, closed/reopened. Floating results passed 320/320 ALU vectors.
- Restart restored edited circuit, selected node, sizes, result and floating window. Re-docking and persistent Undo/Redo worked (r8→r9→r10). Unix socket/MCP kill/restart integration passed. Windows/Linux and physical multi-monitor DPI QA remain ⭕️ [Pending].

These are sequential real UI gestures with revision observations between moves and brief tool-call boundaries, not a saturated stream or sustained-preview FPS measurement. Timing begins at the final pointer event and includes durability/synchronization. Native return is a presentation request; WebView double-requestAnimationFrame acknowledgment is a paint opportunity. Neither is physical scan-out or GPU completion. No builds/GPU tests ran during the performance runs. Wider fanout, multiple simultaneous moved blocks and longer sessions remain useful follow-up measurements.

Reproduce with `scripts/ui_benchmark.py prepare ROOT --nodes 2000`, isolated release launches using `LAS_WORKSPACE=ROOT LAS_UI_TRACE=/tmp/run.json`, 120 alternating header drags, and `scripts/ui_benchmark.py summarize /tmp/before-120.json /tmp/after-120.json --moves 120 --out /tmp/report.json`. Keep only complete matching revision windows; retain original captured counts when selecting a prefix.
