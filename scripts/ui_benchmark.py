"""Seed an isolated workspace or summarize live UI traces; never drives the UI."""
import argparse
import json
import math
import statistics
from pathlib import Path


def prepare(root, count):
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    seed = commands.add_parser("prepare")
    seed.add_argument("root", type=Path)
    seed.add_argument("--nodes", type=int, choices=[100, 500, 2000], required=True)
    report = commands.add_parser("summarize")
    report.add_argument("traces", nargs="+", type=Path)
    report.add_argument("--out", type=Path, required=True)
    report.add_argument("--moves", type=int, default=12)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.root, args.nodes)
    else:
        summarize(args.traces, args.out, args.moves)
