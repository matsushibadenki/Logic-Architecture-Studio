# Workspace recovery

The desktop app opens a recovery window if its workspace cannot load or panel layout validation fails. The recovery window does not start the editor renderer or normal WebView polling. It supports English, Japanese and Simplified Chinese.

Choose a validated restore point and select **Back up & restore**. After the archive path is displayed, **Open recovered workspace** opens the same absolute workspace with the same executable. Refresh rechecks candidates; Retry opening changes no files. Selection is explicit and no candidate is chosen automatically.

| Failure | Available action | Effect |
|---|---|---|
| Damaged move journal, valid current checkpoint | Restore latest checkpoint | Restore its circuit/history/settings; preserve the original journal in the archive and start an empty active journal |
| Damaged current checkpoint, valid previous checkpoint | Restore previous checkpoint | Restore that earlier state; archive current/previous/journal first |
| Damaged `panel-layout.json` | Reset panel layout | Restore default dimensions, dock-restore homes and floating frames; preserve circuit, journal and canonical panel modes |
| No valid checkpoint | Choose a checkpoint backup, refresh, or open another workspace | No automatic reset or overwrite |
| Another service owns the workspace | Retry after that owner exits | Recovery cannot acquire the lock and does not replace files |

The info icon explains the restoration boundary. **Checkpoint-only restoration does not replay edits after the chosen checkpoint.** Their original journal is preserved for later/manual recovery. Valid-prefix journal salvage is available when the current checkpoint and at least one following move are valid. The native workspace-folder picker is available from the recovery window to open another project without rewriting the damaged source; see [WORKSPACES.md](WORKSPACES.md).

## Stored files and durability

Normal checkpoint saves retain one prior checkpoint as `.logic/workspace.previous.json`. A hard link preserves the immutable old inode without copying its history on supported filesystems; a synced-copy fallback handles unavailable hard links. Move-only WAL appends do not rotate the previous checkpoint. This supplements regular backups and does not replace them.

Before recovery replaces anything, existing `workspace.json`, `workspace.previous.json`, `workspace.wal`, `panel-layout.json` and `workspace.tmp` are copied to `.logic/recovery/<unique-id>/`. A manifest records the choice, source fingerprint and timestamp. The archive files and directories are synced before replacements. The owner lock prevents a live Design Core from racing recovery; SHA-256 content tokens reject stale previews and detect source changes during backup.

Circuit recovery installs the checkpoint first and clears the active journal second, with explicit sync boundaries. A crash or I/O failure between these steps can leave startup requiring recovery again; the original archive remains. There is no hardware power-loss certification or guarantee of recovering arbitrary corruption. Preserve the entire `.logic` directory when backing up, including the journal and recovery archive. Existing projects gain a previous checkpoint at their next successful checkpoint save.

Reopening uses an absolute `--workspace PATH` argument (paths with spaces are supported), ahead of `LAS_WORKSPACE`. The child process uses the exact executable and detached stdio/process group. This was necessary on macOS where normal bundle relaunch could lose the workspace environment or resolve a different installed build.

## Verification — macOS, 2026-11-09

Isolated fixture workspaces were used; user circuit files were not modified by the recovery checks.

- Bad journal checksum: current `Current_ALU`, revision 7, and previous `Prior_ALU`, revision 0, appeared. Restoring current archived the original corrupt journal and reopened **Current_ALU r007**. Native move→Undo reached r8→r9; the 320-vector ALU check passed. Inspector floating/re-docking worked afterward.
- Bad current checkpoint: only the valid previous candidate appeared. Recovery reopened **Prior_ALU r000**, preserving the literal corrupted checkpoint in its archive.
- Bad layout: only layout reset appeared. After reset, the circuit reopened at **r001**, including a move to x=555 replayed from its unchanged journal. The active circuit checkpoint and journal matched their archived bytes exactly.
- Both checkpoints invalid: no candidate appeared; restore stayed disabled and original files remained unchanged.
- Recovery labels and selection were checked in English/Japanese/Simplified Chinese. The missing-backup message uses sentence boundaries for wrapping.
- Unit tests cover exact original backups, checkpoint/previous restoration, persisted Undo, layout-only journal preservation, active-owner rejection, stale content rejection, backup-write failure, and workspace arguments containing spaces/missing paths. Existing WAL compaction/checksum/replay, atomic-write rollback and IPC/MCP tests remain applicable.

Windows/Linux packaging and native recovery interaction are ⭕️ [Pending]. The fallback-copy path and physical power-loss behavior have not been measured on additional filesystems. Recovery after the first damaged frame is 🔴 [Later]; no attempt is made to skip corrupt records.

## Selected checkpoint backups — 2026-11-10

🟢 [Done] **Choose backup** opens the macOS native JSON file picker. It accepts a full Workspace checkpoint (`workspace.json`, `workspace.previous.json`, or a copy), validates the circuit, Undo/Redo and experiment candidates, and previews its name, revision, node count and path. Design-only `.logic` exports and folders are not checkpoint backups. The selected checkpoint is restored without replaying its source journal.

Rust retains the validated bytes; JavaScript receives only preview metadata. The source is checked again before archiving and before replacement. A changed source or destination requires a new selection/refresh. Cancellation preserves the existing choice. The selected file is never modified. The archive manifest records the external source path and SHA-256 alongside the exact original target files.

An isolated native app fixture with both local checkpoints and its journal corrupt was tested: picker cancellation, invalid JSON rejection, metadata preview, changed-source rejection, restore and same-workspace restart. It reopened **RestoredCircuit r023** with `saved_sum`; Undo restored **PriorCircuit r024**, Redo restored **RestoredCircuit r025**, and all **320/320** ALU vectors passed. Tests also reject invalid graphs and changed targets. English, Japanese and Simplified Chinese labels are provided; this new picker flow was exercised in Japanese. Windows/Linux interaction remains ⭕️ [Pending].

## Valid-prefix move salvage — 2026-11-10

🟢 [Done] Recovery offers **Recover valid moves** alongside the original checkpoint when a valid current checkpoint has at least one replayable new move followed by corruption or an incomplete tail. The preview shows the resulting revision, recovered move count and excluded byte count. Its tooltip gives the stopping reason. No option is preselected. English, Japanese and Simplified Chinese labels are provided.

Startup and recovery share the same frame decoder: length (1 MiB bound), SHA-256, JSON/schema, position-only patch, sequence and expected revision checks. Replay stops at the first invalid frame; bytes after it are never searched or applied. A failed record is applied to a temporary state so partial mutations cannot enter the recovered checkpoint. The previous checkpoint is not used as a salvage base because the retained journal may not span its history. An invalid current checkpoint or zero recoverable new moves offers no salvage candidate.

Preview reads without trimming the original log. Restore revalidates under the owner lock and existing content token, archives exact originals, writes the replayed Workspace including Undo/Redo and journal sequence, and then empties the active journal. The manifest records recovered moves, valid/excluded bytes and stopping reason. Interrupted replacement uses the existing conservative restart behavior and durable archive. Normal startup still trims only incomplete tails and rejects complete corrupt records.

Unit tests cover checksum, length, JSON, schema, sequence gap, revision conflict, missing node and incomplete tail; a later valid frame is never recovered after a bad frame. Native macOS QA used two valid moves (x=500 then x=550) followed by a bad checksum: preview **r2 / 2 moves / 197 excluded bytes**, restore/restart **SalvageCircuit r002**, Undo/Redo **r003/r004**, **320/320** ALU vectors and inspector floating/re-docking. Archived journal bytes matched the original fixture exactly. The new UI flow was exercised in Japanese; other platforms remain ⭕️ [Pending].
