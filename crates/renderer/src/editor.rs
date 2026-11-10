use crate::*;
use std::sync::Arc;
use unge_interaction::{Interaction, PointerEvent};
/// Rust-owned view and gestures; returned patches are committed by the host.
pub struct EditorView {
    pub viewport: Viewport,
    pub selection: BTreeSet<Id>,
    pub interaction: Interaction,
    pub revision: u64,
    pub ids: Arc<BTreeMap<String, Id>>,
    cached: Option<retained::RetainedProjection>,
}
impl Default for EditorView {
    fn default() -> Self {
        Self {
            viewport: Viewport {
                origin: [-30., -30.],
                zoom: 0.8,
                size: [1000., 800.],
            },
            selection: BTreeSet::new(),
            interaction: Interaction::default(),
            revision: 0,
            ids: Arc::new(BTreeMap::new()),
            cached: None,
        }
    }
}
impl EditorView {
    pub fn sync_document(&mut self, doc: &Document) -> Result<(), String> {
        if let Some(cached) = &mut self.cached {
            if cached.sync(doc)? != unge_render::SceneUpdate::Unchanged {
                self.ids = cached.ids.clone();
            }
        } else {
            let cached = retained::RetainedProjection::new(doc)?;
            self.ids = cached.ids.clone();
            self.cached = Some(cached);
        }
        Ok(())
    }
    pub fn pointer(
        &mut self,
        doc: &Document,
        revision: u64,
        event: PointerEvent,
    ) -> Result<Vec<design_core::Operation>, String> {
        self.sync_document(doc)?;
        let cached = self.cached.as_ref().unwrap();
        let command = self
            .interaction
            .handle(
                cached.document(),
                revision,
                cached.index.spatial_index(),
                &mut self.viewport,
                &mut self.selection,
                event,
            )
            .map_err(|e| e.to_string())?;
        self.revision = revision;
        let mut operations = vec![];
        if let Some(command) = command {
            self.convert(command, &mut operations)?;
        }
        Ok(operations)
    }
    fn convert(
        &self,
        command: Command,
        out: &mut Vec<design_core::Operation>,
    ) -> Result<(), String> {
        let lookup = |id: Id| {
            self.ids
                .iter()
                .find(|(_, v)| **v == id)
                .map(|(k, _)| k.clone())
                .ok_or("Unknown render node".to_string())
        };
        match command {
            Command::Batch { commands } => {
                for c in commands {
                    self.convert(c, out)?
                }
            }
            Command::MoveNode { id, rect } => out.push(design_core::Operation::UpdateNode {
                id: lookup(id)?,
                name: None,
                kind: None,
                width: None,
                x: Some(rect.x),
                y: Some(rect.y),
            }),
            Command::Connect { edge } => out.push(design_core::Operation::Connect {
                edge: design_core::Edge {
                    id: format!("e_{}", edge.id.simple()),
                    from: lookup(edge.from.node)?,
                    to: lookup(edge.to.node)?,
                    port: edge.to.port,
                },
            }),
            _ => return Err("Unsupported gesture command".into()),
        }
        Ok(())
    }
    pub fn selected(&self) -> Option<String> {
        self.selection.iter().next().and_then(|id| {
            self.ids
                .iter()
                .find(|(_, v)| *v == id)
                .map(|(k, _)| k.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_commits_once_and_conflicts_cancel() {
        let d = Document::demo();
        let mut v = EditorView::default();
        v.viewport.zoom = 1.;
        v.viewport.origin = [0., 0.];
        let down = PointerEvent::Down {
            pointer: 0,
            position: [350., 90.],
            button: unge_interaction::PointerButton::Primary,
            additive: false,
        };
        assert!(v.pointer(&d, 0, down).unwrap().is_empty());
        assert_eq!(v.selected().as_deref(), Some("add"));
        assert!(v
            .pointer(
                &d,
                0,
                PointerEvent::Move {
                    pointer: 0,
                    position: [390., 110.]
                }
            )
            .unwrap()
            .is_empty());
        let ops = v
            .pointer(
                &d,
                0,
                PointerEvent::Up {
                    pointer: 0,
                    position: [390., 110.],
                },
            )
            .unwrap();
        assert_eq!(ops.len(), 1);
        let changed = design_core::apply(&d, &ops).unwrap();
        assert_eq!(changed.node("add").unwrap().x, 350.);
        v.pointer(&d, 0, down).unwrap();
        assert!(v
            .pointer(
                &d,
                1,
                PointerEvent::Up {
                    pointer: 0,
                    position: [390., 110.]
                }
            )
            .is_err());
    }
}
