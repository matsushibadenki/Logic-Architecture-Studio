# Native synchronization and durable commit performance

Measured 2026-11-09 on Apple M4 / Metal, macOS 27.2, release Tauri app, 1440×900 window. Same isolated 2,000-node / 3,996-edge fixture, default panels, selected first ADD. Each run used 12 actual native header drags, alternating 20×15 screen-pixel offsets; the first two were warmups and the remaining ten form the statistics. Both runs already used WebView deltas and retained GPU rendering. The baseline executable was copied before this change; no builds ran during either measurement. Raw samples: [apple-m4-native-commit.json](performance/apple-m4-native-commit.json).

| Final pointer event → stage | Before p50 / p95 | After p50 / p95 |
|---|---:|---:|
| Successful durable commit response | 64.15 / 105.24 ms | 14.06 / 16.94 ms |
| Native draw/present-call return | 196.45 / 240.30 ms | 33.46 / 36.06 ms |
| All four WebViews acknowledged paint opportunity | 92.13 / 126.20 ms | 59.32 / 60.07 ms |

The native median fell 83.0%; durable commit median fell 78.1%. After warmup, every measured native sync packet was a delta, 279 JSON body bytes. Baseline packets contained the whole document on revision changes; that executable did not instrument native packet bytes. These results compare the combined implementation changes, not isolated attribution to each optimization.

Timing starts immediately before handling the final native pointer event that commits a move. Rust records commit and native draw/present-call return. Each of controls/library/inspector/results acknowledges its committed revision after two requestAnimationFrame callbacks, timestamped by the same Rust Instant. Results are software pipeline latency including persistence and synchronization, **not physical scan-out, GPU completion, sustained drag-preview FPS or a 60 FPS guarantee**. Ten observations give a coarse empirical p95 (the largest measured value). The fixture is a regular grid/chain; checkpoint thresholds, long histories, wide fanout and sustained input can have different tail latency.

## Implemented paths

- Native synchronization filters the bounded service delta journal to document entities and renderer metadata. The typed receiver applies each batch transactionally, retains Arc<Document> for metadata changes and retries a full reset for missing bases or application errors.
- Position-only patches reuse validated graph structure and check coordinates, IDs and expected revision. Compact GUI dispatch no longer builds an unused full snapshot response.
- Undo/Redo byte sizes are counted once per document entry and cached, maintaining the combined 100-entry / 16 MiB limit.
- Moves append a sequenced/checksummed record and fsync before acknowledgment. Other writes, or the next write after 128 records / 4 MiB, stream a compact checkpoint. Checkpoint file and containing directory are synced before pruning the journal. Startup replays acknowledged moves and Undo/Redo.

At the time of this measurement, full Workspace/history cloning, O(N) delta derivation and typed document copy, and periodic whole-workspace checkpoints remained. Shared document snapshots and buffered checkpoints were subsequently implemented and measured in [HISTORY-PERFORMANCE.md](HISTORY-PERFORMANCE.md). The short run above does not cross checkpoint compaction. GPU geometry/upload work is described separately in [PERFORMANCE.md](PERFORMANCE.md); previous WebView comparisons are in [WEBVIEW-PERFORMANCE.md](WEBVIEW-PERFORMANCE.md).

## Verification

macOS actual-app checks on this change used isolated workspaces:

- 2,000-node moves reached revisions r001–r012 with native presentation and all four WebView acknowledgments. Restart restored r012; Undo restored the prior displaced node position at r013.
- Demo circuit: native selection/move, disconnect and port drag reconnect, rename and Undo, add/select/delete, wheel zoom; floating results ran the 320-vector contract successfully.
- Library dragged right → bottom → left. All three splitter dimensions changed. Library, inspector and results each opened a real floating window and re-docked. Header close and toolbar restore were exercised.
- Unit tests cover atomic invalid coordinate/unknown-ID rejection, revision conflicts, cached history/reload equivalence, native metadata Arc reuse, node/edge removal/readdition and ordering, invalid sync bases/order, WAL replay, truncation of an incomplete tail, checksum corruption preservation, checkpoint replay de-duplication and compaction, and failed log/checkpoint writes without state/cursor publication.
- Unix socket/MCP integration kills the service after an acknowledged move, restarts it and verifies coordinates, revision, cursor reset and Undo. Windows/Linux and cross-monitor DPI transitions remain ⭕️ [Pending].

To reproduce, seed independent roots with `python3 scripts/ui_benchmark.py prepare ROOT --nodes 2000`, launch each release executable with `LAS_WORKSPACE=ROOT LAS_UI_TRACE=/tmp/run.json`, perform the alternating drags, then summarize with `python3 scripts/ui_benchmark.py summarize /tmp/before.json /tmp/after.json --out /tmp/report.json`. UI input must come from the real native window. Keep both `.logic/workspace.json` and `.logic/workspace.wal` when copying a workspace.
