# Shared snapshots and buffered checkpoint performance

Measured 2026-11-09 on Apple M4 / Metal, macOS 27.2, release Tauri app at 1440×900. Each build used a fresh isolated 2,000-node / 3,996-edge fixture and 140 actual native header drags, alternating 20×15 screen pixels. The first two moves are warmups; statistics use the remaining 138, including the checkpoint at revision 129. All 140 moves in each run had a durable commit, native presentation request and acknowledgments from controls/library/inspector/results. Raw data: [apple-m4-history-checkpoint.json](performance/apple-m4-history-checkpoint.json).

| Final pointer event → stage | Before p50 / p95 | After p50 / p95 |
|---|---:|---:|
| Durable commit response | 26.97 / 29.97 ms | 9.44 / 11.18 ms |
| Native draw/present-call return | 34.31 / 38.53 ms | 33.35 / 34.10 ms |
| All four WebViews acknowledged paint opportunity | 59.45 / 60.80 ms | 59.98 / 61.30 ms |

The ordinary commit median fell 65.0%. Normal native/WebView latency remains dominated by scheduling/frame opportunities; the change does not establish a meaningful WebView median improvement.

The checkpoint occurs only once per run, so it is reported separately rather than hidden by p95:

| Revision 129 (checkpoint included) | Before | After |
|---|---:|---:|
| Durable commit response | 19,741.71 ms | 61.35 ms |
| Native draw/present-call return | 19,750.62 ms | 71.17 ms |
| Last WebView acknowledgment | 19,777.09 ms | 92.66 ms |

The checkpoint held 45 Undo documents, 16,860,588 bytes including current document and metadata. Before/after checkpoint JSON values matched exactly, and their final WAL bytes matched exactly. The same 128-record threshold, history limits and durability boundaries were used. The before run's UI observation timed out during checkpoint blocking; after completion r129 was observed and measurement captured the full delay. The following moves reached r140 normally. A preliminary nine-move run with a changed camera was discarded; it is absent from these reports.

## Changes

Workspace current document, Undo/Redo documents and experimental candidate documents use `Arc<Document>`. Transactional Workspace clones copy references instead of deep-copying every circuit. Each edit creates its new document once, retaining the previous document by reference for Undo; Undo/Redo swap references. Metadata vectors/maps and result metadata still clone, and each edited document still has its own node/edge arrays. This reduces transaction copies, not the logical amount of snapshot history. The on-disk schema and core/MCP JSON remain unchanged; serde's `rc` support encodes the document contents transparently.

Checkpoint output now uses an explicit 1 MiB BufWriter. The old streaming serializer wrote many tiny JSON pieces directly to File, causing excessive write syscalls. The buffer flush is checked before file fsync, atomic rename and directory fsync. WAL pruning still follows the durable checkpoint. Saving remains synchronous and may block at compaction; this implementation does not introduce a background writer or weaken acknowledgments.

## Verification and limits

- Workspace tests verify pointer sharing across transaction clones, immutable prior documents, reference reuse through Undo/Redo, JSON compatibility and reload equality. Existing revision/conflict, invalid patch, import, experiments, history budget, failed-write, WAL replay and compaction tests pass.
- Actual macOS QA: node selection/move, disconnect/reconnect, rename/Undo/Redo, add/select/delete; all three region splitters; library dragged right → bottom → left; each of library/inspector/results floated and re-docked, closed and reopened. Floating results passed 320/320 vectors.
- Restart restored the floating results window, region dimensions, edited circuit and verification result. Re-docking and persisted Undo/Redo worked (r8→r9→r10). The Unix socket/MCP integration separately kills/restarts the service after an acknowledged move and checks recovery and Undo.
- `cargo test --workspace --locked`, Clippy with `-D warnings`, format/diff checks and release app build passed. GPU offscreen tests were not repeated because rendering code did not change; actual native GPU interactions were checked. Windows/Linux and multi-monitor DPI transitions remain ⭕️ [Pending].

These are sequential real UI drags, with revision observations between moves and brief tool-call boundaries between batches. They exercise growing/bounded history and one checkpoint, not a saturated input stream or sustained preview FPS. Timing begins at the final pointer event; native return is a presentation request, and double-requestAnimationFrame WebView acknowledgment is a paint opportunity. Neither is physical scan-out/GPU completion. One checkpoint pair is evidence of this fixture, not a checkpoint tail-latency distribution. Wider fanout, larger reports/candidates, storage variability and longer sessions need more samples.

Reproduce with fresh fixture roots from `scripts/ui_benchmark.py prepare ROOT --nodes 2000`, release app launches using `LAS_WORKSPACE=ROOT LAS_UI_TRACE=/tmp/run.json`, the alternating drags, and `scripts/ui_benchmark.py summarize /tmp/before.json /tmp/after.json --moves 140 --out /tmp/report.json`. Do not build concurrently with measured input. Keep both workspace.json and workspace.wal for recovery.
