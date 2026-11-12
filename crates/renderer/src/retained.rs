use crate::*;
use std::sync::Arc;
use unge_render::{SceneChanges, SceneUpdate};

/// Derived view cache only; the canonical Rust service remains the authority.
pub struct RetainedProjection {
    source: Document,
    node_colors: BTreeMap<String, String>,
    editor: Editor,
    pub index: SceneIndex,
    pub labels: LabelCatalog,
    pub ids: Arc<BTreeMap<String, Id>>,
}
impl RetainedProjection {
    pub fn new(source: &Document) -> Result<Self, String> {
        let (doc, labels, ids) = project(source)?;
        let index = SceneIndex::new(&doc);
        Ok(Self {
            source: source.clone(),
            node_colors: BTreeMap::new(),
            editor: Editor::new(doc, 0).map_err(|e| e.to_string())?,
            index,
            labels,
            ids: Arc::new(ids),
        })
    }
    pub fn set_node_colors(&mut self, colors: &BTreeMap<String, String>) {
        if &self.node_colors == colors {
            return;
        }
        self.node_colors = colors.clone();
        for node in &self.source.nodes {
            if let Some(label) = self.labels.get_mut(&node.id) {
                label.color = colors
                    .get(node.kind.key())
                    .and_then(|c| design_core::parse_node_color(c));
            }
        }
    }
    pub fn document(&self) -> &unge_core::Document {
        self.editor.document()
    }
    pub fn sync(&mut self, source: &Document) -> Result<SceneUpdate, String> {
        if &self.source == source {
            return Ok(SceneUpdate::Unchanged);
        }
        if source.edges.same_storage(&self.source.edges) {
            if let Some(changed) = source.nodes.changed(&self.source.nodes) {
                if changed.iter().all(|(a, b)| {
                    a.id == b.id && a.name == b.name && a.kind == b.kind && a.width == b.width
                }) {
                    let commands: Vec<_> = changed
                        .into_iter()
                        .filter(|(a, b)| node_rect(a) != node_rect(b))
                        .map(|(node, _)| Command::MoveNode {
                            id: self.ids[&node.id],
                            rect: node_rect(node),
                        })
                        .collect();
                    let update = if commands.is_empty() {
                        SceneUpdate::Unchanged
                    } else {
                        let command = Command::Batch { commands };
                        let changes = SceneChanges::from_command(&command);
                        self.editor.execute(command).map_err(|e| e.to_string())?;
                        self.index.update(self.editor.document(), &changes)
                    };
                    self.source = source.clone();
                    return Ok(update);
                }
            }
        }
        let old: BTreeMap<_, _> = self.source.nodes.iter().map(|n| (&n.id, n)).collect();
        let new: BTreeMap<_, _> = source.nodes.iter().map(|n| (&n.id, n)).collect();
        let replaced: BTreeSet<_> = old
            .iter()
            .filter_map(|(id, n)| {
                new.get(id)
                    .filter(|v| v.kind != n.kind || v.width != n.width)
                    .map(|_| (*id).clone())
            })
            .collect();
        let old_edges: BTreeMap<_, _> = self.source.edges.iter().map(|e| (&e.id, e)).collect();
        let new_edges: BTreeMap<_, _> = source.edges.iter().map(|e| (&e.id, e)).collect();
        let mut commands = vec![];
        for e in &self.source.edges {
            if new_edges.get(&e.id).is_none_or(|v| *v != e)
                || replaced.contains(&e.from)
                || replaced.contains(&e.to)
            {
                commands.push(Command::Disconnect {
                    id: stable_id("edge", &e.id),
                });
            }
        }
        for n in &self.source.nodes {
            if !new.contains_key(&n.id) || replaced.contains(&n.id) {
                commands.push(Command::RemoveNode {
                    id: self.ids[&n.id],
                });
            }
        }
        for n in &source.nodes {
            if !old.contains_key(&n.id) || replaced.contains(&n.id) {
                commands.push(Command::AddNode {
                    node: render_node(n),
                    rect: node_rect(n),
                });
            } else if node_rect(old[&n.id]) != node_rect(n) {
                commands.push(Command::MoveNode {
                    id: stable_id("node", &n.id),
                    rect: node_rect(n),
                });
            }
        }
        for e in &source.edges {
            if old_edges.get(&e.id).is_none_or(|v| *v != e)
                || replaced.contains(&e.from)
                || replaced.contains(&e.to)
            {
                commands.push(Command::Connect {
                    edge: render_edge(e),
                });
            }
        }
        let update = if commands.is_empty() {
            SceneUpdate::Unchanged
        } else {
            let command = Command::Batch { commands };
            let changes = SceneChanges::from_command(&command);
            self.editor.execute(command).map_err(|e| e.to_string())?;
            self.index.update(self.editor.document(), &changes)
        };
        self.labels.retain(|id, _| new.contains_key(id));
        Arc::make_mut(&mut self.ids).retain(|id, _| new.contains_key(id));
        for n in &source.nodes {
            if old
                .get(&n.id)
                .is_none_or(|v| v.name != n.name || v.kind != n.kind || v.width != n.width)
            {
                let mut label = node_labels(n);
                label.color = self
                    .node_colors
                    .get(n.kind.key())
                    .and_then(|c| design_core::parse_node_color(c));
                self.labels.insert(n.id.clone(), label);
            }
            Arc::make_mut(&mut self.ids)
                .entry(n.id.clone())
                .or_insert_with(|| stable_id("node", &n.id));
        }
        self.source = source.clone();
        Ok(update)
    }
}
