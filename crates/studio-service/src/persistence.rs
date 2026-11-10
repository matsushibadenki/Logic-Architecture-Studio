//! Durable move journal with atomic full checkpoints. Acknowledgments wait for
//! fsync; a checkpoint sequence makes replay safe if a crash precedes log pruning.
use design_core::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
};

const MAX_RECORD: usize = 1_048_576;
const CHECKPOINT_RECORDS: usize = 128;
const CHECKPOINT_BYTES: u64 = 4 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
pub(crate) struct Checkpoint {
    #[serde(flatten)]
    pub(crate) workspace: Workspace,
    #[serde(default)]
    pub(crate) journal_sequence: u64,
}
#[derive(Serialize, Deserialize)]
struct MoveRecord {
    schema: u32,
    sequence: u64,
    args: Value,
    actor: String,
}
/// Shared strict decoder: recovery stops at the first bad frame and never scans
/// past it. Normal startup accepts only an incomplete final frame.
pub(crate) struct Replay {
    pub offset: usize,
    pub sequence: u64,
    pub records: usize,
    pub error: Option<String>,
}
pub(crate) fn replay_prefix(state: &mut Workspace, base: u64, bytes: &[u8]) -> Replay {
    state.init_history_budget();
    let mut result = Replay {
        offset: 0,
        sequence: base,
        records: 0,
        error: None,
    };
    while result.offset < bytes.len() {
        let offset = result.offset;
        let decode = || -> Result<Option<(MoveRecord, usize)>, String> {
            if bytes.len() - offset < 36 {
                return Ok(None);
            }
            let size = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            if size > MAX_RECORD {
                return Err("Move journal has invalid length; file preserved".into());
            }
            if bytes.len() - offset - 36 < size {
                return Ok(None);
            }
            let payload = &bytes[offset + 36..offset + 36 + size];
            if Sha256::digest(payload).as_slice() != &bytes[offset + 4..offset + 36] {
                return Err("Move journal checksum mismatch; file preserved".into());
            }
            let record: MoveRecord = serde_json::from_slice(payload)
                .map_err(|e| format!("Move journal preserved: {e}"))?;
            if record.schema != 1 || !design_core::is_position_patch(&record.args) {
                return Err("Unsupported move journal record; file preserved".into());
            }
            Ok(Some((record, size + 36)))
        };
        let mut apply = || -> Result<Option<usize>, String> {
            let Some((record, length)) = decode()? else {
                return Ok(None);
            };
            if record.sequence > base {
                if record.sequence
                    != result
                        .sequence
                        .checked_add(1)
                        .ok_or("Move sequence overflow")?
                {
                    return Err("Move journal sequence gap; file preserved".into());
                }
                // A rejected record must not leak partially applied state into
                // the preview or the replacement checkpoint.
                let mut next = state.clone();
                next.dispatch_view("design.apply_patch", record.args, &record.actor)
                    .map_err(|e| format!("Move journal replay failed; preserved: {e}"))?;
                *state = next;
                result.sequence = record.sequence;
                result.records += 1;
            }
            Ok(Some(length))
        };
        match apply() {
            Ok(Some(length)) => result.offset += length,
            Ok(None) => break,
            Err(error) => {
                result.error = Some(error);
                break;
            }
        }
    }
    result
}
pub struct Persistence {
    root: PathBuf,
    sequence: u64,
    records: usize,
    bytes: u64,
    poisoned: bool,
}
impl Persistence {
    pub fn open(root: &Path) -> Result<(Self, Workspace), String> {
        let path = root.join(".logic/workspace.json");
        let (mut state, sequence) = if path.exists() {
            let checkpoint: Checkpoint =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("Cannot open workspace; file preserved: {e}"))?;
            (checkpoint.workspace, checkpoint.journal_sequence)
        } else {
            (Workspace::default(), 0)
        };
        if state.schema != 1 {
            return Err("Unsupported workspace schema".into());
        }
        state.document.validate(false)?;
        state.init_history_budget();
        let mut store = Self {
            root: root.into(),
            sequence,
            records: 0,
            bytes: 0,
            poisoned: false,
        };
        if !path.exists() {
            store.checkpoint(&state, sequence)?;
        }
        let wal = store.wal();
        let existed = wal.exists();
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&wal)
            .map_err(|e| e.to_string())?;
        if !existed {
            file.sync_all().map_err(|e| e.to_string())?;
            File::open(root.join(".logic"))
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let replay = replay_prefix(&mut state, sequence, &bytes);
        if let Some(error) = replay.error {
            return Err(error);
        }
        let offset = replay.offset;
        store.sequence = replay.sequence;
        store.records = replay.records;
        // Only an incomplete final frame is uncommitted. Never silently discard
        // a complete corrupt record, including records covered by a checkpoint.
        if offset < bytes.len() {
            file.set_len(offset as u64)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
        }
        store.bytes = offset as u64;
        Ok((store, state))
    }
    fn wal(&self) -> PathBuf {
        self.root.join(".logic/workspace.wal")
    }
    fn checkpoint(&mut self, state: &Workspace, sequence: u64) -> Result<(), String> {
        let temp = self.root.join(".logic/workspace.tmp");
        let file = File::create(&temp).map_err(|e| e.to_string())?;
        let mut writer = BufWriter::with_capacity(1024 * 1024, file);
        // Stream compact JSON directly instead of allocating a pretty full copy.
        #[derive(Serialize)]
        struct Borrowed<'a> {
            #[serde(flatten)]
            workspace: &'a Workspace,
            journal_sequence: u64,
        }
        serde_json::to_writer(
            &mut writer,
            &Borrowed {
                workspace: state,
                journal_sequence: sequence,
            },
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        writer.get_ref().sync_all().map_err(|e| e.to_string())?;
        let current = self.root.join(".logic/workspace.json");
        if current.exists() {
            let previous_temp = self.root.join(".logic/workspace.previous.tmp");
            if previous_temp.exists() {
                std::fs::remove_file(&previous_temp).map_err(|e| e.to_string())?;
            }
            // The checkpoint inode is immutable after rename. A hard link keeps
            // the previous known-good version without copying its history.
            if std::fs::hard_link(&current, &previous_temp).is_err() {
                std::fs::copy(&current, &previous_temp).map_err(|e| e.to_string())?;
                File::open(&previous_temp)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            std::fs::rename(
                previous_temp,
                self.root.join(".logic/workspace.previous.json"),
            )
            .map_err(|e| e.to_string())?;
        }
        std::fs::rename(temp, self.root.join(".logic/workspace.json"))
            .map_err(|e| e.to_string())?;
        // The new checkpoint must survive before removing the recovery log.
        // After rename a directory-sync failure has an uncertain disk outcome;
        // stop writes rather than acknowledging a rollback we cannot guarantee.
        if let Err(e) = File::open(self.root.join(".logic")).and_then(|f| f.sync_all()) {
            self.poisoned = true;
            return Err(format!(
                "Checkpoint directory sync failed; restart required: {e}"
            ));
        }
        Ok(())
    }
    pub fn commit(
        &mut self,
        state: &Workspace,
        method: &str,
        args: &Value,
        actor: &str,
    ) -> Result<(), String> {
        if self.poisoned {
            return Err("Persistence outcome uncertain; restart required".into());
        }
        let sequence = self.sequence + 1;
        if method == "design.apply_patch"
            && design_core::is_position_patch(args)
            && self.records < CHECKPOINT_RECORDS
            && self.bytes < CHECKPOINT_BYTES
        {
            let payload = serde_json::to_vec(&MoveRecord {
                schema: 1,
                sequence,
                args: args.clone(),
                actor: actor.into(),
            })
            .map_err(|e| e.to_string())?;
            if payload.len() > MAX_RECORD {
                return Err("Move journal record exceeds 1 MiB".into());
            }
            let mut frame = Vec::with_capacity(payload.len() + 36);
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(&Sha256::digest(&payload));
            frame.extend_from_slice(&payload);
            let mut file = OpenOptions::new()
                .read(true)
                .append(true)
                .open(self.wal())
                .map_err(|e| e.to_string())?;
            let before = file.metadata().map_err(|e| e.to_string())?.len();
            if let Err(e) = file.write_all(&frame).and_then(|_| file.sync_all()) {
                if file.set_len(before).and_then(|_| file.sync_all()).is_err() {
                    self.poisoned = true;
                    return Err(format!(
                        "Persistence outcome uncertain; restart required: {e}"
                    ));
                }
                return Err(e.to_string());
            }
            self.bytes = before + frame.len() as u64;
            self.records += 1;
        } else {
            self.checkpoint(state, sequence)?;
            // After rename the snapshot is authoritative. Failure to prune is
            // safe: replay skips all sequence numbers covered by the checkpoint.
            if let Ok(file) = OpenOptions::new().write(true).open(self.wal()) {
                if file.set_len(0).and_then(|_| file.sync_all()).is_ok() {
                    self.bytes = 0;
                }
            }
            self.records = 0;
        }
        self.sequence = sequence;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Root(PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("las-wal-{name}-{}", std::process::id()));
            std::fs::create_dir_all(root.join(".logic")).unwrap();
            Self(root)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn moved(store: &mut Persistence, old: &Workspace, x: f32) -> Workspace {
        let mut next = old.clone();
        let args = serde_json::json!({"expected_revision":old.revision,"label":"Move","operations":[{"op":"update_node","id":"add","x":x}]});
        next.dispatch_view("design.apply_patch", args.clone(), "test")
            .unwrap();
        store
            .commit(&next, "design.apply_patch", &args, "test")
            .unwrap();
        next
    }
    #[test]
    fn replay_restores_durable_moves_and_undo_redo_without_rewriting_snapshot() {
        let root = Root::new("replay");
        let (mut store, state) = Persistence::open(&root.0).unwrap();
        let checkpoint = std::fs::read(root.0.join(".logic/workspace.json")).unwrap();
        let state = moved(&mut store, &state, 450.);
        let state = moved(&mut store, &state, 470.);
        assert_eq!(
            checkpoint,
            std::fs::read(root.0.join(".logic/workspace.json")).unwrap()
        );
        let (mut reopened, mut recovered) = Persistence::open(&root.0).unwrap();
        assert_eq!(state, recovered);
        let args = serde_json::json!({"expected_revision":2});
        recovered
            .dispatch_view("design.undo", args.clone(), "test")
            .unwrap();
        reopened
            .commit(&recovered, "design.undo", &args, "test")
            .unwrap();
        let (_, mut recovered) = Persistence::open(&root.0).unwrap();
        assert_eq!(recovered.document.node("add").unwrap().x, 450.);
        recovered
            .dispatch_view(
                "design.redo",
                serde_json::json!({"expected_revision":3}),
                "test",
            )
            .unwrap();
        assert_eq!(recovered.document.node("add").unwrap().x, 470.);
    }
    #[test]
    fn incomplete_tail_is_trimmed_but_checksum_corruption_is_preserved() {
        let root = Root::new("torn");
        let (mut store, state) = Persistence::open(&root.0).unwrap();
        let state = moved(&mut store, &state, 400.);
        let path = store.wal();
        let good = std::fs::read(&path).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[1, 2, 3])
            .unwrap();
        assert_eq!(Persistence::open(&root.0).unwrap().1, state);
        assert_eq!(std::fs::read(&path).unwrap(), good);
        let mut bad = good;
        *bad.last_mut().unwrap() ^= 1;
        std::fs::write(&path, &bad).unwrap();
        assert!(Persistence::open(&root.0).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bad);
    }
    #[test]
    fn checkpoint_sequence_prevents_duplicate_replay_and_bounds_log() {
        let root = Root::new("checkpoint");
        let (mut store, mut state) = Persistence::open(&root.0).unwrap();
        state = moved(&mut store, &state, 420.);
        let covered = std::fs::read(store.wal()).unwrap();
        let args = serde_json::json!({"locale":"en"});
        state
            .dispatch_view("view.preferences", args.clone(), "test")
            .unwrap();
        store
            .commit(&state, "view.preferences", &args, "test")
            .unwrap();
        std::fs::write(store.wal(), covered).unwrap();
        assert_eq!(Persistence::open(&root.0).unwrap().1, state);
        for i in 0..145 {
            state = moved(&mut store, &state, 500. + i as f32);
        }
        let (reopened, recovered) = Persistence::open(&root.0).unwrap();
        assert_eq!(recovered, state);
        assert!(reopened.records < CHECKPOINT_RECORDS);
        assert_eq!(recovered.undo.len(), 100);
    }
}
