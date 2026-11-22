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
