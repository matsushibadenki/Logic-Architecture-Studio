use crate::Service;
use design_core::NODE_WIDTH;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use studio_routing::{Link, Obstacle, Router};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    id: String,
    x: f32,
    y: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    expected_revision: u64,
    definition: Option<String>,
    #[serde(default)]
    positions: Vec<Position>,
}
fn id(domain: &str, key: &str) -> u128 {
    let digest = Sha256::digest(format!("{domain}:{key}").as_bytes());
    u128::from_be_bytes(digest[..16].try_into().unwrap())
}
pub fn execute(service: &Service, args: Value) -> Result<Value, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| e.to_string())?;
    if args.positions.len() > 512 {
        return Err("Too many preview positions".into());
    }
    let doc = {
        let state = service.state.lock().map_err(|e| e.to_string())?;
        if state.revision != args.expected_revision {
            return Err("Revision conflict".into());
        }
        match args.definition.as_deref() {
            Some(name) => std::sync::Arc::new(state.document.definition_graph(name)?),
            None => state.document.clone(),
        }
    };
    let mut positions = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for p in args.positions {
        if !p.x.is_finite()
            || !p.y.is_finite()
            || p.x.abs() > 1e6
            || p.y.abs() > 1e6
            || doc.nodes.by_id(&p.id).is_none()
            || !seen.insert(p.id.clone())
        {
            return Err("Invalid preview position".into());
        }
        positions.insert(p.id, (p.x, p.y));
    }
    let nodes: BTreeMap<_, _> = doc
        .nodes
        .iter()
        .map(|n| {
            let (x, y) = positions.get(&n.id).copied().unwrap_or((n.x, n.y));
            (n.id.as_str(), (n, [x, y, NODE_WIDTH, n.height()]))
        })
        .collect();
    let obstacles = nodes
        .iter()
        .map(|(key, (_, rect))| Obstacle {
            id: id("node", key),
            rect: *rect,
        })
        .collect();
    let links: Vec<_> = doc
        .edges
        .iter()
        .map(|e| {
            let (source, a) = nodes[e.from.as_str()];
            let outputs = source.outputs();
            let output = outputs.iter().position(|p| p.0 == e.source_port).unwrap();
            let (b, rect) = nodes[e.to.as_str()];
            let inputs = b.inputs();
            let index = inputs.iter().position(|p| p.0 == e.port).unwrap();
            Link {
                id: id("edge", &e.id),
                source: (id("node", &e.from), output),
                target: (id("node", &e.to), index),
                start: [
                    a[0] + a[2],
                    a[1] + a[3] * ((output + 1) as f32 / (outputs.len() + 1) as f32),
                ],
                end: [
                    rect[0],
                    rect[1] + rect[3] * ((index + 1) as f32 / (inputs.len() + 1) as f32),
                ],
            }
        })
        .collect();
    let routes = Router::new(obstacles).route_all(&links);
    let paths: BTreeMap<_, _> = doc
        .edges
        .iter()
        .map(|e| {
            let route = &routes[&id("edge", &e.id)];
            (
                &e.id,
                json!({"points":route.display_points(),"blocked":route.blocked}),
            )
        })
        .collect();
    Ok(json!({"revision":args.expected_revision,"routes":paths}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_read_only_and_revision_guarded() {
        let root = std::env::temp_dir().join(format!("las-routes-{}", std::process::id()));
        let s = Service::open(&root).unwrap();
        let old = s.state.lock().unwrap().document.clone();
        let result = execute(
            &s,
            json!({"expected_revision":0,"positions":[{"id":"add","x":900.,"y":700.}]}),
        )
        .unwrap();
        assert_eq!(result["revision"], 0);
        assert!(!result["routes"].as_object().unwrap().is_empty());
        assert_eq!(s.state.lock().unwrap().document, old);
        assert!(execute(&s, json!({"expected_revision":1})).is_err());
        assert!(execute(
            &s,
            json!({"expected_revision":0,"positions":[{"id":"missing","x":0,"y":0}]})
        )
        .is_err());
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }
}
