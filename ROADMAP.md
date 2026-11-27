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
- 🟢 [Done] Shared graph traversal for layout and RTL, indexed port drivers, and one evaluation plan per simulation or vector suite. Native canvas fitting uses module port counts. Out-of-range layouts leave the revision and Undo history intact.
- 🟢 [Done] Single service shared by GUI/MCP; private Unix socket and exclusive owner lock.
- 🟢 [Done] Snapshot Undo/Redo, atomic JSON replacement, rollback on storage failure.
- 🟢 [Done] MCP stdio, shared tool schemas, IR resource, research prompt and read-only default.
- 🟢 [Done] Read-only MCP revision summaries since a client revision, with an explicit resync signal for missing or future revisions. The service reads the retained log directly without cloning the workspace or writing a checkpoint.
- 🟢 [Done] rmcp stdio with 2026-07-28 discovery and 2025-11-25 legacy initialization. Structured tool errors and protocol error data. Live stdio integration verifies both lifecycles, existing tools, IR resource, research prompt and revision summaries.
- 🟢 [Done] Versioned design-file export/import with graph validation, revision checks and Undo.
- 🟢 [Done] Combined Undo/Redo limits: 100 snapshots and 16 MiB serialized history.
- 🟢 [Done] Startup recovery window for damaged workspace/checkpoint/journal/layout files; preview validated current/previous checkpoints, preserve originals before explicit restoration, scoped layout reset and exact-workspace restart.
- 🟢 [Done] Retain the previous durable checkpoint using a hard link where supported (synced-copy fallback); owner-lock/content-token recovery guards and macOS recovery/editing QA in docs/RECOVERY.md.
- 🟢 [Done] macOS native workspace-folder picker from editor/recovery/startup, remembered selection, cancel/same-folder no-op and missing-folder protection; editing/restart QA in docs/WORKSPACES.md.
- 🟢 [Done] Native checkpoint-backup selection, validation, original-file archiving and restoration with persisted Undo/Redo; verified in the macOS app.
- 🟢 [Done] Explicit valid-prefix move-log salvage: shared strict decoder stops at the first damaged frame, previews recovered moves/revision and excluded bytes, preserves originals and restores Undo/Redo; macOS restart/editing/panel QA in docs/RECOVERY.md.
- 🟢 [Done] MCP foundation Phase 1. Transport-neutral AppAdapter and ToolRegistry, shared schema and access validation, structured errors, bounded document state, and a standalone in-memory SampleAdapter. Verified sample CRUD without Design Core and paged state over the real workspace bridge. See docs/MCP.md.
- 🟢 [Done] MCP foundation Phase 2. rmcp Streamable HTTP on opt-in IPv4 loopback, with local Host and Origin enforcement and read-only workspace access by default. Inspector 2.15.0 verified connection, 45 tool definitions and simulation over both HTTP lifecycles, and tool discovery and revision summaries over stdio. See docs/MCP.md.
- 🟢 [Done] MCP foundation Phase 3. Loopback bearer authentication, credential-bound legacy sessions, tool scopes, separate one-time write approval, per-client rate limits and redacted audit logs. Inspector 2.15.0 verified permitted and rejected calls, approved edits and stdio compatibility. See docs/MCP.md.
- 🟢 [Done] MCP foundation Phase 4. Tauri Preferences manages loopback start/stop, publication of the current workspace document, client scopes, and in-app write approvals. Verified macOS approval and denial with Inspector 2.15.0, including the approved rename and move appearing on the GPU canvas without reopening the workspace. See docs/MCP.md.
- 🟢 [Done] MCP foundation Phase 5. Revision-checked circuit preview, macOS review of changed nodes and wires, approved edits, and revision-tagged verification reports. Verified with Inspector 2.15.0 and the native macOS GPU canvas, including GUI Undo, restored nodes and wires, and stale verification status. See docs/MCP.md.
- 🟢 [Done] MCP foundation Phase 6 (initial slice). SSH-forwarded Streamable HTTP with existing Bearer scopes and desktop approval, and revision-change notifications through MCP resource subscriptions. Inspector 2.15.0 verified the SSH path, scope denial, app approval, and delivery of the initial sync, approved edit, and GUI Undo notifications on one Mac.
- ⭕️ [Pending] Phase 6 cross-device verification. Repeat the SSH, scope, approval, and notification workflow from Inspector on a separate physical device once an authorized test endpoint is available.
- 🔴 [Later] Phase 6 direct remote deployment with OAuth, long-running MCP tasks, MCP Apps, and multi-app management.
- 🟢 [Done] Circuit review of the immediately preceding revision in Results → History and through MCP. Shows changed nodes, wires and shared module definitions, including reversed changes after Undo and forward changes after Redo.
- 🟢 [Done] Compare any two retained revisions in Results → History and via MCP. Durable snapshots keep circuit and shared-module changes available after reopening, including Undo branches and replayed moves.
- 🔴 [Later] SQLite Design Store and longer-term revision archives.

## UI / Rendering

- 🟠 [Next] Reconcile the three previously failing standalone UNGE renderer checks for localized port labels, repeated moves and high fan-out updates with the current LOD and index behavior. Keep fresh-rebuild equivalence covered.

- 🟢 [Done] Smallest zoom level (<0.35) uses full role-colored tiles and large centered bold white symbols; detailed/intermediate layouts retained. All ten node kinds verified with Metal rendering and LOD boundary tests; browser zoom-out/zoom-in transitions visually verified.

- 🟢 [Done] Free positioning by default, with Option/Alt at drag start for dot-aligned snapping: shared native grid spacing (24 units, 120 in overview), absolute grabbed-node anchor, group offsets preserved, identical preview/commit and zero-change history suppression; SVG grid uses the same origin and spacing. Native group drag/Undo and browser normal/overview snapping verified on screen.

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
- 🟢 [Done] Release Metal benchmark for 2,000 nodes, 1,998-way fanout and simultaneous moves across four entity blocks. Separately measures retained sync and scene assembly, with full-rebuild equivalence on every frame. See docs/PERFORMANCE.md.
- 🟢 [Done] Reuse retained edge topology for position-only updates, deduplicate shared routing reservations and port warnings, and batch native drag redraws at the event-loop boundary. See docs/PERFORMANCE.md for the A/B results.
- 🟢 [Done] Profile wide-fanout overview assembly by stage and reduce wire-cache, warning deduplication and drag-preview routing work. Run matched 250 Hz group-drag replay through the renderer on Apple M4 Metal. See docs/PERFORMANCE.md.
- 🟢 [Done] Profile slow overview frames with wall and macOS thread CPU time. Keep missing-input checks off the retained-node cache hit path and invalidate the affected destination on topology changes.
- 🟢 [Done] Compare matched 250 Hz in-app group-drag replay through actual macOS window presentation using Metal drawable timestamps. See docs/PERFORMANCE.md for the measured latency and frame counts.
- 🟢 [Done] Trace per-preview input wait, CPU assembly, Metal GPU execution interval and display wait. Compare overview wire-warning batching and diagnose drawable backpressure in the shallow-queue trial.
- 🟢 [Done] Correlate CoreVideo refresh timing with Metal drawable presentation. Compare opt-in single-in-flight preview pacing at 250 Hz on macOS and document its latency and throughput tradeoff.
- 🟢 [Done] Reuse shaped text and centered glyph ink bounds for adjacent repeated labels in the native canvas. Benchmark the normal preview path at 250 Hz with the 2,000-node fixture.
- 🟢 [Done] Group sustained-drag preview timings by observed earlier presentations after GPU completion. Matched macOS traces at 250 Hz show two drawables reduced preview input-to-display p95 from 66.87 to 51.44 ms, with drawable acquisition p95 close in both configurations. See docs/PERFORMANCE.md.
- 🟢 [Done] Add opt-in drag redraw spacing shared by the native scheduler and its replay. Matched 2 Drawable macOS runs at 250 Hz measured lower p95 with 20 ms spacing (51.68 → 46.14 ms) and fewer visible previews (111 → 74). The default remains unpaced.
- 🟢 [Done] Add an experimental macOS preview path that assembles the scene while the window thread acquires a drawable. Six matched 2 Drawable pairs at 250 Hz measured 51.60 → 50.42 ms median run p95 and 176 visible previews in each arm. See docs/PERFORMANCE.md.
- 🟢 [Done] Add paired native drag analysis for the two-drawable baseline. Report paired p95 deltas, per-pair wins and regressions, and which pairs improved without losing visible previews. Reject mismatched replay conditions, display, drawable depth, or input cadence.
- 🟢 [Done] Traverse retained edge metadata and drag-preview routes together in UUID order. Reuse one wire-cache borrow for the scene pass and retain viewport filtering and wire paint order.
- 🟠 [Next] Obtain a repeatable sustained-drag tail reduction before enabling a new scheduler by default. Measure acquisition and GPU-to-display wait alongside actual preview throughput.
- 🟢 [Done] Return the next native delta in a durable position-only commit acknowledgement when its base cursor matches. Apply it before drawing, retain polling for stale bases, and verify the saved-to-visible interval on macOS.
- 🟢 [Done] Trace committed frames from save acknowledgement through delta application, draw start, GPU execution and Metal presentation. Submit the native frame before notifying WebViews for inline commits.
- 🟢 [Done] Correlate committed-frame waits with preceding Metal drawables and display refresh targets. Use a two-drawable macOS workspace queue, verified at 250 Hz for a 2,000-node group drag.
- 🟢 [Done] Reuse applied macOS panel layout across GPU redraws when window dimensions, scale, splitter sizes and panel positions agree. Verify Inspector floating, redocking and splitter resizing in the native window.
- 🟢 [Done] Reuse the last drag preview's routes when placement is unchanged. Refilter for the current viewport and invalidate the cache on committed geometry changes.
- 🟢 [Done] Reuse preview routes for a position-only commit when the full changed placement matches. Verify the 2,000-node commit produces a scene identical to a fresh rebuild.
- 🟢 [Done] Compare native pointer-up to durable-save confirmation and canonical Metal presentation with and without preview-route reuse. Verified matching persisted four-node moves on macOS. End-to-end p95 did not improve in the measured runs.
- 🟢 [Done] Build routing obstacles and endpoint metadata in one node pass, with one lookup per wire endpoint. Record median and tail timings for the 2,000-node Metal replay.
- 🟢 [Done] Batch comparisons of unchanged GPU instance data and stop scanning once fragmented writes require a full upload. Verify exact ranges and Metal delta/full rendering equivalence.
- 🟢 [Done] Short-circuit obstacle clearance queries without allocating candidate lists, and assemble routed edges in ID order without repeated map removals. Verified native 2,000-node drag preview and renderer regressions.
- 🟢 [Done] Short-circuit wire clearance checks during the no-crossing routing pass. Verify identical routes on a 2,000-node grid and preserve single-count crossing penalties in the fallback pass.
- 🟢 [Done] Reuse terminal clearance between direct routing and detour search, and compact the detour edge-cache key. Verify before/after route equivalence on grid and nonplanar fixtures.
- 🟢 [Done] Reuse detour search allocations within a routing pass. Verify identical routes and measure the improvement on nonplanar and 2,000-node grid fixtures.
- 🟢 [Done] Replace detour edge hash lookups with bounded grid-indexed memoization. Verify exact routes on a nonplanar fixture and retained scene equivalence on the 2,000-node Metal stress case.
- 🟢 [Done] Reuse each detour grid between crossing-free and crossing-enabled searches on the same wire. Verify unchanged route geometry and benchmark the nonplanar fixture.
- ⭕️ [Pending] Measure physical pointing-device and AppKit/Tao input dispatch through presentation.
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
- 🟢 [Done] Saved stimulus expectations support typed comparisons against literals, current inputs/outputs and previous-cycle samples. Rust, Icarus and Verilator share the evaluator; CSV round trips and failures show the resolved operand. Verified with signed nested pipeline stimuli in both external simulators.
- 🟢 [Done] Assertion failures can be filtered by signal and cycle. Selecting a failure opens the matching waveform page, highlights the signal and scrolls to the shared cycle cursor. Verified in Chrome with the Rust service, including a signal beyond the first waveform page. All assertion failures are searchable, with 25 results per list page. Live QA covered 128 failures, combined filtering and a jump to the final failure.
- 🟢 [Done] Reusable module definitions and instances: group selected processing nodes, infer typed boundary ports, preserve top-level IO, place independent copies, nest existing modules, and expand one level back into editable nodes. Rust owns definitions, validation, persistent history and native/WebView deltas; both canvases support named multi-output ports.
- 🟢 [Done] Library create/place controls, modifier selection in the hierarchy list and recursive inspector tree in English/Japanese/Simplified Chinese. macOS/WebView live QA: create → reuse → three-level nesting → expand → Undo → restart restoration; two distinct module outputs connected to a primitive gate and reflected in the native renderer.
- 🟢 [Done] Flattened execution with stable independent instance signals; combinational equivalence, signed sequential/reset expectations, independent register state, self-feedback expansion, malformed/recursive definition rejection, persistence and delta replay tests. Nested saved stimuli pass real Icarus and Verilator comparison with Rust and VCD generation. See docs/MODULES.md.
- 🟢 [Done] Verilog export preserves reachable module definitions, named instances, typed multi-output connections and clock/reset across hierarchy. Verified the exported signed pipeline in Icarus and the hierarchy in the Yosys synthesis netlist.
- 🟢 [Done] Double-click module navigation with breadcrumbs and shared-definition editing. Internal edits preserve the external interface and support revision checks and Undo/Redo. Chrome and macOS live QA verified nested navigation and shared edits across two instances. macOS live QA also verified opening by double-click directly on the GPU canvas, returning from both the module and its nested child via the top-level breadcrumb, and opening the same GPU node again.
- 🔴 [Later] Module interface migration, parameter overrides and per-port module back-annotation.
- 🔴 [Later] Full X/Z semantics with external simulator alignment; HDL import, FST and signal grouping.
- ⭕️ [Pending] External GTKWave/Surfer opening and platform-specific CSV/VCD save QA beyond macOS; viewers are not installed in this environment.
- 🟢 [Done] Native schematic symbols, zoom-dependent port ranges, missing-input indicators, orthogonal feedback routes and revision-checked register values on the canvas.
- 🔴 [Later] PDK-specific mapping, STA, cell area/power and physical evaluation.

## AI / Experiments / Phases 3–5

- 🟢 [Done] Isolated revision-bound candidate documents, common vectors, comparison and guarded GUI adoption.
- 🟢 [Done] Regression-detection example using baseline, ADD→SUB, XOR→OR candidates.
- 🟢 [Done] External verification and synthesis job IDs, status polling and cancellation via UI and MCP. Cancelling stops the tool process group before another job can run. Verified cancellation during real Verilator compilation and Yosys synthesis, then successful reruns.
- 🟢 [Done] Unix external-tool resource limits inherited by child processes: 64 MiB per output file and core dumps disabled. Existing log, VCD and wall-clock limits also apply.
- 🟢 [Done] Verification and synthesis monitor each run directory across tool stages, with a 256 MiB aggregate file-size budget and an 8,192-entry limit. Checks run during execution and before accepting the tool result.
- 🟢 [Done] macOS EDA tools and their child processes run under Seatbelt with network access denied and writes limited to the run directory and /dev/null. Compiler temporary files and Clang module cache stay in the run directory.
- 🟢 [Done] macOS EDA stages use a 256 MiB capacity-limited filesystem for artifacts and temporary files. The runner stops the process group as the volume fills and carries successful artifacts into subsequent stages. Verified descendant shutdown, then Icarus verification and Yosys synthesis.
- 🟢 [Done] macOS monitors each EDA process group for more than 64 processes or 4 GiB of summed resident memory and stops the group on a sampled overrun.
- 🟢 [Done] EDA tools inherit a 120 CPU-second per-process limit on Unix. On macOS the runner also stops the process group when sampled CPU time exceeds 90 seconds in a stage, including work already attributed to reaped children.
- 🟢 [Done] Linux external EDA stages support delegated cgroup v2 through `LAS_EDA_CGROUP`. The kernel enforces two CPU cores of bandwidth and a 4 GiB memory limit with group OOM killing. The runner stops the cgroup at a cumulative 45 CPU-second stage budget.
- 🟠 [Next] Kernel-enforced aggregate CPU and memory limits on macOS and broader process isolation on other platforms.
- 🟢 [Done] One-shot design proposals in Preferences from OpenAI, Google Gemini and OpenAI-compatible local servers. The Rust service validates the circuit and source revision before showing a reviewable diff, and the user explicitly applies the patch.
- 🟢 [Done] Codex and Google Antigravity setup for the workspace MCP tools, including the authenticated desktop approval flow. See docs/MCP.md.
- 🟢 [Done] Codex CLI MCP smoke test on macOS. Discovered 49 tools and executed `project.inspect` over stdio against a disposable workspace.
- 🟢 [Done] Google Antigravity desktop app connected to the authenticated loopback MCP server and discovered 49 tools in a temporary ALU32 project on macOS.
- 🟢 [Done] macOS approval control check on 2026-11-24. An independent HTTP MCP client read revision 0, previewed and submitted a node rename, then retried after approval in Preferences. The GPU canvas, library, and inspector showed the renamed node at revision 1.
- 🟢 [Done] Google Antigravity desktop MCP compatibility via `/mcp/portable`. The client showed 49 enabled tools with underscore-separated names. Its native calls read ALU32 at r000, previewed and submitted a rename, retried after macOS diff approval, and read back r001. Verified the renamed node on the native GPU canvas.
- ⭕️ [Pending] Live OpenAI and Gemini proposal generation, diff review, and application. Provider API credentials were unavailable in the test environment.
- 🟢 [Done] Design AI proposals can be compared with the current circuit before applying. Both run the saved stimulus and assertion table in Rust, with revision- and stimulus-version-tagged summaries. The comparison leaves the workspace and its active trace untouched.
- 🟢 [Done] Internal read-only rmcp client and budgeted Design AI exploration. Each run allows 2–4 model requests with a 90-second deadline, feeds assertion summaries into subsequent proposals, and displays candidates against saved stimuli. Applying a selected candidate checks the circuit revision and stimulus version. macOS QA with a loopback fixture verified two candidates, first-failure detail, stop at 3/3 requests including one rejected response, and the selected change on the GPU canvas at r001.
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
