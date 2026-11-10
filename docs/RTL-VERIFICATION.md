# Generated RTL verification

🟢 [Done] Icarus adapter and sampled comparisons, macOS 2026-11-10.

In Results → Toolchain, use the play icon beside `iverilog` or `verilator`. Icarus requires both `iverilog` and `vvp`; Verilator requires its compiler and a working C++/make toolchain. The GUI uses the independent built-in ALU contract (320 vectors). The info icon explains the scope. English, Japanese and Simplified Chinese labels are provided. Arbitrary combinational designs can use explicit vectors through GUI IPC/MCP:

```json
{"method":"verify.rtl","args":{"expected_revision":0,"vectors":[{"inputs":{"a":"0xffffffffffffffff","b":"1","op":"0"},"expected":{"result":"0"}}]}}
```

This example expects a 64-bit ALU. Decimal/`0x` strings retain exact 64-bit precision. Omit `vectors` to use the ALU contract; provide 1–4096 vectors otherwise, with exactly the design's inputs and every expected output. Up to 131,072 sampled output values and a 16 MiB generated testbench are allowed. MCP jobs require `--allow-write`, because execution writes artifacts.

Two independent counts are shown:

- **RTL / Rust**: The selected simulator output matches Rust evaluation for the supplied inputs.
- **Contract**: Rust output matches the supplied independent expected values.

Success requires both counts to equal the vector total. RTL/Rust agreement alone can describe the same incorrect circuit, so it is not sufficient. These are sampled combinational tests, not exhaustive equivalence or formal proof. The report is revision-stamped; later edits show it as out of date. The circuit, Undo/Redo and existing Rust behavioral report are not modified. The display is transient panel UI state; reproduction artifacts remain on disk after restart.

## Execution and artifacts

The service captures the document Arc only after checking `expected_revision`, then releases the Workspace mutex. One RTL job per service is allowed. It generates validated `design.v` and `testbench.v`, compiles with fixed `iverilog -g2012 -s __las_verification_tb -o simulation.vvp design.v testbench.v`, and executes `vvp -n simulation.vvp`. No shell or client-provided RTL, command, path or script is accepted. See the official [compiler flags](https://steveicarus.github.io/iverilog/usage/command_line_flags.html) and [runtime flags](https://steveicarus.github.io/iverilog/usage/vvp_flags.html).

Compile and simulation share a 30-second external-process deadline. Output is monitored every 20 ms with a 4 MiB per-phase ceiling; output may overshoot between checks. Unix process groups are terminated on timeout/output overflow and the child is reaped. This is not an OS sandbox, hard memory/CPU/disk quota or user-cancellable job system; those remain unfinished ROADMAP work. The Rust preparation/evaluation phase is not covered by that external-process deadline.

Each `.logic/runs/rtl-<backend>-<revision>-<timestamp>/` contains generated source, testbench, input/expected vectors, Rust reference outputs, compiler/simulator logs, compiled image when available, and `result.json`. Compilation/runtime/protocol failures cannot become success. Unknown X/Z values count as mismatches. Missing, duplicate or out-of-order vector rows and a missing completion marker reject the run. At most 20 mismatch messages are returned per comparison; GUI log excerpts are capped at 256K characters. Tool absence and invalid inputs/revision are rejected before execution. On macOS, PATH is preferred, then `/opt/homebrew/bin` and `/usr/local/bin` are checked for Finder launches.

## Verification

Icarus Verilog 13.0 was installed from Homebrew for real execution checks:

- 1/8/32/64-bit ALUs: all 320 vectors match. Additional SUB/OR/NOT variants cover every supported operator, input masking, wraparound and full-width hexadecimal values.
- Intentionally changed emitted ADD→SUB RTL fails RTL/Rust comparison while the Rust contract remains 320/320.
- A wrong expected value reports RTL/Rust 320/320 and contract 319/320, with overall failure.
- Invalid emitted Verilog reports compilation failure; absent/duplicate/out-of-order/unknown results fail closed. Timeout and unbounded-output fixtures are terminated.
- Service revision conflict is rejected; successful execution leaves Workspace identical.
- Native GUI launched with PATH only `/usr/bin:/bin` still finds Icarus/vvp. Docked execution passes both 320/320 at r0; native node movement reaches r1 and marks the old result stale. Floating execution passes at r1, and bottom re-docking preserves the result. The 420 pt floating window was visually checked and the status text stays together.

The new GUI flow was exercised in Japanese. Windows/Linux packaging/interaction is ⭕️ [Pending]. Formal proof, sequential clocks/resets and waveforms remain unfinished.

## Verilator adapter — 2026-11-10

🟢 [Done] `verify.rtl` accepts optional `engine: "icarus" | "verilator"`. Omitting it retains the Icarus default. GUI buttons are attached to their tool rows and results display the chosen engine. The generated RTL/testbench, vector contract, strict output parser, revision guard, job serialization, 30-second process deadline and log limits are shared.

Verilator compiles with fixed `--binary --timing --top-module __las_verification_tb --Mdir obj_dir -o las_sim --build-jobs 2 -Wno-fatal design.v testbench.v`; the service executes the resulting `obj_dir/las_sim` by absolute path. Compilation warnings remain in `compile.log`; warnings alone do not fail the job. The compiled sources and executable remain with the other reproduction artifacts. The absolute run directory also makes relative workspace roots safe. See the official [Verilator argument reference](https://verilator.org/guide/latest/exe_verilator.html).

Verilator 5.054 was installed through Homebrew. Real-tool differential tests run the same 320 vectors through both engines at 1/8/32/64 bits, covering the normal ADD/AND/XOR/MUX circuit and SUB/OR/NOT variants. Both engines match Rust and the supplied expectations. Intentionally wrong generated RTL and invalid Verilog are rejected, and `verify.rtl` service dispatch selects Verilator without changing the circuit revision. The shared timeout/output-limit tests remain applicable.

The adapter compares fully driven binary inputs/outputs of validated combinational circuits. Verilator's two-state behavior is not a proof of X/Z equivalence with Icarus. Broader generated topologies, irregular widths, multiple outputs, formal and sequential verification remain ROADMAP work. C++ toolchain absence or compiler failure is an execution failure with retained logs, never a passing result.

Native macOS QA used PATH `/usr/bin:/bin`: both tools were detected. Verilator and Icarus each passed **320/320** at r0; a native node move reached r1 and marked the r0 result stale. In a 420 pt floating Results window, Verilator passed again at r1 with readable engine/revision/count labels. Bottom re-docking retained the result. The new flow was exercised in Japanese; Windows/Linux and actual screen-reader speech remain ⭕️ [Pending].

For the full service suite including external tools, use `cargo test -p studio-service --locked -- --include-ignored --test-threads=1`. In a parallel run, one recovery test temporarily failed to reacquire the owner lock while other tests spawned child processes; the isolated recovery test passed. This is consistent with transient file-lock inheritance during process startup; serial execution avoids that overlap. The ordinary workspace suite (external-tool tests ignored) passed in parallel.
