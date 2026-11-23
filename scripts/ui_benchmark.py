"""Seed an isolated workspace or summarize live UI traces; never drives the UI."""
import argparse
import bisect
import json
import math
import statistics
from pathlib import Path


def prepare(root, count, wide_fanout=False):
    dest = root / ".logic" / "workspace.json"
    if dest.exists():
        raise SystemExit(f"Refusing to overwrite {dest}")
    nodes = [dict(id=f"n{i}", name=f"n{i}", kind="input" if i < 2 else "add",
                  width=32, x=float(i % 40 * 260), y=float(i // 40 * 180)) for i in range(count)]
    edges = [dict(id=f"e{i}_{port}", **{"from": f"n{source}"}, to=f"n{i}", port=port)
             for i in range(2, count) for port, source in [("a", i - 1), ("b", i - 2)]]
    state = dict(schema=1, revision=0, document=dict(name=f"Large_{count}", nodes=nodes, edges=edges),
                 history=[], undo=[], redo=[], report=None, experiments=[], selected="n2", locale="en",
                 panels=dict(library="left", inspector="right", results="bottom"))
    if wide_fanout:
        for edge in edges:
            if edge["port"] == "b":
                edge["from"] = "n0"
        state["selected"] = "n0"
        state["selection"] = [f"n{i}" for i in range(0, count, 512)]
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(state))
    print(root)


def stats(values):
    return dict(p50=statistics.median(values), p95=sorted(values)[math.ceil(len(values) * .95) - 1])


def summarize(paths, out, moves=12):
    runs = []
    for path in paths:
        trace = json.loads(path.read_text())
        samples = trace["samples"]
        if len(samples) != moves or any(s["native_present_request_ms"] is None or
                                    set(s["webviews"]) != {"controls", "library", "inspector", "results"}
                                    for s in samples):
            raise SystemExit(f"Incomplete run: {path}")
        measured = samples[2:]
        runs.append(dict(source=path.name, profile=trace["profile"], web_sync=trace["web_sync"],
                         nodes=samples[0]["nodes"], measured_samples=len(measured),
                         commit_ms=stats([s["commit_ms"] for s in measured]),
                         native_present_request_ms=stats([s["native_present_request_ms"] for s in measured]),
                         all_webviews_paint_opportunity_ms=stats([max(v["paint_opportunity_ms"] for v in s["webviews"].values()) for s in measured]),
                         one_webview_sync_body_bytes=stats([s["webviews"]["inspector"]["sync_body_bytes"] for s in measured]),
                         samples=samples))
    for run in runs:
        packets = [sample.get("native_sync") for sample in run["samples"][2:]]
        if all(packet is not None for packet in packets):
            run["native_sync_body_bytes"] = stats([packet["body_bytes"] for packet in packets])
            run["native_sync_kinds"] = sorted({packet["kind"] for packet in packets})
    output = dict(warmup_moves=2, measured_moves_per_run=moves - 2, runs=runs)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(output, indent=2) + "\n")
    for r in runs:
        print(r["nodes"], r["web_sync"], "web p50", round(r["all_webviews_paint_opportunity_ms"]["p50"], 2),
              "native p50", round(r["native_present_request_ms"]["p50"], 2))


def summarize_native_drag(paths):
    reference = None
    results = []
    for path in paths:
        trace = json.loads(path.read_text())
        inputs = trace.get("inputs", [])
        if trace.get("error") or trace.get("errors") or len(inputs) != 250:
            raise SystemExit(f"Incomplete run: {path}")
        positions = [(sample["index"], sample["offset"]) for sample in inputs]
        condition = [trace[key] for key in ("nodes", "moved", "target_hz", "zoom", "canvas_window_px")]
        condition.append(positions)
        if reference is not None and condition != reference:
            raise SystemExit(f"Mismatched replay conditions: {path}")
        reference = condition
        times = [sample["input_at"] for sample in inputs]
        intervals = [(b - a) * 1000 for a, b in zip(times, times[1:])]
        frames = trace["frames"]
        shown = [frame for frame in frames if frame["presented_at"] > 0]
        if not shown or any(times[frame["input"]] > frame["delivered_at"] or
                            frame["delivered_at"] > frame["presented_at"] for frame in shown):
            raise SystemExit(f"No valid Metal presentation timing: {path}")
        result = dict(
            trace=str(path), input_hz=(len(times) - 1) / (times[-1] - times[0]),
            interval_ms=stats(intervals), presented_frames=len(shown),
            skipped_drawables=len(frames) - len(shown),
            input_to_display_ms=stats([
                (frame["presented_at"] - times[frame["input"]]) * 1000 for frame in shown]),
        )
        if trace.get("release") is not None:
            release = trace["release"]
            displayed = trace.get("release_display_at")
            if displayed is None or not release["released_at"] <= release["saved_at"] <= displayed:
                raise SystemExit(f"No valid committed frame timing: {path}")
            result["release"] = dict(
                revision=release["revision"],
                save_confirmation_ms=(release["saved_at"] - release["released_at"]) * 1000,
                display_ms=(displayed - release["released_at"]) * 1000,
                save_to_display_ms=(displayed - release["saved_at"]) * 1000,
            )
        if "stages" in trace:
            marks = {(mark["input"], mark["stage"]): mark for mark in trace["stages"]}
            breakdown = []
            ticks = sorted({mark["target_at"] for mark in trace.get("refresh", [])})
            if len(ticks) >= 2:
                result["display"] = trace["display"]
                result["present_paced"] = trace["present_paced"]
                result["refresh_interval_ms"] = stats([
                    (b - a) * 1000 for a, b in zip(ticks, ticks[1:])])
            for frame in shown:
                index = frame["input"]
                cpu = marks.get((index, "cpu"))
                gpu = marks.get((index, "gpu"))
                if (not cpu or not gpu or not
                        frame["delivered_at"] <= gpu["start_at"] <= gpu["end_at"] <= frame["presented_at"]):
                    raise SystemExit(f"Missing or invalid stage timing for input {index}: {path}")
                surface = cpu["surface"]
                breakdown.append(dict(
                    input_wait_ms=(frame["delivered_at"] - times[index]) * 1000,
                    scene_ms=cpu["assembly_ms"],
                    acquire_ms=surface["acquire_ms"],
                    prepare_ms=surface["prepare_ms"],
                    encode_submit_ms=surface["encode_submit_ms"],
                    pre_gpu_ms=(gpu["start_at"] - frame["delivered_at"]) * 1000 - cpu["assembly_ms"],
                    gpu_ms=(gpu["end_at"] - gpu["start_at"]) * 1000,
                    display_wait_ms=(frame["presented_at"] - gpu["end_at"]) * 1000,
                    total_ms=(frame["presented_at"] - times[index]) * 1000,
                ))
                if len(ticks) >= 2:
                    gpu_end = gpu["end_at"]
                    shown_at = frame["presented_at"]
                    first = bisect.bisect_left(ticks, gpu_end)
                    if 0 < first < len(ticks) and shown_at <= ticks[-1]:
                        # Allow for sub-millisecond disagreement between the
                        # display clock's target and Metal's presented time.
                        count = bisect.bisect_left(ticks, shown_at - .001) - first
                        breakdown[-1].update(
                            next_refresh_ms=(ticks[first] - gpu_end) * 1000,
                            extra_refreshes=max(0, count),
                        )
            result["frame_latency"] = trace["frame_latency"]
            result["stages"] = {key: stats([row[key] for row in breakdown if key in row])
                                for key in sorted(set().union(*breakdown))}
            ranked = sorted(breakdown, key=lambda row: row["total_ms"])
            result["p95_frame_breakdown"] = ranked[math.ceil(len(ranked) * .95) - 1]
        results.append(result)
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    seed = commands.add_parser("prepare")
    seed.add_argument("root", type=Path)
    seed.add_argument("--nodes", type=int, choices=[100, 500, 2000], required=True)
    seed.add_argument("--wide-fanout", action="store_true")
    report = commands.add_parser("summarize")
    report.add_argument("traces", nargs="+", type=Path)
    report.add_argument("--out", type=Path, required=True)
    report.add_argument("--moves", type=int, default=12)
    native_drag = commands.add_parser("summarize-native-drag")
    native_drag.add_argument("traces", nargs="+", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.root, args.nodes, args.wide_fanout)
    elif args.command == "summarize":
        summarize(args.traces, args.out, args.moves)
    else:
        summarize_native_drag(args.traces)
