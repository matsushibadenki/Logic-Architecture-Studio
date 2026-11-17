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

## Desktop MCP settings (Phase 4)

Open **Preferences → MCP**. A workspace contains one circuit document, including its module definitions. Select **Publish** for the named circuit. Reading is included with publication. Enable simulation access or write requests as needed, then select **Start**. The app starts a loopback server on an available port and creates a new Bearer token. Copy the URL and reveal the token only when configuring a local client. The publishing and scope choices are stored per workspace. The server always starts stopped when the application opens.

In the Inspector commands below, use the URL and token from Preferences. Leaving the MCP tab or closing Preferences keeps the server running. Select **Stop** to close the listener and invalidate its token and approvals. Stop before changing the policy. Exiting or switching workspaces closes the published connection. A Core operation already dispatched before stopping can finish.

For a write request, open its entry under **Write approvals**, review the exact tool arguments, and select **Approve** or **Deny**. An approved client must retry the same request within 120 seconds of the initial request. The desktop server accepts decisions only from the Preferences window. The edit runs through the same Design Core as the editor and updates the native canvas and panels. Desktop audit records are kept in the private `.logic/mcp/` directory within the workspace.

On macOS, Inspector 2.15.0 listed 45 tools and saw only the published ALU32 document. Read-only mode rejected edits, simulations, and requests for an unknown document. Two writes at revision 0 were reviewed in Preferences. The denied write returned `approval_denied` and kept revision 0. The approved retry renamed and moved the ADD node, and the native GPU canvas and inspector showed `mcp_sum` at revision 1 without a restart. Stopping closed the listener, restarting invalidated the previous token, and unpublishing disabled startup.

## Local Streamable HTTP and access control

Start Design Core for the same workspace. Create a private security directory owned by the account running the bridge. Use one independently generated 32-byte token per client. Put the original 64-character hexadecimal token in the client's secret store and put only its SHA-256 digest in the server configuration.

```sh
umask 077
mkdir -m 700 /absolute/path/to/mcp-security
# Generate each credential separately. Save the original token in the client secret store.
token=$(openssl rand -hex 32)
printf '%s' "$token" | shasum -a 256
unset token
```

Create `clients.json` in this directory with mode `0600`. Substitute each placeholder with the corresponding lowercase SHA-256 digest of the token text without a trailing newline.

```json
{
  "requests_per_minute": 120,
  "clients": [
    {"id": "reader", "token_sha256": "<reader hash>", "scopes": ["read"]},
    {"id": "editor", "token_sha256": "<editor hash>", "scopes": ["read", "session", "write"]},
    {"id": "operator", "token_sha256": "<operator hash>", "scopes": ["approve"]}
  ]
}
```

```sh
LAS_WORKSPACE=/absolute/path/to/workspace target/debug/logic-mcp \
  --http 8765 --http-security /absolute/path/to/mcp-security
```

The endpoint is `http://127.0.0.1:8765/mcp`. Use port `0` for an OS-selected port reported on stderr. The listener binds to IPv4 loopback and checks Host and Origin before authenticating every request. Legacy session IDs are bound to the client that initialized them. Restart the bridge after changing credentials or scopes. Restarting also invalidates open sessions and outstanding approvals.

- `read` grants inspection, discovery and retrieval of existing results.
- `session` grants operations that update the workspace's simulation results or selected cycle.
- `write` grants access to request approval for workspace edits and other write tools, including starting or cancelling jobs. Launch the bridge with `--allow-write` to enable these operations.
- `approve` is reserved for operator endpoints and must be the credential's only scope. Every MCP client credential must have `read`.

The limit defaults to 120 HTTP requests per minute per credential, including initialization and session traffic. Failed authentication attempts share a separate counter. A request above the limit receives HTTP 429 and `Retry-After`. Each limit is a fixed 60-second window.

### Verify with MCP Inspector

Load the client's token into `MCP_TOKEN` from its secret store and pass it in the Authorization header. Omit the header to confirm that unauthenticated connections are rejected.

```sh
npx @modelcontextprotocol/inspector@2.15.0 --cli \
  --server-url http://127.0.0.1:8765/mcp --transport http \
  --header "Authorization: Bearer $MCP_TOKEN" --protocol-era modern --method tools/list
npx @modelcontextprotocol/inspector@2.15.0 --cli \
  --server-url http://127.0.0.1:8765/mcp --transport http \
  --header "Authorization: Bearer $MCP_TOKEN" --protocol-era modern \
  --method tools/call --tool-name revision.changes_since --tool-args-json '{"since_revision":0}'
```

Use `--protocol-era legacy` to exercise the 2025-11-25 session lifecycle. A read-only credential calling a session or write tool receives the structured `permission_denied` error. [Official Inspector CLI guide](https://github.com/modelcontextprotocol/inspector/blob/main/clients/cli/README.md)

### Approve a write

Call the write tool using the editor credential. The structured `approval_required` response contains an `approval_id`. Use a distinct operator credential to view the exact arguments and approve or deny. Keep the operator credential in the operator's secret store.

```sh
base=http://127.0.0.1:8765
curl --fail -H "Authorization: Bearer $MCP_APPROVER_TOKEN" "$base/approvals/$APPROVAL_ID"
curl --fail -X POST -H "Authorization: Bearer $MCP_APPROVER_TOKEN" \
  "$base/approvals/$APPROVAL_ID/approve"
```

Use `/deny` instead of `/approve` to reject the request. After approval, retry the same tool with the same arguments and editor credential within 120 seconds of creating the approval. Changing an argument, including `expected_revision` where present, requires a fresh approval. Matching an approval consumes it before execution. Workspace revision checks still apply. At most 16 approval requests are retained in memory.

### Audit and stdio

The private security directory holds `audit.jsonl` and one rotated file, `audit.previous.jsonl`. Rotation occurs at approximately 1 MiB per file. Entries identify the client, operation, outcome, duration and applicable revision or approval ID. Tokens, request arguments and results are excluded. Approval decisions and the start of a privileged operation must be recorded before they take effect.

The stdio transport still starts without HTTP configuration and uses its existing `--allow-write` policy. To exercise stdio in Inspector, use `--method tools/list -- /absolute/path/to/target/debug/logic-mcp` with `LAS_WORKSPACE` set for the Design Core process and the bridge.

### Phase 3 verification

Inspector 2.15.0 listed 45 tools over authenticated HTTP and stdio, and read the same workspace over legacy HTTP. In an isolated workspace, a read-only credential could not run a simulation or edit, an editor could not approve its own request, and an operator-approved edit advanced revision 0 to 1. A changed argument required a separate approval, an explicit denial blocked execution, and replay required a fresh approval. The rate-limited credential received HTTP 429 with `Retry-After` while another credential could still list tools. The audit log recorded decisions and results without credential or argument contents. A stdio call retrieved the resulting revision summary. The loopback integration test also verifies rejection of foreign hosts, browser origins, invalid credentials and another client's legacy session ID.

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
