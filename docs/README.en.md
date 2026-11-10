# Logic Architecture Studio

A working Tauri 2 / Rust EDA foundation with a shared command service for the GUI and a stdio MCP bridge. UI languages: English, Japanese, Simplified Chinese.

Run `npm ci`, then `npm run desktop`. For browser development, run `npm run core` and `npm run dev` in separate terminals. Use `LAS_WORKSPACE` to select the directory. macOS is the current development target; IPC uses Unix sockets.

Implemented: 1–64-bit combinational circuit editing, atomic patches, revision checks, persistent Undo/Redo, Verilog generation, signal evaluation, 320-vector independent ALU tests, isolated candidate comparisons, docking/floating panel code, and UNGE-based rich nodes/native GPU rendering. The desktop main editor uses Rust-owned native GPU rendering and gestures, with dockable/floating WebView panels; browser development retains SVG. Large frame buffers never cross the JavaScript bridge.

The default ALU contract is `op=0: ADD`, `1: AND`, `2/3: XOR`. The built-in experiment intentionally mutates ADD→SUB and XOR→OR to demonstrate rejection of incorrect candidates. This is behavioral testing, not formal verification or measured architecture optimization.

Yosys is detected and invoked only when installed. No cell area, delay or power figures are invented. Internal LLM orchestration, MCP client, formal tools, OpenROAD and fabrication flows are unfinished.

See [roadmap](../ROADMAP.md), [architecture](ARCHITECTURE.md), [MCP setup](MCP.md), and [UNGE attribution](../THIRD_PARTY_NOTICES.md). Run `npm run build`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`. GPU and socket integration tests are explicitly marked ignored and can be selected with `-- --ignored`.

Native and WebView sync transfer changed entities. Position commits append/fsync `.logic/workspace.wal`, periodically compacting into `.logic/workspace.json`. Restart replays moves and Undo/Redo; filesystem backups need both files. Use application export for the current circuit. See [2,000-node live measurements](NATIVE-PERFORMANCE.md).

Current/history/candidate circuits share immutable Arc snapshots; checkpoints use buffered output with checked flush and fsync. See [140 sequential moves and checkpoint measurements](HISTORY-PERFORMANCE.md).

Indexed moves copy/compare changed 32-entity blocks and retain unchanged circuit data; stale post-commit frames are suppressed. See [normal move latency](SPARSE-MOVE-PERFORMANCE.md).

Startup recovery previews valid current/previous checkpoints or resets only damaged panel layout. Original files are archived before explicit restoration; circuit restore excludes later journal moves, while layout reset preserves them. See [recovery behavior and verification](RECOVERY.md).

The macOS folder button opens another workspace and remembers the selection. Missing remembered folders open a selection screen rather than creating replacement data. See [workspace selection](WORKSPACES.md).

Generated RTL can be tested in Icarus or Verilator against Rust and an independent vector contract, from Toolchain or MCP `verify.rtl`. See [RTL verification](RTL-VERIFICATION.md).
