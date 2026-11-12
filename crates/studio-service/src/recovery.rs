//! Explicit offline recovery. Original files are copied and synced before any
//! replacement; the owner lock and a content token reject concurrent changes.
use crate::persistence::Checkpoint;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
const FILES: [&str; 5] = [
    "workspace.json",
    "workspace.wal",
    "workspace.previous.json",
    "panel-layout.json",
    "workspace.tmp",
];
#[derive(Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub nodes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub salvage: Option<Salvage>,
}
#[derive(Serialize)]
pub struct Salvage {
    pub moves: usize,
    pub valid_bytes: usize,
    pub discarded_bytes: usize,
    pub stopped: String,
}
fn salvaged(root: &Path) -> Result<(Checkpoint, Salvage), String> {
    let mut value = checkpoint(&root.join(".logic/workspace.json"))?;
    let bytes = fs::read(root.join(".logic/workspace.wal")).map_err(|e| e.to_string())?;
    let replay =
        crate::persistence::replay_prefix(&mut value.workspace, value.journal_sequence, &bytes);
    if replay.records == 0 || replay.offset == bytes.len() {
        return Err("No damaged journal prefix to recover".into());
    }
    value.journal_sequence = replay.sequence;
    Ok((
        value,
        Salvage {
            moves: replay.records,
            valid_bytes: replay.offset,
            discarded_bytes: bytes.len() - replay.offset,
            stopped: replay
                .error
                .unwrap_or_else(|| "Incomplete final frame".into()),
        },
    ))
}
#[derive(Serialize)]
pub struct Status {
    pub token: String,
    pub candidates: Vec<Candidate>,
    pub can_reset_layout: bool,
}
fn lock(root: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".logic/owner.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|_| "A Design Core already owns this workspace".to_string())?;
    Ok(file)
}
fn checkpoint(path: &Path) -> Result<Checkpoint, String> {
    validate_checkpoint(&fs::read(path).map_err(|e| e.to_string())?)
}
fn validate_checkpoint(bytes: &[u8]) -> Result<Checkpoint, String> {
    let value: Checkpoint = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value.workspace.schema != 1 {
        return Err("Unsupported workspace schema".into());
    }
    value.workspace.document.validate(false)?;
    for doc in value.workspace.undo.iter().chain(&value.workspace.redo) {
        doc.validate(false)?;
    }
    for experiment in &value.workspace.experiments {
        for candidate in &experiment.candidates {
            candidate.document.validate(false)?;
        }
    }
    Ok(value)
}
/// Validated file snapshot; paths/bytes cannot be supplied by a WebView on restore.
pub struct ExternalCheckpoint {
    path: PathBuf,
    bytes: Vec<u8>,
    candidate: Candidate,
}
impl ExternalCheckpoint {
    pub fn preview(&self) -> serde_json::Value {
        serde_json::json!({"id":"external","name":self.candidate.name,"revision":self.candidate.revision,"nodes":self.candidate.nodes,"source":self.path})
    }
    fn check_source(&self) -> Result<(), String> {
        if fs::read(&self.path).map_err(|e| e.to_string())? != self.bytes {
            return Err("Backup changed; choose it again".into());
        }
        Ok(())
    }
}
pub fn read_backup(path: &Path) -> Result<ExternalCheckpoint, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let value = validate_checkpoint(&bytes)?;
    Ok(ExternalCheckpoint {
        path,
        bytes,
        candidate: Candidate {
            id: "external".into(),
            name: value.workspace.document.name.clone(),
            revision: value.workspace.revision,
            nodes: value.workspace.document.nodes.len(),
            salvage: None,
        },
    })
}
pub fn restore_backup(
    root: &Path,
    expected: &str,
    backup: &ExternalCheckpoint,
) -> Result<PathBuf, String> {
    restore_inner(root, false, expected, "external", Some(backup))
}
fn token(root: &Path) -> Result<String, String> {
    let mut digest = Sha256::new();
    for name in FILES {
        digest.update(name.as_bytes());
        match fs::read(root.join(".logic").join(name)) {
            Ok(bytes) => {
                digest.update([1]);
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => digest.update([0]),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn inspect(root: &Path, layout: bool) -> Result<Status, String> {
    let mut candidates = vec![];
    if !layout {
        for (id, name) in [
            ("checkpoint", "workspace.json"),
            ("previous", "workspace.previous.json"),
        ] {
            if let Ok(value) = checkpoint(&root.join(".logic").join(name)) {
                candidates.push(Candidate {
                    id: id.into(),
                    name: value.workspace.document.name.clone(),
                    revision: value.workspace.revision,
                    nodes: value.workspace.document.nodes.len(),
                    salvage: None,
                });
            }
        }
    }
    if !layout {
        if let Ok((value, salvage)) = salvaged(root) {
            candidates.insert(
                0,
                Candidate {
                    id: "salvage".into(),
                    name: value.workspace.document.name.clone(),
                    revision: value.workspace.revision,
                    nodes: value.workspace.document.nodes.len(),
                    salvage: Some(salvage),
                },
            );
        }
    }
    Ok(Status {
        token: token(root)?,
        candidates,
        can_reset_layout: layout,
    })
}
pub fn status(root: &Path, layout: bool) -> Result<Status, String> {
    let _owner = lock(root)?;
    inspect(root, layout)
}
fn synced_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
pub fn restore(root: &Path, layout: bool, expected: &str, choice: &str) -> Result<PathBuf, String> {
    restore_inner(root, layout, expected, choice, None)
}
fn restore_inner(
    root: &Path,
    layout: bool,
    expected: &str,
    choice: &str,
    external: Option<&ExternalCheckpoint>,
) -> Result<PathBuf, String> {
    let _owner = lock(root)?;
    let status = inspect(root, layout)?;
    if status.token != expected {
        return Err("Workspace changed; refresh recovery options".into());
    }
    let replacement = if let Some(external) = external {
        external.check_source()?;
        Some(external.bytes.clone())
    } else if layout && choice == "layout" {
        None
    } else {
        if !status.candidates.iter().any(|c| c.id == choice) {
            return Err("Invalid recovery choice".into());
        }
        if choice == "salvage" {
            Some(serde_json::to_vec(&salvaged(root)?.0).map_err(|e| e.to_string())?)
        } else {
            Some(
                fs::read(root.join(".logic").join(if choice == "checkpoint" {
                    "workspace.json"
                } else {
                    "workspace.previous.json"
                }))
                .map_err(|e| e.to_string())?,
            )
        }
    };
    let dir = root.join(".logic");
    let parent = dir.join("recovery");
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let backup = parent.join(format!("{}-{}", now.as_nanos(), std::process::id()));
    fs::create_dir(&backup).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        fs::set_permissions(&backup, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    // No active file changes until every existing source file and manifest is durable.
    for name in FILES {
        match fs::read(dir.join(name)) {
            Ok(bytes) => synced_write(&backup.join(name), &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let manifest = serde_json::json!({"schema":1,"choice":choice,"source_token":expected,"unix_ms":now.as_millis(),"salvage":if choice == "salvage" {Some(salvaged(root)?.1)}else{None},"external_source":external.map(|v|&v.path),"external_sha256":external.map(|v|format!("{:x}",Sha256::digest(&v.bytes)))});
    synced_write(
        &backup.join("manifest.json"),
        &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
    )?;
    sync_dir(&backup)?;
    sync_dir(&parent)?;
    sync_dir(&dir)?;
    if token(root)? != expected {
        return Err("Workspace changed during backup; original files were not replaced".into());
    }
    if let Some(external) = external {
        external.check_source()?;
    }
    if let Some(bytes) = replacement {
        let temp = backup.join("restored.tmp");
        synced_write(&temp, &bytes)?;
        // Install checkpoint first; if interrupted before journal replacement,
        // startup remains conservative and shows recovery again.
        fs::rename(temp, dir.join("workspace.json")).map_err(|e| e.to_string())?;
        sync_dir(&dir)?;
        let temp = backup.join("empty-journal.tmp");
        synced_write(&temp, &[])?;
        fs::rename(temp, dir.join("workspace.wal")).map_err(|e| e.to_string())?;
    } else {
        let temp = backup.join("layout.tmp");
        synced_write(
            &temp,
            br#"{"schema":1,"sizes":[300,300,230],"homes":{},"frames":{}}"#,
        )?;
        fs::rename(temp, dir.join("panel-layout.json")).map_err(|e| e.to_string())?;
    }
    sync_dir(&dir)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Request, Service};
    use serde_json::json;
    struct Root(PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("las-recovery-{name}-{}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn call(service: &Service, method: &str, args: serde_json::Value) {
        service
            .execute(Request {
                method: method.into(),
                args,
                actor: "test".into(),
            })
            .unwrap();
    }
    #[test]
    fn salvage_stops_at_first_bad_frame_preserves_originals_and_history() {
        for mode in [
            "checksum", "length", "json", "schema", "sequence", "revision", "node", "partial",
        ] {
            let root = Root::new(&format!("salvage-{mode}"));
            let service = Service::open(&root.0).unwrap();
            for revision in 0..3 {
                call(
                    &service,
                    "design.apply_patch",
                    json!({"expected_revision":revision,"label":"move","operations":[{"op":"update_node","id":"add","x":500+revision}]}),
                );
            }
            drop(service);
            let path = root.0.join(".logic/workspace.wal");
            let good = fs::read(&path).unwrap();
            let first = 36 + u32::from_le_bytes(good[..4].try_into().unwrap()) as usize;
            let second = first
                + 36
                + u32::from_le_bytes(good[first..first + 4].try_into().unwrap()) as usize;
            let mut damaged = good[..first].to_vec();
            match mode {
                "checksum" => {
                    damaged.extend_from_slice(&good[first..second]);
                    damaged[first + 4] ^= 1;
                }
                "length" => damaged.extend_from_slice(&[255; 36]),
                "partial" => damaged.extend_from_slice(&good[first..first + 10]),
                _ => {
                    let mut record: serde_json::Value =
                        serde_json::from_slice(&good[first + 36..second]).unwrap();
                    match mode {
                        "schema" => record["schema"] = json!(2),
                        "sequence" => record["sequence"] = json!(99),
                        "revision" => record["args"]["expected_revision"] = json!(99),
                        "node" => record["args"]["operations"][0]["id"] = json!("missing"),
                        _ => {}
                    }
                    let payload = if mode == "json" {
                        b"invalid".to_vec()
                    } else {
                        serde_json::to_vec(&record).unwrap()
                    };
                    damaged.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                    damaged.extend_from_slice(&Sha256::digest(&payload));
                    damaged.extend_from_slice(&payload);
                }
            }
            if mode != "partial" {
                damaged.extend_from_slice(&good[second..]);
            }
            fs::write(&path, &damaged).unwrap();
            let checkpoint_bytes = fs::read(root.0.join(".logic/workspace.json")).unwrap();
            let preview = status(&root.0, false).unwrap();
            let candidate = preview
                .candidates
                .iter()
                .find(|c| c.id == "salvage")
                .unwrap();
            assert_eq!(candidate.revision, 1, "{mode}");
            assert_eq!(candidate.salvage.as_ref().unwrap().moves, 1);
            assert_eq!(fs::read(&path).unwrap(), damaged); // preview is read-only
            let archive = restore(&root.0, false, &preview.token, "salvage").unwrap();
            assert_eq!(fs::read(archive.join("workspace.wal")).unwrap(), damaged);
            assert_eq!(
                fs::read(archive.join("workspace.json")).unwrap(),
                checkpoint_bytes
            );
            let recovered = Service::open(&root.0).unwrap();
            assert_eq!(
                recovered
                    .state
                    .lock()
                    .unwrap()
                    .document
                    .node("add")
                    .unwrap()
                    .x,
                500.
            );
            call(&recovered, "design.undo", json!({"expected_revision":1}));
            assert_eq!(
                recovered
                    .state
                    .lock()
                    .unwrap()
                    .document
                    .node("add")
                    .unwrap()
                    .x,
                design_core::Document::demo().node("add").unwrap().x
            );
        }
    }
    #[test]
    fn salvage_requires_a_valid_current_base_and_at_least_one_new_move() {
        let root = Root::new("salvage-empty");
        drop(Service::open(&root.0).unwrap());
        let wal = root.0.join(".logic/workspace.wal");
        for bytes in [b"".as_slice(), b"corrupt"] {
            fs::write(&wal, bytes).unwrap();
            let preview = status(&root.0, false).unwrap();
            assert!(!preview.candidates.iter().any(|c| c.id == "salvage"));
            assert!(restore(&root.0, false, &preview.token, "salvage").is_err());
        }
    }
    #[test]
    fn selected_backup_restores_without_valid_local_candidates_and_keeps_originals() {
        let source = Root::new("external-source");
        let service = Service::open(&source.0).unwrap();
        call(
            &service,
            "design.apply_patch",
            json!({"expected_revision":0,"label":"rename","operations":[{"op":"update_node","id":"add","name":"restored_sum"}]}),
        );
        drop(service);
        let source_path = source.0.join(".logic/workspace.json");
        let source_bytes = fs::read(&source_path).unwrap();
        let selected = read_backup(&source_path).unwrap();
        assert_eq!(selected.preview()["revision"], 1);
        let target = Root::new("external-target");
        drop(Service::open(&target.0).unwrap());
        fs::write(target.0.join(".logic/workspace.json"), b"damaged target").unwrap();
        fs::write(target.0.join(".logic/workspace.wal"), b"damaged journal").unwrap();
        let state = status(&target.0, false).unwrap();
        assert!(state.candidates.is_empty());
        let archive = restore_backup(&target.0, &state.token, &selected).unwrap();
        assert_eq!(
            fs::read(archive.join("workspace.json")).unwrap(),
            b"damaged target"
        );
        assert_eq!(
            fs::read(archive.join("workspace.wal")).unwrap(),
            b"damaged journal"
        );
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        let recovered = Service::open(&target.0).unwrap();
        assert_eq!(
            recovered
                .state
                .lock()
                .unwrap()
                .document
                .node("add")
                .unwrap()
                .name,
            "restored_sum"
        );
        call(&recovered, "design.undo", json!({"expected_revision":1}));
        assert_eq!(
            recovered
                .state
                .lock()
                .unwrap()
                .document
                .node("add")
                .unwrap()
                .name,
            "add"
        );
    }
    #[test]
    fn bad_or_changed_backup_and_changed_target_cannot_be_restored() {
        let source = Root::new("external-guards-source");
        drop(Service::open(&source.0).unwrap());
        let path = source.0.join(".logic/workspace.json");
        let original = fs::read(&path).unwrap();
        let selected = read_backup(&path).unwrap();
        let target = Root::new("external-guards-target");
        drop(Service::open(&target.0).unwrap());
        let state = status(&target.0, false).unwrap();
        let before = fs::read(target.0.join(".logic/workspace.json")).unwrap();
        fs::write(&path, b"bad JSON").unwrap();
        assert!(read_backup(&path).is_err());
        assert!(restore_backup(&target.0, &state.token, &selected)
            .unwrap_err()
            .contains("Backup changed"));
        assert_eq!(
            fs::read(target.0.join(".logic/workspace.json")).unwrap(),
            before
        );
        fs::write(&path, &original).unwrap();
        fs::write(target.0.join(".logic/workspace.wal"), b"changed target").unwrap();
        assert!(restore_backup(&target.0, &state.token, &selected).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["document"]["nodes"][0]["width"] = json!(0);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(read_backup(&path).is_err());
    }
    #[test]
    fn corrupt_journal_restores_checkpoint_with_exact_original_backups() {
        let root = Root::new("wal");
        let service = Service::open(&root.0).unwrap();
        call(
            &service,
            "design.apply_patch",
            json!({"expected_revision":0,"label":"rename","operations":[{"op":"update_node","id":"add","name":"sum"}]}),
        );
        call(
            &service,
            "design.apply_patch",
            json!({"expected_revision":1,"label":"move","operations":[{"op":"update_node","id":"add","x":555}]}),
        );
        drop(service);
        let wal = root.0.join(".logic/workspace.wal");
        let mut bytes = fs::read(&wal).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&wal, &bytes).unwrap();
        assert!(Service::open(&root.0).is_err());
        let status = status(&root.0, false).unwrap();
        assert_eq!(status.candidates[0].revision, 1);
        assert_eq!(status.candidates[1].revision, 0);
        let current = fs::read(root.0.join(".logic/workspace.json")).unwrap();
        let backup = restore(&root.0, false, &status.token, "checkpoint").unwrap();
        assert_eq!(fs::read(backup.join("workspace.json")).unwrap(), current);
        assert_eq!(fs::read(backup.join("workspace.wal")).unwrap(), bytes);
        let service = Service::open(&root.0).unwrap();
        assert_eq!(service.state.lock().unwrap().revision, 1);
        call(&service, "design.undo", json!({"expected_revision":1}));
        assert_eq!(
            service
                .state
                .lock()
                .unwrap()
                .document
                .node("add")
                .unwrap()
                .name,
            "add"
        );
    }
    #[test]
    fn corrupt_checkpoint_can_restore_previous_and_missing_options_are_read_only() {
        let root = Root::new("previous");
        let service = Service::open(&root.0).unwrap();
        call(&service, "view.preferences", json!({"locale":"en"}));
        drop(service);
        let current = root.0.join(".logic/workspace.json");
        fs::write(&current, b"broken checkpoint").unwrap();
        let status = status(&root.0, false).unwrap();
        assert_eq!(status.candidates.len(), 1);
        assert_eq!(status.candidates[0].id, "previous");
        let backup = restore(&root.0, false, &status.token, "previous").unwrap();
        assert_eq!(
            fs::read(backup.join("workspace.json")).unwrap(),
            b"broken checkpoint"
        );
        let service = Service::open(&root.0).unwrap();
        assert_eq!(service.state.lock().unwrap().locale, "ja");
        drop(service);
        fs::write(&current, b"broken again").unwrap();
        fs::write(
            root.0.join(".logic/workspace.previous.json"),
            b"also broken",
        )
        .unwrap();
        let status = super::status(&root.0, false).unwrap();
        assert!(status.candidates.is_empty());
        assert!(restore(&root.0, false, &status.token, "checkpoint").is_err());
        assert_eq!(fs::read(current).unwrap(), b"broken again");
    }
    #[test]
    fn active_owner_stale_content_and_backup_failure_prevent_replacement() {
        let root = Root::new("guards");
        let service = Service::open(&root.0).unwrap();
        assert!(status(&root.0, false).is_err());
        assert!(restore(&root.0, false, "bad", "checkpoint").is_err());
        drop(service);
        let status = status(&root.0, false).unwrap();
        let wal = root.0.join(".logic/workspace.wal");
        fs::write(&wal, b"changed").unwrap();
        assert!(restore(&root.0, false, &status.token, "checkpoint")
            .unwrap_err()
            .contains("changed"));
        assert_eq!(fs::read(&wal).unwrap(), b"changed");
        let status = super::status(&root.0, false).unwrap();
        fs::write(root.0.join(".logic/recovery"), b"blocked").unwrap();
        assert!(restore(&root.0, false, &status.token, "checkpoint").is_err());
        assert_eq!(fs::read(&wal).unwrap(), b"changed");
    }
    #[test]
    fn layout_reset_preserves_circuit_and_live_journal() {
        let root = Root::new("layout");
        let service = Service::open(&root.0).unwrap();
        call(
            &service,
            "design.apply_patch",
            json!({"expected_revision":0,"label":"move","operations":[{"op":"update_node","id":"add","x":555}]}),
        );
        drop(service);
        fs::write(root.0.join(".logic/panel-layout.json"), b"bad layout").unwrap();
        let wal = fs::read(root.0.join(".logic/workspace.wal")).unwrap();
        let current = fs::read(root.0.join(".logic/workspace.json")).unwrap();
        let status = status(&root.0, true).unwrap();
        assert!(status.can_reset_layout);
        assert!(status.candidates.is_empty());
        let backup = restore(&root.0, true, &status.token, "layout").unwrap();
        assert_eq!(
            fs::read(backup.join("panel-layout.json")).unwrap(),
            b"bad layout"
        );
        assert_eq!(fs::read(root.0.join(".logic/workspace.wal")).unwrap(), wal);
        assert_eq!(
            fs::read(root.0.join(".logic/workspace.json")).unwrap(),
            current
        );
        assert_eq!(
            Service::open(&root.0)
                .unwrap()
                .state
                .lock()
                .unwrap()
                .document
                .node("add")
                .unwrap()
                .x,
            555.
        );
    }
}
