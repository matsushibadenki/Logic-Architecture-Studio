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

The new GUI flow was exercised in Japanese. Windows/Linux packaging/interaction is ⭕️ [Pending]. Waveform visualization and VCD export remain on the ROADMAP.

## Verilator adapter — 2026-11-10

🟢 [Done] `verify.rtl` accepts optional `engine: "icarus" | "verilator"`. Omitting it retains the Icarus default. GUI buttons are attached to their tool rows and results display the chosen engine. The generated RTL/testbench, vector contract, strict output parser, revision guard, job serialization, 30-second process deadline and log limits are shared.

Verilator compiles with fixed `--binary --timing --top-module __las_verification_tb --Mdir obj_dir -o las_sim --build-jobs 2 -Wno-fatal design.v testbench.v`; the service executes the resulting `obj_dir/las_sim` by absolute path. Compilation warnings remain in `compile.log`; warnings alone do not fail the job. The compiled sources and executable remain with the other reproduction artifacts. The absolute run directory also makes relative workspace roots safe. See the official [Verilator argument reference](https://verilator.org/guide/latest/exe_verilator.html).

Verilator 5.054 was installed through Homebrew. Real-tool differential tests run the same 320 vectors through both engines at 1/8/32/64 bits, covering the normal ADD/AND/XOR/MUX circuit and SUB/OR/NOT variants. Both engines match Rust and the supplied expectations. Intentionally wrong generated RTL and invalid Verilog are rejected, and `verify.rtl` service dispatch selects Verilator without changing the circuit revision. The shared timeout/output-limit tests remain applicable.

The adapter compares fully driven binary inputs/outputs of validated combinational circuits. Verilator's two-state behavior is not a proof of X/Z equivalence with Icarus. Sequential verification remains ROADMAP work. C++ toolchain absence or compiler failure is an execution failure with retained logs, never a passing result.

Native macOS QA used PATH `/usr/bin:/bin`: both tools were detected. Verilator and Icarus each passed **320/320** at r0; a native node move reached r1 and marked the r0 result stale. In a 420 pt floating Results window, Verilator passed again at r1 with readable engine/revision/count labels. Bottom re-docking retained the result. The new flow was exercised in Japanese; Windows/Linux and actual screen-reader speech remain ⭕️ [Pending].

For the full service suite including external tools, use `cargo test -p studio-service --locked -- --include-ignored --test-threads=1`. In a parallel run, one recovery test temporarily failed to reacquire the owner lock while other tests spawned child processes; the isolated recovery test passed. This is consistent with transient file-lock inheritance during process startup; serial execution avoids that overlap. The ordinary workspace suite (external-tool tests ignored) passed in parallel.

## Seeded circuit regression — 2026-11-12

Three fixed seeds generate combinational circuits with four outputs each and width pairs of 3/17, 7/33, and 31/63 bits. The fixtures exercise every supported operator, reverse node order to check output mapping, and drive all four MUX selector values. Each circuit receives 64 vectors including zero, all ones, and seeded hexadecimal inputs. Both simulators matched the Rust evaluator for all 384 simulator/vector pairs. Expected values come from Rust for these generated circuits.

Run the regression with `cargo test -p studio-service seeded_multi_output --locked -- --ignored`.

## Combinational ALU proof — 2026-11-12

With Yosys installed, use the proof icon on the Yosys row in Results → Tools. MCP clients can call `verify.formal` with `{"expected_revision": 0}`, substituting the current revision. The same revision check and job lock used for sampled RTL verification apply.

The proof checks the `result` port for every binary assignment to `a`, `b` and the 2-bit `op`. The reference uses modular addition for 0, bitwise AND for 1 and XOR for 2 or 3. Yosys flattens the generated RTL and the reference into a miter, then checks whether the mismatch signal can ever be asserted. This proof covers the built-in ALU contract and combinational binary behavior. See the [Yosys SAT command reference](https://yosyshq.readthedocs.io/projects/yosys/en/latest/cmd/index_formal.html).

The result reports `proved`, `counterexample` or `inconclusive`. It records the revision and includes hexadecimal input and output values when the solver supplies a valid witness. The SAT solver has a 25-second limit, and the process has a 30-second deadline and monitored log limit. The run directory contains the RTL, miter, proof script, log, result, and WaveJSON witness when available.

Yosys 0.70 proved the default 32-bit and a 64-bit ALU. Replacing ADD with SUB produced a witness whose output and expected value were independently checked using wrapping 64-bit arithmetic. A stale request revision was rejected.

The macOS release app displayed a proof at revision 0, marked it stale after changing ADD to SUB, and displayed the resulting counterexample at revision 1. The counterexample survived floating and re-docking the Results panel. The existing synthesis button also ran Yosys 0.70 successfully and wrote a JSON netlist.


## Sequential circuits

Add a Register node and connect its width-matched `d` input. All registers sample together on the rising edge of `las_clk`. Asserting `las_reset` at that edge sets every register to zero. Both signals are shared top-level ports in exported Verilog. Feedback through a register is supported. Validation still rejects combinational cycles.

In Results → Signal Evaluation → Manual input, enter input values and press Reset to start a trace. The clock button adds one rising edge using the current inputs. The table shows register and output values after each edge. An edit makes the trace stale; Rerun replays its captured stimuli against the new revision, while Reset starts a new trace. MCP clients can call `simulation.cycles` with the current `expected_revision` and 1–256 `cycles` entries. Each entry has a Boolean `reset` and an `inputs` object containing the design's input names and decimal or hexadecimal strings. The first entry asserts reset.

The 3-bit accumulator and its delayed output matched Rust and Icarus across six cycles, covering initial reset, wraparound, simultaneous register updates and a second reset. ALU SAT proofs and sampled combinational RTL verification continue to apply to combinational circuits. The sampled waveform view and VCD export are available as described below.

The macOS release app displayed the accumulator feedback on the native GPU canvas. Moving the register advanced the revision, and the reset and clock controls showed the expected samples in the Results panel.


## Sample cursor and VCD

The Rust service retains the latest trace in memory, shared by all windows. `simulation.state` accepts an optional cursor and omits unchanged payloads. `simulation.select` takes `expected_revision`, `trace_id`, zero-based `index`, and `radix` (10 or 16). Replaced traces and stale revisions cannot be selected or exported. Results survive WebView remounts, floating and redocking, but are deliberately not persisted across service restarts or included in Undo/checkpoints.

Canvas values are post-edge samples, including combinational nodes, inputs and outputs. Register **Q** is the output after the selected edge. **D** is the settled data input under that sample's inputs; it is not a prediction of the next cycle, whose inputs or reset can differ. Changed revisions remove annotations immediately. The waveform shows inputs/registers/outputs in pages of 12 signals; the cursor and radix are shared, while zoom and signal page are local presentation state.

`simulation.vcd` takes `expected_revision` and `trace_id`, returning `source` and `filename`. The GUI saves through a macOS file dialog (browser preview uses a download). The export includes all IR node signals as `n_<name>`, exact declared widths, and reset. It is a **sampled two-state trace**: sample 0 is already after the mandatory reset edge, and each sample is assigned a synthetic 1 ns interval. There is no pre-reset state, event-level clock, propagation delay, timing simulation, X or Z support. The VCD comment records these limits. Existing waveform tools support VCD; see the [Verilator tracing FAQ](https://veripool.org/guide/latest/faq.html#how-do-i-view-waveforms-aka-dumps-or-traces). Opening the exported files in GTKWave/Surfer has not been tested locally.

Limits: 1–256 cycles, at most 524,288 retained node samples, 4 MiB of serialized stimuli/results, and 12 MiB VCD source. All node values stay in Rust; IPC sends only the selected node values and compact table results, not every node at every cycle or frame data. Rejected runs preserve the previous trace. Inputs outside the declared signed/unsigned port range are rejected; add/sub arithmetic intentionally wraps modulo 2^width, matching the generated HDL. Explicit slice/concat and zero/sign-extension nodes are available below; X/Z remains separate roadmap work.

Validation (2026-11-12): Rust unit tests cover sample selection, replacement/revision guards, overflow/reset, VCD values and 64-bit precision. Real Icarus checks compare the generated accumulator/pipeline RTL; an independent VCD reader also matched all 5 node values across 6 cycles (reset, wrap and reset reassertion) against Icarus executing the generated Verilog. Playwright at 1440×960 and 720×540 checks selection, radix, waveform, VCD download, second-window state and stale/rerun behavior with no console errors. Native macOS QA checks GPU values, floating/redocking, splitter resizing, native VCD save, English/Japanese/Simplified Chinese labels and value invalidation after node movement. A native feedback reconnection mismatch discovered during QA was corrected so preview cycle analysis stops at clocked boundaries; purely combinational cycles remain rejected.


## Saved stimulus tables

Results → Signal Evaluation → Stimulus table edits 1–256 cycles. Use the toolbar to add, duplicate or delete rows, save the draft, and run it. Each row supplies every top-level input, a reset flag and optional expected top-level outputs. Blank expectations are unchecked. Sequential circuits require reset in the first row; subsequent resets may be asserted independently. A failed check shows expected/actual values and selects that cycle in the shared waveform/canvas cursor. The waveform scrolls horizontally to keep the selected cycle visible in narrow panels.

Saved tables belong to the Rust Workspace and are restored after restart; simulation results remain transient. Saving changes the table version but not the circuit revision or Undo history. An outdated draft cannot overwrite a newer table or a changed circuit. A changed table marks its existing results stale; Rerun uses the current saved table. Input/output renames require updating the table (or creating/importing a new one). Input-table/manual-mode switching preserves an uncommitted draft within the mounted evaluation panel.

CSV uses UTF-8 (optional BOM), comma-separated fields, optional standard quoting and LF or CRLF line endings. `reset` and every `in.<input-name>` column are required. `cycle` is optional; when present it must contain sequential numbers starting at 1. `expect.<output-name>` columns are optional and empty cells skip checks. Unknown or duplicate columns, missing inputs, wrong field counts and out-of-width values reject the entire import without changing the saved table. Reset accepts `0`, `1`, `false` or `true`. CSV source and serialized table each have a 256 KiB limit.

Example for a 3-bit accumulator with input `inc` and output `result`:

```csv
cycle,reset,in.inc,expect.result
1,1,3,0
2,0,3,3
3,0,3,6
4,0,3,1
5,1,7,0
6,0,2,2
```

Values are typed signed/unsigned decimal or `0x` raw hexadecimal strings, preserving exact 64-bit precision. Expectations compare numeric equality, so `0xf` equals `15`. Combinational tables evaluate each row independently; sequential tables use post-edge values and the shared synchronous reset described above. These runs use the Rust two-state evaluator, and their VCD export has the existing sampled-trace limits. External execution of saved tables is available below. Arbitrary assertion expressions remain roadmap work.

GUI and MCP share `testbench.set` (`rows`), `testbench.import_csv` (`source`), `testbench.export_csv` and `simulation.testbench`. All require the current `expected_revision` and `expected_version`; read the latter from snapshot `testbench.version`. Each row has `reset`, `inputs` and optional `expected` maps of signal names to numeric strings. Table writes require MCP write authorization; evaluation/export remain available in read-only mode. Node moves share the table Arc and do not copy its rows or resend them to the native renderer.

Validation (2026-11-15): automated tests cover reset/reassertion/wraparound, mismatches, unchecked outputs, combinational u64 precision, CSV round trips and malformed/oversized input, atomic revision/version conflicts, legacy workspace defaults, persistence/restart, delta structural sharing and preserving a previous trace after rejected runs. macOS release QA imports the six-row CSV above with an intentionally wrong fourth expectation, observes 5/6 passing, selects the failed cycle, corrects/saves it, and reruns to 6/6. Native CSV export preserves all six corrected rows; Results floating and redocking retain the table and trace. Restart restores the six saved rows and rerunning passes 6/6. At a 420-pixel floating-panel width, selecting the first and last samples scrolls the waveform into view. A draft seventh row survives switching to Manual input and back; discarding it restores the six saved rows. Native CSV saving is implemented/tested on macOS; browser preview uses a download.


## Constant, slice and concat

All three node kinds use the existing output `width` (1–64 bits) and a typed `config` object:

| Kind | Configuration | Input ports | Verilog expression |
| --- | --- | --- | --- |
| `constant` | `{"type":"constant","value":"0xAB"}` | none | Sized unsigned literal, e.g. `8'hAB` |
| `slice` | `{"type":"slice","input_width":8,"lsb":4}` | `a`: `input_width` bits | `a[lsb + width - 1:lsb]` |
| `concat` | `{"type":"concat","high_width":4,"low_width":4}` | `hi`, `lo` with their declared widths | `{hi, lo}` |

Constants accept typed decimal or `0x` hexadecimal strings and must fit their output type. No JSON floating-point conversion is used. Slice ranges must fit the declared input width; full-width and single-bit slices are supported. Both concat inputs must be nonempty and their sum must equal the output width, at most 64. Concatenations with more than two operands can be built by chaining nodes. There is no implicit truncation, extension or mixed-sign connection.

`add_node` carries `config`; `update_node` can atomically change `config` and `width`. When changing to a different kind, supply a matching configuration (`{"type":"none"}` for existing kinds). Old nodes without `config` retain their original behavior. The inspector supplies valid initial configurations and derives concat output width automatically. Changing a connected width requires disconnecting/reconnecting affected ports or applying a compatible atomic patch. Invalid edits preserve the circuit and revision. Parameter edits participate in Undo/Redo and workspace/project persistence; the retained GPU scene updates captions and port widths even when output width and position are unchanged.

Validation (2026-11-15): core tests exercise all 2,016 nonempty high/low splits for total widths 2–64, full 64-bit slices/constants, example outputs for all 16 nibble values, configuration overflow/mismatch rejection, mixed configuration/position edits, serialization and Undo/Redo. A sequential test feeds the composed nodes into a register and checks reset and the next edge. Retained-scene tests compare parameter changes with a full scene rebuild. Real Icarus and Verilator executions match Rust and independent expectations at 2/8/32/64 bits, including maximum constants, full-width slices and shifted concatenations (five vectors and three outputs per width/engine).

macOS release QA adds all three kinds from the library, connects concat to output with a native drag, and evaluates the example to `0xA3` with a passing expectation. Changing the constant to `0xCD` yields `0xC3`; changing the slice LSB to zero yields `0xD3`. GPU values/captions update and old results are marked stale. A constant value of 256 at 8 bits, a slice input width of 7 connected to 8 bits, and concat hi width 5 connected to 4 bits are rejected without committing. Floating/redocking and restart preserve saved configuration/table state; rerunning after restart passes the `0xD3` expectation. Inspector labels were checked at the 300-pixel dock width and adjusted to avoid mid-word wrapping.


## Zero and sign extension

Nodes `zero_extend` and `sign_extend` each have input port `a` and output port `y`. Their `config` is respectively `{"type":"zero_extend","input_width":4}` or `{"type":"sign_extend","input_width":4}`; the existing node `width` specifies output width. Validation requires `1 ≤ input_width ≤ width ≤ 64`, exact connection widths and a configuration matching the kind. Equal-width operations pass the bits through; narrowing requires a slice. Parameter edits use the existing atomic patch, history, persistence and retained-renderer paths.

Zero extension preserves the unsigned number. Sign extension treats the input MSB as the sign of a two’s-complement number and replicates it into the added upper bits. One-bit signed `1` therefore extends to all ones (−1). At 64 bits, values and signed annotations remain exact: the minimum signed value is −9223372036854775808. Internal simulation and VCD keep raw u64 bit patterns. DEC annotations, tables and waveforms use each node’s explicit signed type; HEX always shows the exact bit pattern. Legacy nodes default to unsigned.

Generated Verilog explicitly fills the added bits, avoiding dependence on expression signedness or implicit assignment resizing. For 4 → 8 bits:

```verilog
assign n_zero = {{4{1'b0}}, n_data};
assign n_signed_bits = {{4{n_data[3]}}, n_data};
```

Equal widths generate a direct assignment, avoiding zero-length replication. Wire declarations include `signed` for signed nodes. X/Z remains separate roadmap work.

Validation (2026-11-15): core tests cover all 2,080 input/output width pairs from 1 through 64, zero/positive/sign-boundary/negative values and an independent i128 mathematical reference; narrowing, incompatible configurations, width overflow, connected width mismatches and position-plus-configuration patches are rejected atomically. Tests also cover Undo/Redo, serialization, register/reset behavior, equal-width Verilog, signed DEC versus raw HEX annotations, unchanged VCD bits and incremental native port/label updates. Icarus and Verilator both match independent expectations for 1→1, 1→64, 4→8, 8→16, 31→32, 32→64, 63→64 and 64→64 (five vectors, two outputs for each engine/pair).

Native macOS QA adds both nodes from the library, connects SX to the output by dragging, and rejects output narrowing and an input width that mismatches its driver. Importing the example CSV gives 8/8 passing checks. In the original extension-only version, selecting input 8 displayed ZX=8 and local SX=−8. With explicit signed types, existing unsigned SX nodes now display 248; changing SX output type to signed propagates to sign_out and displays −8 consistently. HEX remains 0x08/0xF8. Results floating/redocking preserves selection and checks. The inspector shows the two’s-complement rule and signed port markers. After restarting the release app, node configurations and the four-row input table restore and rerunning again passes 8/8 checks.


## Circuit-wide signed types (2026-11-15)

Every node has optional `signed: boolean` (default false for legacy documents). Data inputs and output use that signedness; a MUX selector is always unsigned 2-bit. Normal connections require exact width and signedness. Both extension nodes accept either input signedness with an exact input width: ZX fills zero and SX replicates MSB regardless of the input type. Output type is explicit. Equal-width extensions are available as bit-preserving casts. Slice and concat operate on bits but require their inputs to match the node’s declared signedness.

`design.set_signed {expected_revision, id, signed}` traverses connected data constraints in both directions, including fan-out and register feedback, and commits one undoable transaction. It stops at extension inputs and selector connections. `update_node.signed` is also available for explicit atomic patches and never silently propagates. A signed selector source, incompatible constant, malformed type or mixed-sign edge rejects the entire transaction. Changed types invalidate prior results and are persisted with normal workspace/history state.

Decimal strings are mathematical values: signed width W accepts −2^(W−1)…2^(W−1)−1; unsigned accepts 0…2^W−1. `0x` strings are raw W-bit patterns, so signed 8-bit `-1` and `0xFF` compare equal. Parsing and formatting retain the exact signed minimum at 64 bits. Inputs, constants, manual evaluation, cycle simulation, saved CSV expectations and external RTL vectors share this rule. Negative values are formatted consistently for DEC; HEX and sampled VCD preserve bits. Arithmetic wraps modulo 2^W. Generated input/output/wire/reg declarations use `signed`; slice/concat/extension expressions stay explicit.

Validation: all 1–64-bit numeric boundaries; strict mixed-sign rejection and native cable preview; propagation, selector/conversion boundaries, Undo/Redo and legacy serialization; typed CSV/expectation checks; register feedback/reset and Q/D annotations; retained renderer type updates; exact frontend DEC/HEX formatting. Real Icarus and Verilator runs cover ADD, SUB, AND, OR, XOR, NOT and MUX at 1, 4, 8, 32 and 64 bits, with 15 independent mathematical vectors per case (1,050 engine/vector comparisons). A real Icarus run also checks signed register feedback, pipeline delay and reset against independent expected values.


Native macOS release QA: change an unsigned 8-bit adder’s signal type in the inspector and observe all connected data nodes/ports become signed; import `examples/signed-stimuli.csv` and pass all 4 expectations, including 127 + 1 = −128 and 0xFF = −1; select a sample and observe DEC/HEX annotations and waveform output; reject signed 8-bit input 128 while preserving the saved table; disconnect/reconnect the signed result with native port dragging; float/redock results with the same cursor; restart and rerun the restored table (4/4 pass). The signal-type select layout and waveform tooltip were corrected during QA. X/Z and external viewers retain their existing roadmap limits.


## Saved stimulus tables on Icarus / Verilator

`simulation.rtl_testbench {expected_revision, expected_version, engine: "icarus" | "verilator"}` executes the saved table against internally generated RTL. It publishes an ephemeral shared trace through `simulation.state`, with `external` metadata: engine, total/passed node comparisons, success, up to 100 detailed mismatches, and artifact directory. `checks` separately compares actual RTL outputs to optional saved expectations. Counts describe node samples and output assertions, respectively; the source badge distinguishes Rust from either RTL simulator. All arithmetic, signed decimal/hex parsing and widths follow the circuit type rules above.

Inputs/reset are applied at 10 ns intervals. Sequential testbenches begin with clock low, raise it at +5 ns, sample all nodes at +6 ns (after nonblocking assignments and combinational settling), then lower it at +10 ns. Registers require reset on row 1; later reset rows are supported. Combinational rows also sample at +6 ns. VCD is produced by `$dumpvars`/Verilator tracing at 1 ps precision, including actual event transitions between samples and potentially pre-reset unknown values with Icarus. It is not a physical propagation-delay/timing analysis. The GUI table/waveform remains a sampled view. X/Z at comparison points causes an error instead of silently converting to zero. Incomplete, duplicate, out-of-order or oversized sample streams are rejected.

`simulation.select` operates on actual RTL observations (including registers and internal node annotation). `simulation.vcd` and the native Save VCD dialog return the original simulator VCD; regular state polls never transfer VCD data. A Rust rerun restores the existing sampled Rust VCD behavior. Rerun from a stale external trace retains its engine. External jobs use the same single-job lock as RTL verification, fixed command arguments, a 30 s compile+run deadline, a 4 MiB process-log limit and 12 MiB VCD limit. Process groups are terminated on bounds violations. At most 131,072 node samples and 4 MiB serialized trace data are accepted. Large traces stay in Rust, with no per-frame VCD transfer.

The service snapshots the revision, stimulus version and simulation generation before releasing the workspace lock for the tool run. All three must still match before publication; a changed design/table/newer trace or cursor rejects publication and preserves current results. Invalid input, missing tools, compile/runtime failures and X/Z also preserve the previous trace. Expectation or Rust mismatches publish a failed result with inspectable actual values and exportable VCD. Runs write generated RTL, testbench, saved stimulus JSON, Rust reference, simulator logs, VCD and result/error artifacts below `.logic/runs/stimuli-*`. Results are session state, while the stimulus table remains durable; artifact files persist for debugging. The existing explicit MCP write-enabled gate applies to external tool execution, as with `verify.rtl`.

Automated validation: both installed engines run the signed adder and signed feedback/pipeline examples, plus independent 1-bit and 64-bit boundary/reset vectors. Assertions check actual values, expectation counts, Rust agreement, signal-bearing VCD and rising-edge timestamps. Mutated RTL, wrong expectations, malformed/unknown output streams and compile failures are rejected or reported independently. A real Verilator service test edits the table during compilation and verifies stale-run rejection with the prior trace retained, then reruns the current version successfully. VCD exported through the session is byte-identical to the generated trace.


Native macOS QA (2026-11-15): opened the saved seven-row signed pipeline table, selected Icarus and Verilator in turn, and obtained 42/42 node comparisons plus 14/14 expectations from each. Sample 3 shows acc=127, delay/result=−128; selecting the cycle updates the native canvas. Changed the negative result expectation from −128 to −127, saved and reran with Verilator: Rust remained 42/42 while expectations became 13/14, and the failure button selected sample 3. Restoring the expectation and rerunning returns 14/14. Both native VCD exports were compared byte-for-byte with their run artifacts; clock/reset signals and event timestamps are present (Icarus: 1,081 bytes; Verilator: 1,026 bytes for this example).
