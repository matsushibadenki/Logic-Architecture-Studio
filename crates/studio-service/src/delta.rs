use design_core::{
    entities::{Entities, Entity},
    Workspace,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 8 * 1024 * 1024;
pub struct Journal {
    pub epoch: String,
    pub sequence: u64,
    entries: VecDeque<(u64, Value, usize)>,
    bytes: usize,
}
impl Journal {
    pub fn new(epoch: String) -> Self {
        Self {
            epoch,
            sequence: 0,
            entries: VecDeque::new(),
            bytes: 0,
        }
    }
    pub fn cursor(&self) -> String {
        format!("{}:{}", self.epoch, self.sequence)
    }
    pub fn publish(&mut self, delta: Value) {
        self.sequence += 1;
        let bytes = serde_json::to_vec(&delta).unwrap().len();
        self.bytes += bytes;
        self.entries.push_back((self.sequence, delta, bytes));
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES {
            self.bytes -= self.entries.pop_front().unwrap().2;
        }
    }
    pub fn since(&self, cursor: &str) -> Option<Vec<Value>> {
        let (epoch, sequence) = cursor.rsplit_once(':')?;
        let sequence: u64 = sequence.parse().ok()?;
        if epoch != self.epoch || sequence > self.sequence {
            return None;
        }
        if sequence == self.sequence {
            return Some(vec![]);
        }
        if self.entries.front().is_none_or(|e| sequence + 1 < e.0) {
            return None;
        }
        Some(
            self.entries
                .iter()
                .filter(|(s, _, _)| *s > sequence)
                .map(|(_, d, _)| d.clone())
                .collect(),
        )
    }
}
fn collection<T: PartialEq + Serialize>(
    old: &[T],
    new: &[T],
    id: impl Fn(&T) -> String,
) -> Option<Value> {
    if old == new {
        return None;
    }
    let before: BTreeMap<_, _> = old.iter().map(|v| (id(v), v)).collect();
    let after: BTreeMap<_, _> = new.iter().map(|v| (id(v), v)).collect();
    let upsert: Vec<_> = new
        .iter()
        .filter(|v| before.get(&id(v)).is_none_or(|old| *old != *v))
        .collect();
    let remove: Vec<_> = before.keys().filter(|k| !after.contains_key(*k)).collect();
    let mut delta = json!({"upsert":upsert,"remove":remove});
    let expected = old
        .iter()
        .map(&id)
        .filter(|k| after.contains_key(k))
        .chain(new.iter().map(&id).filter(|k| !before.contains_key(k)));
    if expected.ne(new.iter().map(&id)) {
        delta["order"] = json!(new.iter().map(id).collect::<Vec<_>>());
    }
    Some(delta)
}
fn entities<T: Entity + Clone + PartialEq + Serialize>(
    old: &Entities<T>,
    new: &Entities<T>,
) -> Option<Value> {
    if old.same_storage(new) {
        return None;
    }
    if let Some(changed) = new.changed(old) {
        if changed.is_empty() {
            return None;
        }
        return Some(
            json!({"upsert":changed.into_iter().map(|(new,_)|new).collect::<Vec<_>>(),"remove":[]}),
        );
    }
    collection(&old.to_vec(), &new.to_vec(), |v| v.id().to_owned())
}
pub fn between(old: &Workspace, new: &Workspace) -> Value {
    let mut set = serde_json::Map::new();
    macro_rules! field {
        ($name:ident) => {
            if old.$name != new.$name {
                set.insert(stringify!($name).into(), json!(new.$name));
            }
        };
    }
    field!(revision);
    field!(selected);
    field!(locale);
    field!(panels);
    field!(report);
    if old.undo.is_empty() != new.undo.is_empty() {
        set.insert("can_undo".into(), json!(!new.undo.is_empty()));
    }
    if old.redo.is_empty() != new.redo.is_empty() {
        set.insert("can_redo".into(), json!(!new.redo.is_empty()));
    }
    let mut result = json!({"set":set});
    let mut doc = serde_json::Map::new();
    if old.document.name != new.document.name {
        doc.insert("name".into(), json!(new.document.name));
    }
    if let Some(delta) = entities(&old.document.nodes, &new.document.nodes) {
        doc.insert("nodes".into(), delta);
    }
    if let Some(delta) = entities(&old.document.edges, &new.document.edges) {
        doc.insert("edges".into(), delta);
    }
    if !doc.is_empty() {
        result["document"] = json!(doc);
    }
    if let Some(delta) = collection(&old.history, &new.history, |r| r.revision.to_string()) {
        result["history"] = delta;
    }
    if let Some(delta) = collection(&old.experiments, &new.experiments, |e| e.id.clone()) {
        result["experiments"] = delta;
    }
    // Position changes cannot change graph diagnostics. Avoid revalidating
    // a large graph on every drag solely to construct the WebView delta.
    let semantic_change = |a: &design_core::Node, b: &design_core::Node| {
        a.id != b.id || a.name != b.name || a.kind != b.kind || a.width != b.width
    };
    let topology_changed = old.document.edges != new.document.edges
        || new
            .document
            .nodes
            .changed(&old.document.nodes)
            .is_none_or(|changes| changes.iter().any(|(a, b)| semantic_change(a, b)));
    if topology_changed {
        let before = old.document.diagnostics();
        let after = new.document.diagnostics();
        if before != after {
            result["set"]["diagnostics"] = json!(after);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_multiblock_moves_match_full_comparison() {
        let mut old = Workspace::default();
        old.document = std::sync::Arc::new(design_core::Document {
            name: "Large".into(),
            nodes: (0..2000)
                .map(|i| design_core::Node {
                    id: format!("n{i}"),
                    name: format!("n{i}"),
                    kind: design_core::Kind::Input,
                    width: 32,
                    x: i as f32,
                    y: 0.,
                })
                .collect(),
            edges: Vec::new().into(),
        });
        let mut new = old.clone();
        let operations: Vec<_> = [0, 31, 32, 1000, 1999]
            .into_iter()
            .map(|i| json!({"op":"update_node","id":format!("n{i}"),"x":123.5}))
            .collect();
        new.dispatch_view(
            "design.apply_patch",
            json!({"expected_revision":0,"label":"move","operations":operations}),
            "test",
        )
        .unwrap();
        let sparse = between(&old, &new);
        let independent: Workspace =
            serde_json::from_value(serde_json::to_value(&new).unwrap()).unwrap();
        assert_eq!(sparse, between(&old, &independent));
        assert_eq!(
            sparse["document"]["nodes"]["upsert"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert!(sparse["document"].get("edges").is_none());
        let base = crate::native_sync::apply(
            None,
            &json!({"changed":true,"kind":"snapshot","cursor":"s:0","snapshot":old.snapshot()}),
        )
        .unwrap();
        let applied=crate::native_sync::apply(Some(&base),&json!({"changed":true,"kind":"delta","base":"s:0","cursor":"s:1","deltas":[crate::native_sync::filter(&sparse)]})).unwrap();
        assert_eq!(applied.document, new.document);
    }
    #[test]
    fn journal_replays_or_falls_back_with_bounded_memory() {
        let mut journal = Journal::new("session".into());
        journal.publish(json!({"set":{"locale":"ja"}}));
        assert_eq!(journal.since("session:0").unwrap().len(), 1);
        assert!(journal.since("other:0").is_none());
        assert!(journal.since("session:2").is_none());
        for _ in 0..65 {
            journal.publish(json!({"set":{}}));
        }
        assert!(journal.since("session:0").is_none());
        assert_eq!(journal.since("session:65").unwrap().len(), 1);
        journal.publish(json!({"large":"x".repeat(MAX_BYTES+1)}));
        assert!(journal.entries.is_empty());
        assert!(journal.since("session:66").is_none());
    }
    #[test]
    fn generated_web_deltas_cover_moves_diagnostics_history_and_reordering() {
        let mut state = Workspace::default();
        let initial = state.snapshot();
        let mut cases = vec![];
        for i in 0..110 {
            let old = state.clone();
            let args = json!({"expected_revision":state.revision,"label":"Move","operations":[{"op":"update_node","id":"add","x":200.+i as f64}]});
            state.dispatch("design.apply_patch", args, "test").unwrap();
            let delta = between(&old, &state);
            assert_eq!(
                delta["document"]["nodes"]["upsert"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert!(delta["document"]["nodes"].get("order").is_none());
            assert!(delta.get("experiments").is_none());
            assert!(delta["set"].get("diagnostics").is_none());
            cases.push(json!({"delta":delta,"expected":state.snapshot()}));
        }
        for operation in [
            json!({"op":"disconnect","id":state.document.edges[0].id}),
            json!({"op":"update_node","id":"add","name":"sum"}),
            json!({"op":"remove_node","id":"add"}),
        ] {
            let old = state.clone();
            state.dispatch("design.apply_patch",json!({"expected_revision":state.revision,"label":"Edit","operations":[operation]}),"test").unwrap();
            cases.push(json!({"delta":between(&old,&state),"expected":state.snapshot()}));
        }
        let old = state.clone();
        std::sync::Arc::make_mut(&mut state.document)
            .nodes
            .reverse();
        std::sync::Arc::make_mut(&mut state.document)
            .edges
            .reverse();
        cases.push(json!({"delta":between(&old,&state),"expected":state.snapshot()}));
        if let Some(path) = std::env::var_os("LAS_WEB_DELTA_FIXTURE") {
            std::fs::write(
                path,
                serde_json::to_vec(&json!({"initial":initial,"cases":cases})).unwrap(),
            )
            .unwrap();
        }
    }
}
