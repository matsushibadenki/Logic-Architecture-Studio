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

### Wide fanout and simultaneous moves

Run `cargo run --release -p studio-renderer --example large_circuit -- --stress`. The fixture contains 2,000 nodes and 3,996 wires. One input drives 1,998 destinations. Each sample moves that input and three other nodes across separate entity blocks. The benchmark verifies matching quads and labels against a fresh rebuild on every frame.

Apple M4 / Metal, release build, 1,600 × 1,000 px, 30 measured samples per view after five warmups. Times are in milliseconds.

| View | Visible nodes / wires | Delta sync p50 / p95 | Scene assembly p50 / p95 | Offscreen render completion p50 / p95 |
| --- | ---: | ---: | ---: | ---: |
| Overview | 2,000 / 3,996 | 26.29 / 81.55 | 19.31 / 100.68 | 27.30 / 60.18 |
| Detail | 63 / 1,130 | 20.06 / 46.35 | 1.47 / 9.07 | 7.27 / 16.46 |

The render-completion column starts after scene assembly. Native input delivery, scheduling and presentation are outside this offscreen measurement.

### Wide-fanout A/B on macOS

Release Metal builds on the same Mac used 30 samples per view, with four nodes moving together in a 2,000-node circuit. The source with 1,998 destinations was one of the moved nodes. Each build verified quads and labels against its own fresh scene reconstruction on every measured frame. Values are p50 / p95 in milliseconds.

| Stage | Overview before | Overview after | Detail before | Detail after |
| --- | ---: | ---: | ---: | ---: |
| Retained delta sync | 16.88 / 22.70 | 12.11 / 23.04 | 17.59 / 22.53 | 10.51 / 20.49 |
| CPU scene assembly | 10.20 / 21.36 | 14.93 / 29.02 | 1.40 / 2.12 | 1.64 / 3.04 |
| Completed offscreen frame | 16.46 / 28.97 | 20.56 / 30.73 | 5.23 / 11.19 | 7.23 / 9.83 |

Deduplicating blocked-port warnings reduced the final overview frame's glyph upload from 479,520 to 185,568 bytes. The run recorded slower overview assembly and frame completion, leaving both as optimization targets.

Use `scripts/ui_benchmark.py prepare <workspace> --nodes 2000 --wide-fanout` to seed the native QA circuit. Consecutive pointer drags committed native group moves in both release builds. The captured gestures moved different sets of nodes, and restarting the improved QA instance reset its latency trace. A matched native latency comparison remains pending. The current trace records commit and present-request timing from the final gesture event. Measuring in-flight preview latency under saturated input also requires motion-event counts and preview-frame timing.

### Overview regression and rate-controlled drag replay

On Apple M4 Metal, two repeat runs of the previous release reproduced a smaller
overview assembly regression than the original run. Comparing the code just
before and after the previous optimization, wire geometry rose from 1.39 ms to
1.65 ms in the median of run medians. Node geometry stayed around 1.5 ms.
An ablation on the previous release left routing and cache handling unchanged
and disabled only warning deduplication. Wire geometry fell from 1.81 ms to
1.52 ms in that run set despite emitting more warning labels. This isolates
ordered-tree warning lookups as a contributor to the regression. The original
10.20 to 14.93 ms overview swing did not recur at that magnitude.

This revision uses hash membership for warnings and retained fragments, preserves
the label and wire order, and resolves styling only when a node fragment needs
rebuilding. During drag it computes preview routes against retained edge
topology instead of cloning the committed edge map, and reuses the source
obstacle-clearance check for successive branches from the same anchor.
Obstacles may still change the path of unrelated wires.

Four alternating release runs per build with `--stress` measured 30 committed
frames per view per run and checked each against a fresh scene rebuild.
Overview assembly went from 3.55 / 4.48 ms to 3.21 / 5.07 ms (median of run
p50 / p95). Wire geometry went from 1.73 / 2.40 ms to 1.33 / 2.07 ms.
The assembly median improved, while the measured assembly tail did not.

Replay with `cargo run --release -p studio-renderer --example large_circuit -- --drag-replay`.
The Apple M4 benchmark schedules 250 positions at 4 ms intervals on the
2,000-node, 3,996-wire fixture. It moves n0, n512, n1024 and n1536 together
along the same repeating diagonal, with offsets up to 75 × 37.5 world units.
At the start of each frame it takes the latest due position and counts skipped
positions. A frame is timed from that position's scheduled arrival through
completion of the actual Metal GPU work. The table uses the median of per-run
percentiles across four alternating runs per build.

| Preview stage | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| Routing and visible edge query | 3.59 / 5.43 ms | 3.31 / 4.42 ms |
| CPU scene assembly, including routing | 7.82 / 11.17 ms | 6.98 / 8.69 ms |
| Scheduled input to GPU completion | 19.58 / 22.95 ms | 18.82 / 21.25 ms |

The improved build completed 58–63 preview frames per stream, compared with
54–60 before. The replay times the application's renderer on the physical Mac
using an offscreen target. AppKit/Tao input delivery and compositor presentation
require a window-level test and remain pending.

### Overview tail follow-up

The release benchmark now reports the three slowest frames with stage
breakdowns and, on macOS, on-thread CPU time. Assembly includes no GPU wait.
The slowest frame of the four-run A/B took 27.48 ms elapsed but consumed
5.37 ms of renderer-thread CPU time. At least one large tail spike therefore
came primarily from time off CPU. The benchmark does not identify which system
activity caused that gap.

A retained node used to walk its incident edges to check required inputs even
on a geometry cache hit. That check now runs while rebuilding the fragment.
Connect, disconnect and rewire invalidate the previous and new destination
fragments as applicable. The wire loop also reuses its source label lookup.

The same Apple M4 and 2,000-node fanout workload ran four times per build,
alternating order, with 30 overview samples per run. The baseline retains
the changes from the previous overview optimization. Figures are medians
of per-run p50 and p95, respectively.

| Overview assembly stage | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| Total elapsed | 3.54 / 4.41 ms | 2.56 / 2.95 ms |
| Renderer-thread CPU | 3.52 / 4.32 ms | 2.56 / 2.89 ms |
| Node geometry elapsed | 1.83 / 2.30 ms | 1.08 / 1.28 ms |
| Wire geometry elapsed | 1.51 / 1.92 ms | 1.30 / 1.56 ms |

Window-level validation remains pending. The available native drag control
does not specify event cadence, and the application's timing probe ends
at the present request. Neither can verify a 250 Hz pointer-to-visible-frame
measurement.

### macOS window presentation timing

The release desktop application supports an opt-in, nondestructive drag replay.
Seed a new QA workspace and launch the desktop executable with these variables:

```sh
python3 scripts/ui_benchmark.py prepare "$qa" --nodes 2000 --wide-fanout
LAS_WORKSPACE="$qa" LAS_SETTINGS_ROOT="$qa/settings" \
  LAS_NATIVE_DRAG_TRACE="$qa/drag.json" /path/to/LogicArchitectureStudio
python3 scripts/ui_benchmark.py summarize-native-drag "$qa/drag.json"
```

One worker emits 250 motion positions against absolute 4 ms deadlines using
the same diagonal as the offscreen benchmark. The replay feeds the native
editor's pointer handler on the UI thread. Its bounded pending slot keeps
the newest sample when rendering cannot keep up. The drag is cancelled at
the end and no position changes are saved. This measures application input
through actual Metal window presentation. It bypasses the physical pointing
device and AppKit/Tao event dispatch.

The diagnostic window's Metal layer attaches a presented callback to each
drawable acquired for a drag preview. The callback reads Metal's
`presentedTime`, using the same monotonic host clock as the input timestamps.
Callbacks that return zero identify skipped drawables and are excluded from
latency percentiles. A run with no positive presentation timestamps is invalid.

The final Apple M4 comparison used release builds, a 1440 × 900 physical-pixel
window, zoom 0.06697, and the same 2,000 nodes, 3,996 wires, four selected
nodes and 250 input positions. The baseline includes the earlier fanout
optimizations. The only measured code difference is the latest scene
assembly change that caches required-input checks and reuses wire label
lookups. Both builds use identical replay and presentation instrumentation.

| Paired run | Before p50 / p95 | After p50 / p95 | Presented frames before / after |
| --- | ---: | ---: | ---: |
| 1 | 69.59 / 84.27 ms | 47.48 / 56.12 ms | 24 / 21 |
| 2 | 58.34 / 86.98 ms | 61.34 / 101.71 ms | 12 / 19 |
| 3 | 49.11 / 58.91 ms | 61.65 / 109.98 ms | 13 / 19 |
| Median of run percentiles | 58.34 / 84.27 ms | 61.34 / 101.71 ms | 49 / 59 total |

Effective input rates were 249.89–250.14 Hz. Four pairs were attempted with
alternating launch order. One pair was excluded because every drawable in
its improved-build run returned a zero presentation time. The retained
runs show more presented previews and higher median run latency in the
improved build. The spread between runs is large, so the data does not
establish a reproducible window-level latency improvement. All retained
workspaces had equal circuit topology, positions and selection at revision
zero after the run.

### Window latency breakdown and queue depth

Use the same native drag command above. Both builds use VSync and the
default desired frame latency of two. The diagnostic-only
`LAS_NATIVE_DRAG_SHALLOW_QUEUE=1` option requests a desired latency of one
(Metal's drawable limit changes from three to two).

Each presented frame links its input sequence number to these measurements:

- Input wait runs from publishing the motion to the UI thread consuming it.
- Scene assembly includes preview routing and scene generation.
- Pre-GPU time outside assembly includes pointer processing, drawable
  acquisition, render preparation, submission, and GPU queue wait. Drawable
  acquisition is reported separately as a subset of this interval.
- GPU processing is bracketed by Metal completion timestamps on both sides
  of the render command in the same submission. That interval includes
  scheduling between the markers and the draw; it is neither a CPU callback
  timestamp nor an exclusive hardware-counter measurement of rasterization.
- Display wait runs from the trailing GPU marker to Metal's actual
  `presentedTime`.

`summarize-native-drag` also reports the breakdown of the exact frame that
determines p95 per run. Stage p95 values describe different frames in
general and should not be summed to reconstruct total p95.

A same-build queue-depth trial reproduced a display-heavy tail. One
baseline p95 frame took 82.54 ms, including 50.00 ms after the GPU marker.
The shallow-queue trial also produced a 108.89 ms p95 frame with 32.92 ms
blocked in drawable acquisition. The production queue setting remains at
the default; reducing drawable count can shift the wait ahead of the GPU.

The implementation batches overview wire warnings into one text layer
above wires and below nodes. On Apple M4, both release builds used the
same 2,000-node fixture, same four nodes, identical input positions,
1440 × 900 window, VSync and default queue depth. Of six paired runs,
two had measured average rates within 1 Hz of the 250 Hz target in both
builds (249.65–250.34 Hz). Four pairs were excluded for cadence mismatch.
Values below are the medians of the two runs' p95 measurements.

| Stage | Before | After |
| --- | ---: | ---: |
| Input wait | 15.16 ms | 20.82 ms |
| Scene assembly | 24.04 ms | 21.63 ms |
| CPU time before GPU, outside assembly | 14.26 ms | 17.12 ms |
| Encode and submit, within the previous row | 8.28 ms | 7.35 ms |
| GPU bracket | 4.03 ms | 7.95 ms |
| Display wait | 30.86 ms | 56.81 ms |
| Input to display | 71.39 ms | 87.86 ms |

The two builds presented 31 and 36 previews respectively. At the
end-to-end p95 frame in each run, median post-GPU display wait increased
from 24.94 to 56.81 ms, while the GPU bracket changed from 1.42 to 1.94 ms.
That identifies the interval driving the measured regression. The trace
does not identify the WindowServer or display scheduling event responsible
for the wait. Batching reduced the encode/submit p95, and the full
presentation-tail target remains open.

### Display refresh and presentation pacing

The native replay now records CoreVideo refresh targets for the primary
display. The callback runs independently of editor work and uses the same
monotonic host clock as the Metal timestamps. The report counts refresh
targets between GPU completion and presentation, with a 1 ms comparison
tolerance. The count describes refresh opportunities. It does not imply
that a drawable was eligible for any particular compositor deadline.

For the paced run, set `LAS_NATIVE_DRAG_PRESENT_PACED=1` together with
`LAS_NATIVE_DRAG_TRACE`. With this option the diagnostics submit at most
one preview at a time until Metal returns its presented or skipped callback.
The producer continues sampling at 250 Hz, with a latest-position slot.
Both configurations use the same executable, scene, VSync, drawable count
and input path.

On Apple M4, the screen refreshed every 16.67 ms. Three of four alternating
pairs met the measured input cadence criterion (249.78–249.80 Hz).
The fourth pair was excluded because one run averaged 242.98 Hz.
The table uses the median of per-run percentiles over the three valid pairs.

| Metric | Ordinary replay | Presentation-paced replay |
| --- | ---: | ---: |
| Input-to-display p50 | 57.25 ms | 60.03 ms |
| Input-to-display p95 | 88.98 ms | 80.63 ms |
| GPU-complete-to-display p95 | 57.61 ms | 45.44 ms |
| Extra refresh targets at each run's end-to-end p95 frame (median) | 3 | 2 |
| Visible previews across the three runs | 55 | 20 |

Pacing reduced the observed tail and presented substantially fewer previews,
including only three in one paced run. It remains a diagnostic option.
The normal editor path keeps its throughput-oriented scheduling.

In a separate Instruments Metal System Trace captured during the same
replay, the system display surface queue recorded 675 entries. The Core
Animation client trace recorded 21 present requests, and confirmed all
18 preview drawable IDs in the app trace. The compositor interval and
surface swap exports returned schemas without event rows on this host,
so the system queue entries could not be associated with those drawables.
CoreVideo and Metal therefore bound the wait to the post-GPU presentation
interval and count the intervening refresh opportunities. They do not
identify the exact private WindowServer scheduling decision for a frame.

### Repeated labels in the normal preview path

The text renderer now reuses the shaped line and its ink bounds across
adjacent labels with identical text, font size and weight. Every label
still uses its own position, clip, color and draw order.

Four alternating release runs on Apple M4 used the ordinary preview
scheduler and the same 2,000-node, four-node drag fixture at
249.75–249.80 Hz. Medians of the per-run percentiles were:

| Metric | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| Draw preparation | 1.07 / 1.32 ms | 0.60 / 0.81 ms |
| Input to display | 64.08 / 66.27 ms | 64.87 / 66.92 ms |

All four pairs are included. The improved build showed 114 previews
across the four runs versus 129 before, with the largest count difference
in the fourth pair (14 versus 29). The preparation improvement was not
reflected in end-to-end presentation latency.

### Obstacle clearance during drag preview

Boolean obstacle queries now visit grid buckets directly and stop on
the first intersection. Queries that enumerate routing candidates still
use ordered, deduplicated results. The scene consumes routed links and
routes in UUID order without individual tree removals.

Three matched pairs on Apple M4 used the existing 2,000-node / 3,996-wire
fixture, four-node path and 249.79–249.80 Hz input. A fourth pair had
different viewport zoom and was excluded. Medians of per-run percentiles:

| Metric | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| Preview scene assembly | 8.64 / 9.13 ms | 8.23 / 8.42 ms |
| Input to display | 58.34 / 65.69 ms | 61.71 / 65.66 ms |

Visible previews totaled 102 before and 96 after. Scene-assembly p95
improved, and input-to-display p95 was essentially unchanged.

### Wire clearance during the no-crossing pass

The shared Rust router now visits grid buckets directly when a candidate
segment only needs a yes/no clearance decision. It stops at the first
conflict. The crossing-enabled pass still deduplicates candidate wires
before adding penalties.

An Apple Silicon release CPU benchmark alternated the previous and
updated clearance implementations on 2,000 nodes and 1,950 wires.
After four warm-up pairs, all 60 measured pairs produced identical
route points, bridge locations and blocked flags. The median and p95
routing times (including fixture construction) were 2.26 / 3.80 ms
before and 1.89 / 2.28 ms after. Window presentation was outside this
measurement.

### Detour search

The direct route and the first detour pass share one terminal-clearance
decision. Detour search identifies each undirected grid edge by its lower
vertex index and axis.

An Apple Silicon release CPU benchmark alternated the two implementations
for 60 measured pairs after four warm-up pairs per workload. All route
points, bridges and blocked flags matched. The nonplanar workload used
24 independent six-node clusters with 216 wires, 72 of which needed
bridges. Its p50 / p95 went from 101.44 / 139.36 ms to 92.74 / 127.51 ms.
On a simple 2,000-node / 1,950-wire grid, p50 / p95 went from
5.10 / 15.26 ms to 5.15 / 16.98 ms. The benefit was specific to detour-heavy
routing in this run. Window presentation was outside the measurement.

### Detour search allocation reuse

A routing pass now retains the detour coordinate buffers, search distances,
predecessors, priority queue and edge memo between attempts. Each search
resets its contents, including when retrying a wider region or permitting
crossings. Direct routes do not allocate the search buffers.

The same release CPU workloads used 60 alternating measured pairs after
four warm-up pairs. Route points, bridge coordinates and blocked flags
matched on every run. Nonplanar workload p50 / p95 improved from
32.49 / 33.36 ms to 25.92 / 27.56 ms. The 2,000-node grid stayed near
1.71 / 1.77 ms before and 1.72 / 1.77 ms after. These comparisons measure
routing only.

### Grid-indexed detour edge checks

Detour search now stores clearance and cost in a reusable array indexed by
the lower grid vertex and axis. The existing 40,000-vertex search limit
bounds the array. Each attempt resets the entries before use.

On Apple Silicon, 60 alternating CPU benchmark pairs on the 216-wire
nonplanar fixture produced identical route points, bridges and blocked
flags. Routing p50 / p95 improved from 25.56 / 26.59 ms to 21.01 / 21.58 ms.
A separate 30-sample Metal stress run verified retained/full-rebuild
equivalence with four moving nodes in a 2,000-node circuit. Overview
delta synchronization p50 was 3.34 ms before and 3.04 ms after.
This test uses an offscreen target and does not measure window presentation.

### Reuse detour grids across crossing policies

The crossing-free and crossing-enabled attempts now share the coordinate
grid for each search margin of a single wire. The router builds each grid
only when needed. Each attempt still resets search costs, predecessors and
edge checks, and each new wire builds against the latest reservations.

Two release CPU trials each alternated 60 measured before/after pairs on
the 216-wire nonplanar fixture. Every route, bridge coordinate and blocked
flag matched. Routing p50 / p95 was 23.90 / 33.77 ms before and
23.88 / 30.41 ms after in the first trial, and 25.41 / 41.67 ms before
and 24.99 / 34.25 ms after in the second. These measurements cover the
router only.

### GPU upload range detection

Range detection compares blocks of 32 instances first, then inspects
individual instances only in changed blocks. It ends at the 65th range,
when it would select a full upload anyway. The merge and upload decisions
remain identical to the previous algorithm.

A release CPU microbenchmark on Apple Silicon used 10,000 instances of
48 bytes. It alternated implementations over 60 measured batches of
80 calls for each pattern. The p95 per call was 16.37 to 9.98 µs for
unchanged data, 16.21 to 11.78 µs for sparse edits, 17.79 to 1.16 µs for
fragmented edits, and 16.63 to 18.43 µs when nearly every instance changed.
Both implementations produced identical upload ranges in all four patterns.
Metal tests verified identical pixels for delta and full uploads after edits
and buffer reallocation. Window presentation was outside this benchmark.

### Routing endpoint preparation

Scene routing collects node rectangles and port counts once while building
obstacles, then uses a single lookup per wire endpoint. The same preview
placement supplies both obstacle bounds and port anchors.

Six alternating release Metal offscreen runs per build used the 2,000-node,
four-node 250 Hz drag replay. Medians of the run p50 / p95 were:

| Stage | Before | After |
| --- | ---: | ---: |
| Preview routing and visible-edge query | 2.33 / 2.59 ms | 1.88 / 3.44 ms |
| CPU scene assembly | 4.58 / 5.16 ms | 4.20 / 6.32 ms |
| Scheduled input to GPU completion | 15.08 / 17.07 ms | 14.67 / 19.43 ms |

Typical processing improved and the observed tails increased. The replay
uses an offscreen target. Window presentation remains a separate target.

### Repeated drag preview placement

The scene retains only the latest preview's routes. An unchanged
placement reuses the routing result, with visibility evaluated for the
current viewport. Geometry updates and the first non-preview frame
invalidate the cache. A regression test compares the reused preview and
the preview after a committed endpoint move against fresh rendering.

Three alternating Apple Silicon Metal offscreen pairs per workload used
the same 2,000-node circuit and four-node selection. The new
`--hold-preview` option uses the same path sampled in groups of eight
consecutive 250 Hz inputs per placement. `--drag-replay` changes placement
on each input.

| Workload and stage | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| Held placement scene assembly | 3.87 / 4.73 ms | 2.28 / 4.70 ms |
| Held placement input to GPU completion | 13.86 / 16.48 ms | 13.25 / 16.26 ms |
| Continuous movement scene assembly | 3.91 / 4.41 ms | 3.84 / 4.50 ms |
| Continuous movement input to GPU completion | 14.44 / 16.63 ms | 14.87 / 16.86 ms |

These are medians of run percentiles. Window presentation was outside the
measurement.

### Reusing preview routes when a move commits

A position-only commit now installs the latest preview routes and rebuilds
the edge spatial index when the preview's changed rectangles exactly match
the committed changes. Any mismatch uses the normal routing path.

The `--preview-commit` Metal offscreen benchmark renders each four-node
preview before applying its corresponding move in the 2,000-node stress
fixture. Three alternating trials per build each measured 30 samples per
view after warm-up. Every committed scene matched its fresh reconstruction.
The medians of run p50 / p95 for delta synchronization were
3.18 / 4.54 ms before and 0.63 / 1.39 ms after in overview, and
2.92 / 5.92 ms before and 0.59 / 1.10 ms after in detail. This measures
retained-renderer synchronization and excludes persistence and presentation.

### macOS release-to-save and release-to-display timing

Use the same native replay fixture and set `LAS_NATIVE_DRAG_COMMIT=1`
along with `LAS_NATIVE_DRAG_TRACE` and the explicit QA workspace. This
mode **persists the move**. Use a newly seeded workspace for each run.
It renders the final preview, then sends the same Rust pointer-up and
workspace commit used by the native editor. The release timestamp marks
entry to that pointer-up path. Save confirmation follows the service
response, which includes the synchronized move journal write. The
display timestamp is Metal's `presentedTime` for the canonical revision
on the workspace canvas. `summarize-native-drag` reports both intervals
as well as save-to-display time. This replay feeds the Rust editor
directly; hardware and AppKit input delivery are outside the measurement.

The before/after macOS release builds used the same instrumentation and
differed only in whether a matching preview route could be reused on commit.
Each attempt started from a fresh 2,000-node / 3,996-wire workspace with
n0, n512, n1024 and n1536 selected. The replay sent the same 250 positions.
All accepted traces had the same viewport and saved positions. Persisted
moves were checked in the checksummed journal or in the resulting checkpoint.

The final comparison alternated launch order for 12 pairs on Apple M4.
Eleven pairs met the 249–251 Hz cadence criterion. One pair was excluded
because the improved build averaged 239.30 Hz. An earlier pilot series
was used to diagnose cancelled gestures and incomplete startup snapshots
and is outside the final comparison.

| Interval | Before p50 / p95 | After p50 / p95 |
| --- | ---: | ---: |
| Pointer-up to save confirmation | 10.48 / 21.63 ms | 10.93 / 76.62 ms |
| Pointer-up to canonical frame displayed | 62.21 / 83.84 ms | 62.84 / 241.54 ms |
| Save confirmation to canonical frame displayed | 55.73 / 69.12 ms | 54.88 / 164.92 ms |

With 11 observations per build, nearest-rank p95 is the maximum. In the
slowest after run, 76.62 ms elapsed before save confirmation and 164.92 ms
after it. The offscreen synchronization improvement did not translate to
a demonstrated save or presentation latency improvement in this small
window-level comparison.

### Reusing native panel layout

The host now reuses successfully applied WebView bounds and visibility
while the window dimensions, scale, splitter sizes and panel positions
are unchanged. Panel placement invalidates the cache before starting its
transaction. macOS manual QA floated the Inspector, docked it back on the
right, and changed the right splitter from 300 to 310 points.

Four alternating Apple M4 release pairs used the 2,000-node, four-node
window-level replay at 249.76–249.80 Hz. The median of run p95 values for
pre-GPU time outside scene assembly was 13.72 ms before and 13.50 ms
after. Input-to-display p95 was 66.70 ms before and 66.76 ms after.
Visible previews totaled 130 before and 136 after. This confirms the
elimination of layout work on cache hits but does not establish an
end-to-end latency improvement.

### Native position commit with an inline delta

For a position-only native gesture based on the current sync cursor, the
service returns the same filtered delta that a subsequent sync request
would return. The response is sent after durable storage succeeds. The
native host applies the acknowledged delta to the matching cache before
drawing, skipping the separate sync round trip on the presentation path.
A stale or mismatched cursor uses the ordinary sync loop. Responses from
polls that raced with a newer acknowledgement cannot replace its cache.

Ten alternating Apple M4 comparisons used the same release executable,
workspace, viewport, four-node path and 249.77–250.01 Hz input rate. Set
`LAS_NATIVE_SYNC_BASELINE=1` with the native commit replay to disable the
inline delta. Every after trace confirmed inline application, and
persisted node positions matched in all ten pairs.

| Interval | Polling p50 / p95 | Inline delta p50 / p95 |
| --- | ---: | ---: |
| Save acknowledgement to canonical frame displayed | 52.46 / 55.41 ms | 54.39 / 56.92 ms |
| Pointer release to canonical frame displayed | 62.36 / 63.57 ms | 62.94 / 63.39 ms |

For ten samples per build, nearest-rank p95 equals the maximum. The
inline response removed the extra request from the critical path. A
reduction in saved-to-visible latency was not observed on this display.

### Committed-frame timing by stage

The native commit replay records when the inline delta has been applied
and when the canonical renderer starts. It preserves the Metal marker
pair for that revision and the drawable's actual presentation time.
The UI gives that native frame priority by submitting it before sending
the WebView change notification. The fallback still wakes the sync loop.
Use `LAS_NATIVE_RELEASE_NOTIFY_BASELINE=1` with the commit replay to
measure the former order.

Ten alternating pairs used the same Apple M4 release build, 2,000-node
fixture, four-node drag and displayed viewport. One pair used polling
fallback and was excluded. All nine accepted pairs applied the inline
delta, had matching persisted endpoints, and ran at 249.75–249.94 Hz.
Nearest-rank p95 is the maximum at this sample size.

| Interval | Before p95 | Native submission first p95 |
| --- | ---: | ---: |
| Save confirmation to delta applied | 0.04 ms | 0.04 ms |
| Delta applied to draw start | 3.48 ms | 3.31 ms |
| Draw start to GPU bracket start | 8.11 ms | 7.78 ms |
| GPU bracket | 3.91 ms | 3.59 ms |
| GPU bracket end to display | 42.67 ms | 43.50 ms |
| Save confirmation to display | 56.74 ms | 56.34 ms |

The display wait was the dominant stage, with a median near 42 ms in both
builds. Prioritizing native submission reduced the measured draw-start
tail slightly. It did not demonstrate a repeatable reduction in display
wait, and the end-to-end medians were 53.26 and 55.54 ms respectively.

### Metal queue depth on pointer release

The replay records which preview drawables are actually displayed between
the committed frame's GPU-end marker and presentation. The summary also
counts intervening CoreVideo refresh targets. With the default three-drawable
Metal queue, two earlier previews presented in that interval. Removing the
diagnostic-only forced preview before pointer-up left two earlier previews
in eight of nine matched pairs. Its saved-to-visible p95 was 58.13 ms with
the forced preview and 65.70 ms without it, so removing the forced draw
alone did not solve the queueing delay. The replay now follows the live
pointer-up sequence by default. Restore the old experiment with
`LAS_NATIVE_RELEASE_EXTRA_PREVIEW_BASELINE=1`.

Eight separate pairs compared queue depth with no forced preview in either
arm. Both arms used the same Apple M4 release executable, 2,000 nodes, four
moved nodes, identical positions and endpoints, and 249.78–249.93 Hz input.
VSync stayed enabled; the tested display refreshed about every 16.67 ms.
No pairs were excluded.

| Interval or count | Three drawables | Two drawables |
| --- | ---: | ---: |
| Save acknowledgement to presentation p50 / p95 | 54.36 / 56.90 ms | 35.83 / 37.83 ms |
| GPU end to presentation p50 / p95 | 41.99 / 42.60 ms | 24.17 / 25.10 ms |
| Drawable acquisition, p95 | 3.57 ms | 0.04 ms |
| Earlier previews and refreshes before committed presentation, every run | 2 | 1 |
| Visible previews across all runs | 269 | 241 |

Nearest-rank p95 is the maximum for eight observations. The one-refresh
difference and actual preview presentation timestamps identify queueing in
front of the committed frame on this host. The normal macOS workspace now
requests the two-drawable configuration. For a diagnostic A/B, the trace
defaults to the old three-drawable setting and
`LAS_NATIVE_DRAG_SHALLOW_QUEUE=1` selects the new setting. Sustained input
and other display configurations still merit tail-latency measurement.

### Sustained drag queue analysis

The native trace summarizer now groups previews by the number of earlier
GPU-completed previews that Metal actually reported as presented between
the current frame's GPU completion and presentation. Each group reports
drawable acquisition, display wait and end-to-end latency jointly. Skipped
drawables do not contribute to that count.

Reanalysis of the eight matched queue-depth pairs above gives the following
sustained-preview results. Latencies are medians of per-run p95s.

| Metric | Three drawables | Two drawables |
| --- | ---: | ---: |
| Drawable acquisition | 10.64 ms | 10.89 ms |
| GPU completion to presentation | 42.70 ms | 25.81 ms |
| Input to presentation | 66.87 ms | 51.44 ms |
| Visible previews across eight runs | 269 | 241 |

At the end-to-end p95 frame, two preceding previews were presented in
all eight three-drawable runs. With two drawables, seven runs had one
preceding preview and one run had zero. The acquisition p95 remained close
while post-GPU wait fell by roughly one display refresh. This characterizes
the existing queue-depth change using saved real-window traces. Further
scheduling changes still need a matched live comparison with visible
preview throughput included.

### Minimum interval between drag redraws

Set `LAS_NATIVE_DRAG_INTERVAL_MS` (1–100 ms) to opt into minimum spacing
between drag redraw starts. The native event loop keeps the most recent
drag state and wakes at the deadline when it would otherwise sleep.
Pointer release still follows the normal commit and redraw path. The replay
uses the same cadence rule and checks it on each 250 Hz producer tick.

On macOS, four alternating matched pairs per setting used the same
release binary, 2 Drawable configuration, 2,000-node wide-fanout fixture,
four selected nodes, path, viewport and 250 Hz replay. Recorded input
rates ranged from 249.77 to 249.97 Hz. Each latency is the median of the
per-run p95 values. Preview counts are totals. The two interval settings
had separate baseline runs.

| Metric | 12 ms baseline | 12 ms interval | 20 ms baseline | 20 ms interval |
| --- | ---: | ---: | ---: | ---: |
| Input to actual presentation | 50.52 ms | 49.44 ms | 51.68 ms | 46.14 ms |
| Drawable acquisition | 11.26 ms | 10.15 ms | 11.09 ms | 4.30 ms |
| GPU completion to actual presentation | 26.51 ms | 26.55 ms | 26.99 ms | 31.59 ms |
| Visible previews | 128 | 110 | 111 | 74 |

The 20 ms interval reduced acquisition wait and end-to-end tail latency,
while increasing post-GPU wait and reducing visible previews by a third.
The regular editor keeps its unpaced default. The A/B used synthetic
native drag replay through real Metal presentation.

### Overlap scene assembly with drawable acquisition

The macOS preview can assemble its scene on a scoped CPU worker while the
window thread acquires the drawable. The retained scene cache is borrowed
exclusively. Texture acquisition, GPU encoding, presentation and the
committed redraw stay on the window thread. Set
`LAS_GPU_OVERLAP_DRAWABLE=1` to enable the experimental path. This path
does not impose an interval between previews.

Six alternating before/after pairs used one release build, two drawables,
the same 2,000-node wide-fanout circuit, four selected nodes, viewport and
250 Hz input path. Observed input rates were 249.52–249.97 Hz. Every run
also completed the drag commit at r001. Latencies are medians of each
run's p95. Counts are totals over six runs per arm.

| Metric | Serial assembly and acquisition | Overlapped |
| --- | ---: | ---: |
| Input to actual presentation | 51.60 ms | 50.42 ms |
| Drawable acquisition | 6.41 ms | 14.13 ms |
| GPU completion to actual presentation | 25.30 ms | 25.46 ms |
| Visible previews | 176 | 176 |

The wall time reported for acquisition rises because it starts earlier
and includes time that now overlaps scene work. End-to-end p95 improved
in three pairs and regressed in three. This difference does not justify
enabling the path by default. An 8 ms interval comparison also preserved
the visible count (119 each) but raised end-to-end p95 from 50.03 to
50.55 ms. Preparing buffers before acquisition raised p95 from 50.04 to
50.53 ms with visible counts of 125 and 128. Those trials used four
matched pairs per setting.

### Compare matched preview runs

Pair alternating runs explicitly to keep run-to-run variation visible:

```sh
python3 scripts/ui_benchmark.py compare-native-drag \
  --pair before-1.json after-1.json \
  --pair before-2.json after-2.json
```

The command checks replay geometry and input positions, display identity,
two-drawable configuration, and a measured input rate within 1% of 250 Hz.
It reports after-minus-before p95 differences for input-to-display,
acquisition, and post-GPU display wait. It includes both total visible
previews and the number of pairs that improved latency without losing
previews. The median of the paired deltas preserves the pairing when
summarizing the results.

## Circuit validation, layout and stimulus evaluation

On 2026-11-28, two macOS release binaries ran the same in-process Rust driver, first with the core from `02f6ba3` and then with the refactored core. Each value is the median of five calls measured with `Instant`. Builds and integration tests had finished before the reported runs.

| Operation | Before | After |
|---|---:|---:|
| Validate and generate Verilog | 92.8 ms | 10.7 ms |
| Auto-arrange, commit in memory and serialize the document | 2,139.7 ms | 14.0 ms |
| Run 24 saved stimuli with expectations and retain all node samples | 5,649.7 ms | 197.1 ms |

The fixture has one 1-bit input (`n0`), 1,998 NOT gates and one output. Gate `i` reads node `max(0, i − 40)`. The output reads the last gate. The input alternates between 0 and 1 and the expected output equals the input. Both binaries passed all 24 expectations and produced matching hashes for the Verilog text, serialized layout and full simulation samples. These timings cover CPU-side circuit operations. Display presentation and durable filesystem writes are measured separately.
