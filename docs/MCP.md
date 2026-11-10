# MCP connection

Build the bridge with `cargo build -p studio-service --bin logic-mcp`. Start the desktop app or `npm run core` for the same workspace first. The bridge never edits project files directly and does not start a separate design authority.

Configure an MCP client's stdio server with an absolute binary and workspace path:

```json
{
  "mcpServers": {
    "logic-studio": {
      "command": "/absolute/path/Logic-Architecture-Studio/target/debug/logic-mcp",
      "args": [],
      "env": { "LAS_WORKSPACE": "/absolute/path/Logic-Architecture-Studio" }
    }
  }
}
```

`args: ["--allow-write"]` enables revision-checked editing, testing, experiments and synthesis. No arbitrary shell or file-write tool is exposed. Approving the flag is a host configuration choice; the app does not grant it automatically.

This implementation deliberately targets the legacy **2025-11-25** JSON-RPC stdio lifecycle (`initialize`, `notifications/initialized`, `tools/list`, `tools/call`) and advertises that version. It does not claim the newer 2026-07-28 discovery lifecycle, Tasks, Streamable HTTP or rmcp SDK integration. [Official legacy specification](https://modelcontextprotocol.io/specification/2025-11-25).

Tools: `project.inspect`, `design.get_ir`, `design.propose_patch`, `design.apply_patch`, `design.undo`, `design.redo`, `layout.auto_arrange`, `view.focus_module`, `rtl.generate`, `simulation.evaluate`, `verify.run_tests`, `revision.diff`, `experiment.create`, `experiment.run`, `experiment.compare`, `experiment.get_status`, `tools.status`, `synthesis.run`.

Resource: `logic://workspace/ir`. Prompt: `explore_architecture`.

The self-contained tool schemas define a deliberately small JSON Schema vocabulary; the server validates every keyword it emits and then deserializes typed Rust operations and validates graph semantics. It does not accept arbitrary external schemas or resolve remote references.

## Patch example

Inspect revision first. Preview a rename with `design.propose_patch`:

```json
{
  "expected_revision": 0,
  "label": "Rename adder",
  "operations": [{ "op": "update_node", "id": "add", "name": "sum" }]
}
```

Pass the same patch to `design.apply_patch`. A changed revision produces a conflict, with no mutation. Refresh and review a new proposal instead of blindly retrying it. Supported operations are `add_node`, `remove_node`, `update_node`, `connect`, and `disconnect`.

## Explicit verification vectors

```json
{
  "vectors": [{
    "inputs": { "a": "3", "b": "5", "op": "0" },
    "expected": { "result": "8" }
  }]
}
```

Numbers are unsigned decimal or `0x` hexadecimal strings to preserve all 64 bits. Every output must have an expectation. Omitting vectors selects the built-in ALU contract, not a proof of arbitrary circuit correctness.
