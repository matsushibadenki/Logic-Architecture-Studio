# Native node delta rendering

Measured 2026-11-09 on Apple M4 / Metal, macOS 27.2 (26B123). Release build, 1600×1000 offscreen target. Raw measurements: [apple-m4-node-delta.json](performance/apple-m4-node-delta.json).

## Workload and method

The generator creates 100, 500 or 2,000 nodes on a 40-column grid: two inputs followed by ADD nodes, each connected to its two predecessors. The largest case has 3,996 edges and reaches the application's current node limit. A middle node alternates between two positions 18 world units apart. Overview zoom is 0.1; detail zoom is 0.8 around the changed node. Overview intentionally uses the existing text/port LOD.

Each case warms up for five frames, then measures 30 frames. Both paths receive the same changed document and produce byte-identical Quads and equal text labels on every sample. The full path validates and constructs a fresh projection/index/scene; the delta path updates its retained projection/index and assembles cached fragments. GPU timing uses Metal timestamp queries; offscreen completion waits are measured separately. The final run ran without the native app or GPU correctness tests running concurrently. It does not benchmark canonical patch validation, IPC, WebViews, window presentation, display refresh or end-to-end pointer latency.

## Results

CPU projection + scene time, milliseconds:

| Nodes / edges | View | Full p50 / p95 | Delta p50 / p95 |
|---|---|---:|---:|
| 100 / 196 | Overview | 0.628 / 0.704 | 0.137 / 0.152 |
| 100 / 196 | Detail | 0.612 / 0.783 | 0.117 / 0.152 |
| 500 / 996 | Overview | 4.128 / 4.318 | 0.732 / 0.764 |
| 500 / 996 | Detail | 3.785 / 3.904 | 0.519 / 0.531 |
| 2,000 / 3,996 | Overview | 31.396 / 33.066 | 3.594 / 3.764 |
| 2,000 / 3,996 | Detail | 28.724 / 30.310 | 2.090 / 2.112 |

Largest case, final measured single-node move:

| Metric | Overview | Detail |
|---|---:|---:|
| Visible nodes / edges | 2,000 / 3,996 | 56 / 113 |
| Regenerated nodes / edges | 1 / 4 | 1 / 4 |
| Reused nodes / edges | 1,999 / 3,992 | 55 / 109 |
| Full Quad data | 833,424 B | 101,424 B |
| Quad bytes uploaded | 1,536 B | 3,600 B |
| Quad write calls | 5 | 5 |
| Glyph bytes uploaded | 0 B | 672 B |
| GPU render-pass p50 / p95 | 1.290 / 1.319 ms | 1.000 / 1.014 ms |
| Completed offscreen frame p50 / p95 | 4.040 / 4.095 ms | 4.081 / 4.126 ms |

Completed offscreen frame time includes GPU preparation, submission and completion wait, but **excludes** the CPU projection + scene time shown above. These samples are not an application FPS or interaction-latency guarantee. Dense fanout, all-label workloads, bulk edits and other GPUs can behave differently. Delta detection still scans the canonical snapshot; assembly and upload comparison still scan visible data. Structural edits, large moves or spatial-overlay limits can trigger a full index rebuild; zoom/presentation changes invalidate fragment caches. The whole visible render pass remains necessary for a newly acquired surface.

## Correctness and live QA

- CPU equivalence: movement, rename, kind/port changes, rewiring, node deletion, restoration, vector reordering and import; English/Japanese/Simplified Chinese, dark/light and multiple zoom levels.
- 2,000-node test asserts exactly one regenerated node and four incident edges, then compares all geometry/text against a fresh index. An unchanged follow-up regenerates no fragments.
- Metal pixel equivalence: retained buffers versus fresh GPU buffers after move, text edit, selection, growth/reallocation and shrink. An unchanged frame uploads zero Quad/glyph instance bytes. Three-language rendering also passes.
- Native macOS app: selection/inspector sync, node move/cable tracking, disconnect and pointer reconnect, rename/Undo, zoom, add/delete, library drag left→right→bottom→left, all three panels floating/redocking, result close/reopen, all splitters and application restart. Test edits were undone or removed; the original circuit topology and positions were retained. ALU verification passed 320/320 afterward.
- ⭕️ [Pending] Physical mixed-DPI multi-monitor QA and Windows/Linux native validation.

## Reproduce

```sh
cargo run --release -p studio-renderer --example large_circuit > /tmp/las-large-circuit.json
cargo test -p studio-renderer
cargo test -p studio-renderer --test gpu -- --ignored
```

GPU commands require access to a real desktop GPU and system fonts. The benchmark uses generated in-memory documents and does not modify the user's workspace.

## Live service / WebView follow-up

Actual native pointer-release, canonical commit, WebView sync/React and native present-request measurements are recorded separately in [WEBVIEW-PERFORMANCE.md](WEBVIEW-PERFORMANCE.md).
