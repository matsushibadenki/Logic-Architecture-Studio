# Workspace selection

On macOS, the folder button at the left of the native toolbar opens a system folder picker. The button shows the current folder name; its tooltip contains the full path. Select the **workspace folder containing `.logic/`**, not an exported design file. Selecting another folder opens its circuit/history/settings through the existing Rust service. Selecting an empty folder initializes the standard demo circuit. The picker also allows creating a new folder.

Canceling or choosing the current canonical directory leaves the editor and remembered selection unchanged. Circuit data is not copied between projects. Switching reopens the desktop process; acknowledged edits are already durably stored. This is distinct from design-file import, which replaces the current IR with Undo support.

The same picker is available in recovery, so a damaged project need not be reset to work on another project. The damaged folder remains unchanged. If a remembered folder was removed, moved or is unavailable, startup shows a selection screen rather than creating a new workspace there. Invalid selection JSON also opens this screen.

## Selection storage and startup

UI selections are stored as schema-1 absolute paths in `workspace-selection.json` under Tauri's application configuration directory. Writes use unique temporary files, file fsync, atomic rename and directory fsync. A failed launch restores the earlier selection. `LAS_SETTINGS_ROOT` overrides only this preferences directory for isolated testing; it does not select a circuit.

Startup precedence:

1. `--workspace PATH`
2. `LAS_WORKSPACE`
3. Last folder selected through the UI
4. Existing development checkout / release application-data defaults

CLI/environment overrides do not rewrite the remembered UI choice. The launcher passes the selected absolute path directly to the exact binary and detaches stdio/process group, preserving paths with spaces and non-ASCII names. Documents, history and render state remain Rust-owned; windows do not become independent owners. The normal service owner lock and reuse behavior remain unchanged.

MCP clients continue using their configured `LAS_WORKSPACE`; they do not automatically follow GUI selection. If the desktop owned an old workspace service, that service exits during switching. Run its standalone service when external clients must keep using that old workspace.

## Verification — macOS, 2026-11-09

Isolated application preferences and fixture workspaces were used.

- Native sheet appeared with the current directory and New Folder control. Cancel preserved `AlphaCircuit r003`; reopening the same directory preserved it again.
- Selected `回路 Beta Project` (non-ASCII plus spaces) and opened `BetaCircuit r012`. A native move committed r013. After quit/relaunch without workspace overrides, the remembered directory and r013 returned.
- Selected an empty `New Workspace` folder; the service initialized `ALU32 r000` there. The original Alpha circuit remained at revision 3.
- Remembered a nonexistent `Removed Folder`; startup displayed the unavailable-workspace screen, and that directory was not recreated. English/Japanese/Simplified Chinese labels were checked. Choosing Alpha reopened its original r003.
- Opened a broken workspace; its recovery-window picker opened Beta r013 while the damaged checkpoint bytes remained unchanged. Undo/Redo reached r014/r015, the 320-vector ALU test passed, and inspector floating/re-docking worked.
- Unit tests cover selection roundtrip, path spaces, missing directories, invalid JSON/relative paths, failed-launch rollback with/without a previous selection, and absence of circuit-data creation during selection persistence. Existing service/MCP restart integration passed.

The native dialog implementation is currently macOS-only. Windows/Linux native picker and packaging QA remain ⭕️ [Pending]. User-selected backup restoration and move-log prefix salvage remain 🟠 [Next]. No new rendering performance claim is made for this change.
