use design_core::{
    entities::{Entities, Entity},
    Document,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
pub struct Snapshot {
    pub document: Arc<Document>,
    pub meta: Value,
    pub cursor: String,
}
pub fn filter(delta: &Value) -> Value {
    let mut set = serde_json::Map::new();
    for key in ["revision", "selected", "locale", "panels"] {
        if let Some(v) = delta["set"].get(key) {
            set.insert(key.into(), v.clone());
        }
    }
    let mut result = json!({"set":set});
    if let Some(doc) = delta.get("document") {
        result["document"] = doc.clone();
    }
    result
}
fn collection<T: Clone + DeserializeOwned + Entity>(
    old: &mut Entities<T>,
    delta: &Value,
    id: impl Fn(&T) -> String,
) -> Result<(), String> {
    let upserts: Vec<T> =
        serde_json::from_value(delta["upsert"].clone()).map_err(|e| e.to_string())?;
    if delta["remove"].as_array().is_some_and(|v| v.is_empty())
        && delta.get("order").is_none()
        && upserts.iter().all(|v| old.by_id(v.id()).is_some())
    {
        let mut seen = BTreeSet::new();
        for value in upserts {
            let key = value.id().to_owned();
            if !seen.insert(key.clone()) {
                return Err("Duplicate native delta entity".into());
            }
            old.update(&key, |target| *target = value)?;
        }
        return Ok(());
    }
    let mut values: BTreeMap<String, T> = old.iter().map(|v| (id(v), v.clone())).collect();
    let mut order: Vec<_> = old.iter().map(&id).collect();
    let remove: Vec<String> =
        serde_json::from_value(delta["remove"].clone()).map_err(|e| e.to_string())?;
    for key in &remove {
        if values.remove(key).is_none() {
            return Err("Unknown native delta removal".into());
        }
    }
    let removed: BTreeSet<_> = remove.iter().collect();
    order.retain(|id| !removed.contains(id));
    let upsert: Vec<T> =
        serde_json::from_value(delta["upsert"].clone()).map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    for value in upsert {
        let key = id(&value);
        if !seen.insert(key.clone()) {
            return Err("Duplicate native delta entity".into());
        }
        if !values.contains_key(&key) {
            order.push(key.clone());
        }
        values.insert(key, value);
    }
    if let Some(requested) = delta.get("order") {
        order = serde_json::from_value(requested.clone()).map_err(|e| e.to_string())?;
    }
    if order.len() != values.len() || order.iter().collect::<BTreeSet<_>>().len() != values.len() {
        return Err("Invalid native delta order".into());
    }
    let next = order
        .iter()
        .map(|id| {
            values
                .remove(id)
                .ok_or("Missing native delta entity".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    *old = next.into();
    Ok(())
}
/// Transactional view-cache update. A failed replay leaves the old Arc intact.
pub fn apply(previous: Option<&Snapshot>, response: &Value) -> Result<Snapshot, String> {
    if response["changed"] == false {
        return previous.cloned().ok_or("Missing native base".into());
    }
    let cursor = response["cursor"]
        .as_str()
        .ok_or("Missing native cursor")?
        .to_string();
    if response["kind"] == "snapshot" {
        let mut meta = response["snapshot"].clone();
        let doc = meta
            .as_object_mut()
            .ok_or("Invalid native snapshot")?
            .remove("document");
        let document = if let Some(doc) = doc {
            Arc::new(serde_json::from_value(doc).map_err(|e| e.to_string())?)
        } else {
            previous.ok_or("Missing initial document")?.document.clone()
        };
        return Ok(Snapshot {
            document,
            meta,
            cursor,
        });
    }
    let previous = previous.ok_or("Missing native delta base")?;
    if response["kind"] != "delta" || response["base"].as_str() != Some(&previous.cursor) {
        return Err("Native sync base mismatch".into());
    }
    let mut document = None;
    let mut meta = previous.meta.clone();
    for delta in response["deltas"]
        .as_array()
        .ok_or("Missing native deltas")?
    {
        for (key, value) in delta["set"].as_object().ok_or("Invalid native metadata")? {
            if !["revision", "selected", "locale", "panels"].contains(&key.as_str()) {
                return Err("Unexpected native field".into());
            }
            meta[key] = value.clone();
        }
        if let Some(change) = delta.get("document") {
            let doc = document.get_or_insert_with(|| (*previous.document).clone());
            if let Some(name) = change.get("name") {
                doc.name = name.as_str().ok_or("Invalid document name")?.into();
            }
            if let Some(nodes) = change.get("nodes") {
                collection(&mut doc.nodes, nodes, |n| n.id.clone())?;
            }
            if let Some(edges) = change.get("edges") {
                collection(&mut doc.edges, edges, |e| e.id.clone())?;
            }
        }
    }
    Ok(Snapshot {
        document: document
            .map(Arc::new)
            .unwrap_or_else(|| previous.document.clone()),
        meta,
        cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_replay_matches_full_state_and_shares_metadata_only_documents() {
        let mut old = design_core::Workspace::default();
        let response = json!({"kind":"snapshot","changed":true,"cursor":"s:0","snapshot":{"document":old.document,"revision":0,"locale":old.locale,"panels":old.panels,"selected":old.selected}});
        let first = apply(None, &response).unwrap();
        let mut new = old.clone();
        new.dispatch_view("view.preferences", json!({"locale":"en"}), "test")
            .unwrap();
        let response = json!({"kind":"delta","changed":true,"base":"s:0","cursor":"s:1","deltas":[filter(&crate::delta::between(&old,&new))]});
        let metadata = apply(Some(&first), &response).unwrap();
        assert!(Arc::ptr_eq(&first.document, &metadata.document));
        old = new.clone();
        new.dispatch_view("design.apply_patch",json!({"expected_revision":0,"label":"Move","operations":[{"op":"update_node","id":"add","x":555.}]}),"test").unwrap();
        let response = json!({"kind":"delta","changed":true,"base":"s:1","cursor":"s:2","deltas":[filter(&crate::delta::between(&old,&new))]});
        let moved = apply(Some(&metadata), &response).unwrap();
        assert_eq!(moved.document, new.document);
        assert!(serde_json::to_vec(&response).unwrap().len() < 500);
        assert_eq!(first.document.node("add").unwrap().x, 310.);
        assert!(apply(Some(&first), &response).is_err());
        let malformed = json!({"kind":"delta","changed":true,"base":"s:2","cursor":"s:3","deltas":[{"set":{},"document":{"nodes":{"upsert":[],"remove":[],"order":["missing"]}}}]});
        assert!(apply(Some(&moved), &malformed).is_err());
        assert_eq!(moved.document, new.document);
        let mut cached = moved;
        // Structural removals, additions, import ordering and Undo use the same
        // wire format as moves, without retaining references to removed entities.
        for step in 0..4 {
            let old = new.clone();
            if step == 0 {
                Arc::make_mut(&mut new.document).nodes.reverse();
                Arc::make_mut(&mut new.document).edges.reverse();
                Arc::make_mut(&mut new.document).name = "reordered".into();
            } else if step == 1 {
                new.dispatch_view("design.apply_patch",json!({"expected_revision":new.revision,"label":"remove","operations":[{"op":"remove_node","id":"result"}]}),"test").unwrap();
            } else if step == 2 {
                new.dispatch_view(
                    "design.undo",
                    json!({"expected_revision":new.revision}),
                    "test",
                )
                .unwrap();
            } else if step == 3 {
                new.dispatch_view("design.apply_patch",json!({"expected_revision":new.revision,"label":"rename","operations":[{"op":"update_node","id":"add","name":"sum"}]}),"test").unwrap();
            }
            let response = json!({"kind":"delta","changed":true,"base":cached.cursor,"cursor":format!("s:{}",step+3),"deltas":[filter(&crate::delta::between(&old,&new))]});
            cached = apply(Some(&cached), &response).unwrap();
            assert_eq!(cached.document, new.document);
        }
    }
}
