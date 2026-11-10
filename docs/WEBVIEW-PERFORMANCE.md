# Live WebView delta sync measurements

Measured 2026-11-09, Apple M4 / Metal, macOS 27.2, **release Tauri app**, 1440×900 logical main window. Source data and all individual timings: [apple-m4-webview-delta.json](performance/apple-m4-webview-delta.json).

## Flow and measurement boundaries

The live native app ran generated 100/500/2,000-node circuits with 196/996/3,996 edges, respectively. Nodes use a 40-column grid; each ADD receives its two predecessors. The first ADD (`n2`) was already selected. Actual pointer drags alternated between the same two positions; no offscreen rendering or direct model mutation substituted for these measured gestures. The library, inspector, results and controls WebViews were visible and docked. Three splitter WebViews also participated in store synchronization in both modes, but their paint callbacks were not instrumented. Each run began with the same seeded workspace, empty Undo/Redo and no verification/experiment results. The user's workspace was not used for performance fixtures.

There were 12 committed moves per run: two warmups and ten measured samples. After each move, the test waited for the expected inspector revision and captured the updated screen before continuing. A preliminary 500-node full-sync run that issued gestures before updates finished was discarded and rerun with this completion gate. Every retained sample has native and all four WebView acknowledgments. The final sequential run order was 100 delta/full, 500 delta/full, 2,000 full/delta; this is a small development measurement, not a randomized benchmark or latency guarantee.

`LAS_UI_TRACE` records these stages with one Rust Instant clock:

1. **Start:** entry into native pointer handling for the final event producing the move patch. It excludes OS input queue time, the duration of the drag and preview latency before release.
2. **Commit:** return of the successful canonical service response, including patch validation, Undo/history, workspace serialization, atomic replacement and fsync, plus local IPC.
3. **Native:** first draw/present-call return using that committed revision. This includes cache synchronization, projection and submission, but does **not** wait for GPU completion or physical presentation.
4. **WebView:** React commits the synced revision, then acknowledges after two requestAnimationFrame callbacks. The report uses the latest acknowledgment among controls/library/inspector/results. This is a **paint opportunity**, not physical display scan-out. A move does not change the library's displayed names; this acknowledgment verifies the committed snapshot revision rather than a visible coordinate field.

The same client/build was used for both modes. `LAS_WEB_SYNC_FULL=1` returns full changed snapshots; default mode returns entity deltas. Both modes use compact mutation acknowledgments, hierarchy row memoization and the same native wake mechanism, isolating the sync-mode comparison. The native path still obtains a whole document on revision changes. System scheduling, timing instrumentation and background desktop activity can affect these small samples.

## Results

Milliseconds, p50 / p95. With ten measured samples the empirical p95 is the maximum sample.

| Nodes | Web sync | Commit | Native present request | All WebViews paint opportunity |
|---|---|---:|---:|---:|
| 100 | Full | 8.87 / 13.04 | 33.25 / 35.48 | 125.30 / 142.47 |
| 100 | Delta | 9.27 / 13.52 | 33.49 / 34.18 | 59.10 / 59.86 |
| 500 | Full | 15.35 / 17.84 | 477.50 / 522.71 | 532.46 / 558.27 |
| 500 | Delta | 16.76 / 30.17 | 66.57 / 89.02 | 42.51 / 62.93 |
| 2,000 | Full | 63.06 / 91.77 | 1,209.73 / 1,688.31 | 1,441.13 / 1,788.95 |
| 2,000 | Delta | 56.79 / 103.05 | 193.43 / 257.12 | 83.30 / 142.14 |

Changed response body size for **one** WebView, median:

| Nodes | Full | Delta |
|---|---:|---:|
| 100 | 17,119.5 B | 356 B |
| 500 | 87,326.5 B | 356 B |
| 2,000 | 359,140.5 B | 356 B |

These byte counts are compact UTF-8 JSON reserialization lengths recorded in JavaScript, excluding Tauri transport framing. Cursor lengths, floating-point coordinates and appended history cause small per-sample differences. They exclude initial boot/full-resync traffic. All four WebViews receive their own response. No FPS, photon latency or continuous-input guarantee is inferred from these values. Canonical persistence and native whole-document synchronization remain optimization targets; graph shape and experiment-heavy workspaces need broader profiling.

## Synchronization correctness and live QA

- Rust-generated node/edge/history deltas reconstruct exact full snapshots in TypeScript, including 110 moves crossing the history cap, disconnection, diagnostic rename, deletion and reordering.
- JavaScript tests cover structural sharing, multi-delta replay, null clearing, entity order errors, base mismatch and unrelated experiment identity preservation. Server tests cover journal count/byte bounds, gap/restart fallback, compact mutation acknowledgments and failed writes not advancing the cursor.
- Actual Unix-socket MCP mutation is visible through WebView deltas. Restarting the service forces a full snapshot with a new session cursor.
- Native app: add/select/delete/move, input disconnect/pointer reconnect, rename/Undo, zoom, library drag left→right→bottom→left, all three splitters, all three panels floating/redocking and result close/reopen. Verification updated while results were floating and passed 320/320. English/Japanese/Simplified Chinese settings synchronized across views. Temporary edits were undone or removed; the original circuit topology/positions and original panel dimensions were retained.
- ⭕️ [Pending] Physical scan-out measurements, mixed-DPI multi-monitor and Windows/Linux live QA.

## Reproduce

Use Node 22+ with type-stripping support for the sync tests:

```sh
LAS_WEB_DELTA_FIXTURE=/tmp/las-web-deltas.json cargo test -p studio-service delta::tests
LAS_WEB_DELTA_FIXTURE=/tmp/las-web-deltas.json npm run test:sync
npm run bundle -- --bundles app
python3 scripts/ui_benchmark.py prepare /tmp/las-ui-new-delta --nodes 2000
LAS_WORKSPACE=/tmp/las-ui-new-delta LAS_UI_TRACE=/tmp/las-ui-delta.json \
  target/release/bundle/macos/'Logic Architecture Studio.app'/Contents/MacOS/logic-architecture-studio
```

With default dimensions, drag the first ADD's header approximately from (760,90) to (780,105), then back, twelve times. Wait for each committed inspector revision before the next drag. Quit normally after all views update. For full comparison, prepare a **new** isolated directory with the same node count and launch with `LAS_WEB_SYNC_FULL=1`. Do not run two copies simultaneously. The seed script refuses to overwrite an existing workspace. Summarize the completed logs with:

```sh
python3 scripts/ui_benchmark.py summarize /tmp/las-ui-full.json /tmp/las-ui-delta.json \
  --out /tmp/las-ui-summary.json
```

The trace facility is opt-in and has no GUI controls. Leave its environment variables unset for normal work.
