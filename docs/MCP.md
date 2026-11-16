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

The stdio bridge uses rmcp. Clients using **2026-07-28** discover the server with `server/discover` and include protocol version, client identity and client capabilities in request `_meta`. Clients using **2025-11-25** can connect with `initialize` and `notifications/initialized`. Both paths use the same workspace, tools, resource and prompt. [Official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)

Tool successes expose the payload in `structuredContent.result`. Tool failures set `isError: true` and return `structuredContent.error` with a stable `code` and a human-readable `message`. The codes are `invalid_arguments`, `write_disabled`, `revision_conflict`, `app_not_connected` and `tool_failed`. For unknown tool, resource and prompt names, JSON-RPC errors also carry a code in `error.data.code`.

Use the read-only `revision.changes_since` tool with `{"since_revision":42}` to retrieve edit summaries in revision order. Each summary contains the revision, label and actor. The response also includes the current revision. If `resync_required` is true, call `project.inspect` and resume polling from the returned revision. The server retains the latest 100 summaries. After switching workspaces or restoring a checkpoint, start from a fresh snapshot.

Tools: `project.inspect`, `design.get_ir`, `design.propose_patch`, `design.apply_patch`, `design.undo`, `design.redo`, `layout.auto_arrange`, `view.focus_module`, `rtl.generate`, `simulation.evaluate`, `verify.run_tests`, `revision.diff`, `revision.changes_since`, `experiment.create`, `experiment.run`, `experiment.compare`, `experiment.get_status`, `tools.status`, `synthesis.run`.

Resource: `logic://workspace/ir`. Prompt: `explore_architecture`.

The self-contained tool schemas define a deliberately small JSON Schema vocabulary; the server validates every keyword it emits and then deserializes typed Rust operations and validates graph semantics. It does not accept arbitrary external schemas or resolve remote references.

## Phase 1 adapter and sample

The transport-neutral `mcp-core` crate defines the `AppAdapter` contract, registers operations, validates inputs and enforces the write flag. The workspace adapter calls the existing Design Core command service. `core.get_app_info`, `core.get_capabilities`, `core.get_context`, `core.list_documents` and `core.get_operation_help` provide discovery and context. `core.get_document_state` accepts a listed document ID, a zero-based `offset` and a `limit` of 1–100. The workspace response pages nodes and edges at that offset. Each collection reports its total and next offset, and the response identifies the revision used.

Launch the client with `args: ["--sample"]` to try the independent in-memory adapter. It starts without a workspace or Design Core and exposes `sample.list_objects` plus the common discovery tools. Add `--allow-write` to create, rename or delete sample objects using `expected_revision`. Deletion additionally requires `confirm: true`. Each process starts with a fresh sample document. The tests exercise both the standalone sample and the real workspace through stdio.

## Verification and synthesis jobs

Call `job.start` with a tool name and its usual arguments. For example, `{"method":"verify.rtl","args":{"expected_revision":42,"engine":"verilator"}}`. Supported tools are `verify.rtl`, `verify.formal`, `simulation.rtl_testbench` and `synthesis.run`. Starting and cancelling jobs requires `--allow-write`. Reading status works on a read-only connection to the same workspace.

Use the returned `id` with `job.status` or `job.cancel`. The status transitions from `running` to `completed` or `failed`. After cancellation, poll through `cancelling` until `cancelled` before starting another job. `completed` includes the tool's usual `result`, which may report a failed verification. The workspace runs one external job at a time. Its service keeps the latest 16 job records in memory, and restarting the service clears them. The underlying tool methods remain available for synchronous calls.

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
