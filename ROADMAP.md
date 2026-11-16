# Roadmap

- 🟢 [Done] implemented in the current codebase
- 🟠 [Next] high-priority unfinished work
- 🔴 [Later] planned, but not the closest next step
- ⭕️ [Pending] cannot be tested in the dev environment; on hold

## Appearance preferences

- 🟢 [Done] Dedicated singleton Preferences window, opened by the toolbar gear, with left-side Appearance tab and right-side settings.
- 🟢 [Done] English/Japanese/Simplified Chinese, ten node-kind colors, color picker/hex input, validation, Apply/Discard and individual/all-color reset.
- 🟢 [Done] Rust-owned per-workspace persistence and WebView/native metadata delta sync; custom colors reach nodes, ports, source wires, overview symbols, library and inspector without circuit/history changes.
- 🟢 [Done] macOS release QA: separate window open/close/reopen, all three languages, hex edit/apply, invalid-color blocking, discard/reset, scrollable settings with fixed footer, native canvas color changes and restoration after app restart. Automated persistence, atomicity, delta-sharing and retained-cache tests pass.

## Dark / light appearance

- 🟢 [Done] Appearance → Color mode offers Dark/Light in English, Japanese and Simplified Chinese. Apply/Discard uses the shared Rust preference; old workspaces default to Dark without changing design revision/history.
- 🟢 [Done] Metadata-only sync updates WebView panels, detached windows, native window chrome and wgpu canvas. Default role colors adapt to the background; custom color values are preserved. Results, waveforms and annotations use theme tokens.
- 🟢 [Done] macOS release QA (2026-11-16): Dark → Light → Dark, Preferences and native canvas appearance, floating/redocking Results, Light restoration after restart. Preference validation, legacy loading, persistence, native/WebView delta tests and workspace checks pass.
- ⭕️ [Pending] Windows/Linux native window chrome and rendering QA require those environments.

## Gate symbol display

- 🟢 [Done] Appearance can hide gate frames in either theme. Shared Rust preference persists and syncs without circuit/history changes; unfilled gates retain port anchors, labels, internal leads and corner-only selection marks. Normal/overview macOS QA and renderer cache/restoration tests pass.
- 🟢 [Done] Light-mode smallest LOD retains input/output leads, scaled to the overview gate and inversion bubble, with at least 1.5 screen pixels of stroke width. Seven gate types tested across 0.02–0.349 zoom; native minimum-LOD wiring visually verified.

- 🟢 [Done] Light-mode gate overview removes both tile fill and white knockouts, including selected nodes. Native wgpu and WebView fallback use colored outlines; macOS zoom QA and renderer regression tests pass.

- 🟢 [Done] Toolbar toggle and Appearance → Node display select between blocks and distinctive AND/OR/XOR/NOT/NAND/NOR/XNOR gate shapes. Rust-owned preference persists per workspace and propagates through metadata-only sync.
- 🟢 [Done] Native wgpu outlines, inversion bubbles and XOR curves retain existing node bounds and port anchors. Smallest LOD uses white gate outlines on role-colored tiles in Dark mode; Light mode uses unfilled role-colored outlines with the canvas visible through them. The WebView fallback supports the same modes.
- 🟢 [Done] NAND/NOR/XNOR typed nodes, palette entries, custom colors, bitwise simulation and Verilog output. Independent truth tables cover signed/unsigned 1/8/64-bit values; Icarus and Verilator each pass 112 expected output checks with Rust comparison.
- 🟢 [Done] macOS release QA (2026-11-16): mode toggle, seven distinct gate shapes, selection, smallest LOD, Appearance setting and preference restoration after restart. XNOR/NAND block labels fit their badges at detailed/intermediate/overview zoom levels.

## Core / Phase 1

- 🟢 [Done] Rust-owned typed combinational IR, atomic patch validation and revision conflicts.
- 🟢 [Done] Single service shared by GUI/MCP; private Unix socket and exclusive owner lock.
- 🟢 [Done] Snapshot Undo/Redo, atomic JSON replacement, rollback on storage failure.
- 🟢 [Done] MCP stdio legacy lifecycle, shared tool schemas, IR resource, research prompt, read-only default.
- 🟠 [Next] rmcp migration, modern MCP lifecycle, structured error codes and incremental event summaries.
- 🟢 [Done] Versioned design-file export/import with graph validation, revision checks and Undo.
- 🟢 [Done] Combined Undo/Redo limits: 100 snapshots and 16 MiB serialized history.
- 🟢 [Done] Startup recovery window for damaged workspace/checkpoint/journal/layout files; preview validated current/previous checkpoints, preserve originals before explicit restoration, scoped layout reset and exact-workspace restart.
- 🟢 [Done] Retain the previous durable checkpoint using a hard link where supported (synced-copy fallback); owner-lock/content-token recovery guards and macOS recovery/editing QA in docs/RECOVERY.md.
- 🟢 [Done] macOS native workspace-folder picker from editor/recovery/startup, remembered selection, cancel/same-folder no-op and missing-folder protection; editing/restart QA in docs/WORKSPACES.md.
- 🟢 [Done] Native checkpoint-backup selection, validation, original-file archiving and restoration with persisted Undo/Redo; verified in the macOS app.
- 🟢 [Done] Explicit valid-prefix move-log salvage: shared strict decoder stops at the first damaged frame, previews recovered moves/revision and excluded bytes, preserves originals and restores Undo/Redo; macOS restart/editing/panel QA in docs/RECOVERY.md.
- 🔴 [Later] SQLite Design Store, semantic diff, remote authenticated transport.

## UI / Rendering

- 🟢 [Done] Smallest zoom level (<0.35) uses full role-colored tiles and large centered bold white symbols; detailed/intermediate layouts retained. All ten node kinds verified with Metal rendering and LOD boundary tests; browser zoom-out/zoom-in transitions visually verified.

- 🟢 [Done] Dot-aligned drag snapping: shared native grid spacing (24 units, 120 in overview), absolute grabbed-node anchor, group offsets preserved, identical preview/commit and zero-change history suppression; SVG grid uses the same origin and spacing. Native group drag/Undo and browser normal/overview snapping verified on screen.

- 🟢 [Done] 24-unit port labels (2× native size), square 288-unit nodes with 64-unit port rows; native Metal label/width/shape QA.
- 🟢 [Done] Rust-owned multiple selection (Shift/⌘/Ctrl-click and marquee, up to 512 nodes), atomic group drag/Undo, shared selection deltas and persistence; macOS selection/move/Undo/floating/redocking and browser box-select/group-move/Undo QA.

- 🟢 [Done] Editable node graph, ports, connections, inspector, search, pan/zoom, auto layout.
- 🟢 [Done] English / 日本語 / 简体中文 UI and compact icon controls.
- 🟢 [Done] Docking menus and drag targets; native floating-panel implementation.
- 🟢 [Done] Vendored UNGE rendering subset with license notices and no external path dependency.
- 🟢 [Done] Role-colored headers, symbols, port rings, Bezier cables, selected halo, connection preview and basic zoom detail reduction.
- 🟢 [Done] Overview readability: minimum screen-space title/formula sizes, high-contrast formulas and bit widths in node titles; macOS zoom-out/zoom-in QA.
- 🟢 [Done] Zoom-stable, uniform wires: external nets, internal leads, gate outlines and connection previews share a 3-world-unit stroke with a 1.5-framebuffer-pixel minimum. Single-bit and bus wires use the same thickness; bus-width labels remain available. Browser fallback mirrors the stroke/port rules. Regression coverage checks both themes, multiple bus widths and zoom levels; native light-mode rendering was visually verified at all three detail levels.
- 🟢 [Done] Separate feedback lanes per destination input: upper ports use outer bottom/left approaches to separate return wires. Native drag previews, committed geometry and browser fallback use the same port-order rule without scanning the graph. Regression tests cover multi-input separation, movement and retained/full rebuild equivalence; the reported AND/XOR→NAND overlap was visually verified as resolved in the native app.
- 🟢 [Done] macOS release zoom-in/out across all three LODs; GPU pixel checks at 2–100% zoom, including 0.3/0.35/0.6 boundaries and four subpixel offsets for 1-bit/bus wires (72 render cases); retained geometry and GPU regressions pass.
- 🟢 [Done] Rust UNGE scene adapter, GPU labels, native Surface rendering path; no frame IPC.
- 🟢 [Done] Real Metal GPU offscreen rendering tests for English, Japanese and Simplified Chinese; glyph and pixel checks passed.
- 🟢 [Done] Browser interaction checks and 18 language/viewport combinations (320–1536 px).
- 🟢 [Done] Real MCP/GUI IPC revision conflict and restart/persistence integration test.
- 🟢 [Done] Main native GPU graph with child WebView controls; Rust-owned UNGE selection, move/connect gestures and camera.
- 🟢 [Done] macOS live checks: node drag (revision increment), selection/inspector sync, input disconnect/reconnect, inspector floating and redocking.
- 🟢 [Done] Cursor-based conditional GUI sync; unchanged responses omit snapshots, native metadata excludes history/results and omits unchanged documents. Session cursors invalidate caches after service restart.
- 🟢 [Done] Native snapshot reads/decoding off the UI thread, cached gesture projections and hit-test indexes, unchanged-frame GPU/layout skip and surface resize deduplication.
- 🟢 [Done] macOS live QA: move/select/zoom/Undo, splitter resize, floating/redocking and verification update; MCP conditional-sync/restart integration test.
- 🟢 [Done] Stable node/edge render IDs, retained projection and BVH delta updates for moves, metadata, ports, edges and topology; bounded full-rebuild fallback.
- 🟢 [Done] Node/edge geometry reuse across frames and sparse Quad/glyph GPU instance uploads; full-rebuild CPU scene and Metal pixel equivalence checks.
- 🟢 [Done] Apple M4 release profiling at 100/500/2,000 nodes (up to 3,996 edges), overview/detail, 30 measured frames after warmup; results in docs/PERFORMANCE.md.
- 🟢 [Done] macOS live regression QA after delta integration: node add/delete/select/move/rename, disconnect/reconnect, Undo, zoom, drag docking left/right/bottom, all three panels floating/redocking, hide/reopen, splitters and restart.
- 🟢 [Done] Bounded WebView delta journal (64 entries / 8 MiB serialized deltas), node/edge and history/experiment entity changes, changed root fields, cursor validation and full resync on gaps/restarts.
- 🟢 [Done] Atomic immutable WebView delta application with structural sharing, memoized hierarchy rows and inspector draft preservation across position/revision changes; compact view-mutation acknowledgments.
- 🟢 [Done] Wake native sync after GUI commits; release live-app profiling at 100/500/2,000 nodes with actual pointer drags, canonical persistence, native present requests and all four WebView paint-opportunity acknowledgments. Full/delta comparison in docs/WEBVIEW-PERFORMANCE.md.
- 🟢 [Done] WebView-delta live QA: node add/delete/select/move/rename/Undo/connect/zoom; all panel floating/redocking, floating result updates, drag docking, splitters, hide/reopen and en/ja/zh settings; MCP/restart delta integration and Rust→TypeScript snapshot equivalence.
- 🟢 [Done] Native entity delta synchronization with typed transactional replay, shared metadata-only documents and full recovery on missing bases/restarts.
- 🟢 [Done] Position-only commit path, compact GUI acknowledgments, cached Undo/Redo byte budgets and fsynced move journal with bounded checkpoints and restart replay.
- 🟢 [Done] 2,000-node release before/after pointer measurements and macOS editing/docking/floating regression checks; docs/NATIVE-PERFORMANCE.md.
- 🟢 [Done] Shared Arc document/history/candidate snapshots and buffered durable checkpoint output, preserving JSON compatibility and transactional rollback.
- 🟢 [Done] 2,000-node × 140 real sequential moves per build, including a 16.86 MB checkpoint; editing, floating, re-docking and restart/Undo QA. See docs/HISTORY-PERFORMANCE.md.
- 🟢 [Done] Copy-on-write entity blocks, indexed position updates and cached block JSON sizes; skip shared blocks in service/native/retained-renderer comparisons.
- 🟢 [Done] Suppress stale canonical presentation after accepted native gestures; 2,000-node × 120 matching sequential moves and editor/panel/restart QA in docs/SPARSE-MOVE-PERFORMANCE.md.
- 🟠 [Next] Profile remaining UNGE transactions, visible-scene assembly and scheduling; wider fanout, simultaneous multi-block moves and saturated-input measurements.
- 🟢 [Done] Native left/right/bottom panel strips, same-WebView floating/reparenting, hide/reopen toolbar and OS-close handling.
- 🟢 [Done] macOS live QA for library, inspector and results: all dock destinations, floating/redocking, panel close/reopen, floating OS-close/reopen.
- ⭕️ [Pending] Physical multi-monitor/DPI changes and minimize/restore QA.
- ⭕️ [Pending] Windows named-pipe transport and Windows/Linux native packaging/interaction QA.
- 🟢 [Done] Rust orthogonal obstacle-aware routing with 12-unit clearance, reserved port approaches, separate net corridors and bridge arcs for unavoidable crossings. All moved nodes (including unrelated obstacles) participate in preview/commit rerouting; unchanged wire geometry remains cached. Native and WebView use the same router, with revision-guarded/coalesced web preview requests. Covered ports/search exhaustion show `!` instead of drawing through nodes.
- 🟢 [Done] Native and Web live verification of obstacle movement through the design API and Undo restoration; no browser console errors. Regression coverage includes non-incident obstacle moves, preview/commit/full-rebuild equivalence, multi-net returns and nonplanar crossings. Release smoke benchmark: 2,000 nodes / 1,950 wires in 10.5 ms (simple grid; routing CPU only).
- ⭕️ [Pending] Live drag automation did not deliver movement in this environment; drag preview/commit is covered by Rust tests, while actual screen rerouting was verified through revision-checked edits and Undo.
- 🔴 [Later] Incremental route search for dense netlists, larger search budgets, group navigation and minimap.

## Verification / Phase 2

- 🟢 [Done] Verilog generation from the same validated graph.
- 🟢 [Done] Independent 320-vector ALU contract, custom vectors, exact 64-bit evaluation.
- 🟢 [Done] Revision-stamped results and stale-result display.
- 🟢 [Done] Yosys detection and fixed-script synthesis adapter, run artifacts and timeout.
- 🟢 [Done] Verified real Yosys synthesis from the macOS GUI and checked the generated JSON netlist.
- 🟢 [Done] Icarus generated-RTL sampled comparison against Rust and independent contracts, revision guards, bounded execution/artifacts, GUI/MCP entry point and 1/8/32/64-bit real-tool tests; see docs/RTL-VERIFICATION.md.
- 🟢 [Done] Verilator adapter with explicit GUI/MCP engine selection, shared validation/result protocol and 1/8/32/64-bit differential regression against Icarus; real tool and macOS UI checks in docs/RTL-VERIFICATION.md.
- 🟢 [Done] Seeded differential regression with four outputs per circuit and 3/7/17/31/33/63-bit signals, verified in Icarus and Verilator.
- 🟢 [Done] Yosys SAT proof of the combinational ALU contract over all binary inputs, with revision checks and reproducible counterexamples.
- 🟢 [Done] Register nodes, feedback across register boundaries, shared rising-edge clock and synchronous reset, Verilog export, and cycle traces in GUI and MCP.
- 🟢 [Done] Rust-owned ephemeral simulation traces, revision/trace-guarded sample selection, 10/16-base display and full-node back-annotation (post-edge Q and settled D); sparse retained wire geometry is independent of cursor changes.
- 🟢 [Done] Sampled waveform view with a shared cursor, horizontal zoom, signal paging and VCD export of all node values; macOS native save dialog and floating/redocking QA.
- 🟢 [Done] Reject out-of-range unsigned inputs instead of silently truncating; expose two-state unsigned arithmetic/wrap and synchronous-reset semantics in the inspector. Native bus width rejection and clocked feedback reconnection have regression tests.
- 🟢 [Done] Bus strokes/slashes and width/value labels, prominent stale-result banner and rerun, sequential human-readable node names.
- 🟢 [Done] Constant, slice and two-input concat IR nodes; exact unsigned widths/values, typed configuration, native/WebView ports and inspector, simulation and synthesizable Verilog. Includes 1–64-bit boundary tests, Icarus/Verilator differential checks and macOS editing/restart QA.
- 🟢 [Done] Explicit zero/sign extension nodes with 1 ≤ input ≤ output ≤ 64 width checks, two’s-complement MSB replication, typed canvas annotations and exact Verilog replication. All 2,080 width pairs, 64-bit boundaries and real Icarus/Verilator checks pass.
- 🟢 [Done] Circuit-wide signed types, atomic connected-data-path propagation, strict width/signedness checks, negative decimal inputs/constants/expectations and CSV, signed simulation/annotations/waveforms and Verilog declarations. Legacy circuits remain unsigned; explicit extension boundaries and unsigned MUX selectors are preserved.
- 🟢 [Done] Durable per-cycle stimulus tables (1–256 rows), reset, UTF-8 CSV import/export and exact typed expected-output checks; revision/version guards, shared failure-to-cursor selection and existing canvas/waveform/VCD integration. Rust owns saved tables; edits do not change circuit revisions or Undo history.
- 🟢 [Done] Run saved stimulus tables with Icarus Verilog or Verilator: signed inputs/expectations, registers, synchronous reset, all-node Rust comparison, separate expectation checks, shared RTL trace/annotations and original simulator VCD export. Real engine tests and macOS execution/save QA pass.
- 🟠 [Next] Expression assertions and richer failure navigation for saved stimulus tables.
- 🟢 [Done] Reusable module definitions and instances: group selected processing nodes, infer typed boundary ports, preserve top-level IO, place independent copies, nest existing modules, and expand one level back into editable nodes. Rust owns definitions, validation, persistent history and native/WebView deltas; both canvases support named multi-output ports.
- 🟢 [Done] Library create/place controls, modifier selection in the hierarchy list and recursive inspector tree in English/Japanese/Simplified Chinese. macOS/WebView live QA: create → reuse → three-level nesting → expand → Undo → restart restoration; two distinct module outputs connected to a primitive gate and reflected in the native renderer.
- 🟢 [Done] Flattened execution/Verilog with stable independent instance signals; combinational equivalence, signed sequential/reset expectations, independent register state, self-feedback expansion, malformed/recursive definition rejection, persistence and delta replay tests. Nested saved stimuli pass real Icarus and Verilator comparison with Rust and VCD generation. See docs/MODULES.md.
- 🔴 [Later] In-place shared module-definition editing/drilldown, parameter overrides, hierarchy-preserving HDL output and per-port module back-annotation.
- 🔴 [Later] Full X/Z semantics with external simulator alignment; HDL import, FST and signal grouping.
- ⭕️ [Pending] External GTKWave/Surfer opening and platform-specific CSV/VCD save QA beyond macOS; viewers are not installed in this environment.
- 🟢 [Done] Native schematic symbols, zoom-dependent port ranges, missing-input indicators, orthogonal feedback routes and revision-checked register values on the canvas.
- 🔴 [Later] PDK-specific mapping, STA, cell area/power and physical evaluation.

## AI / Experiments / Phases 3–5

- 🟢 [Done] Isolated revision-bound candidate documents, common vectors, comparison and guarded GUI adoption.
- 🟢 [Done] Regression-detection example using baseline, ADD→SUB, XOR→OR candidates.
- 🟠 [Next] Job IDs, cancellation, process/resource limits and OS sandboxing.
- 🟠 [Next] Internal model adapter, MCP client and budgeted autonomous exploration loop.
- 🔴 [Later] Planned GGUF model support: [gemma-4-31B-it-ARA-heretic-GGUF](https://huggingface.co/mradermacher/gemma-4-31B-it-ARA-heretic-GGUF); runtime integration/inference unimplemented. See [model support plan](docs/MODELS.md).
- 🔴 [Later] Ripple/Carry-select/Brent–Kung/Kogge–Stone generators and measured Pareto optimization.
- 🔴 [Later] OpenROAD/KLayout flow, FPGA deployment and multiple-agent research roles.

No unmeasured PPA metrics, formal guarantees, or 60 FPS targets are marked complete.

## Panel gestures

- 🟢 [Done] Native panel-header pointer capture, Rust-owned drag target detection and GPU drop preview for left/right/bottom docking.
- 🟢 [Done] Native splitter WebViews for independent left/right width and bottom height adjustments; Rust clamps geometry.
- 🟢 [Done] macOS live validation: library left→right→bottom→left, inspector and results drag/redock, all three splitter directions.
- 🟢 [Done] Persist panel dock/hidden/floating modes, restore destinations, splitter dimensions and floating window geometry in Rust; atomic layout-file replacement.
- 🟢 [Done] macOS restart QA: changed left/right widths and bottom height, library bottom/left placement, hidden results, floating inspector geometry, redock and reopen after restart.
- 🟢 [Done] Rust-owned keyboard splitter adjustment (10/50 pt, Home/End), Cmd/Ctrl+Alt+arrow focus, localized accessibility dimensions, targeted state updates and persisted sizes; macOS keyboard/drag/restart/floating QA in docs/PANEL-KEYBOARD.md.
- ⭕️ [Pending] Multi-monitor mixed-DPI drag and cross-window floating-to-dock drag QA.
