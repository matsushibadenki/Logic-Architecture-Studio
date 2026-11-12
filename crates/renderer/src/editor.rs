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
        self.interaction.set_grid_snap(true);
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
        if self.selection.len() > 512 {
            self.interaction
                .cancel(&mut self.viewport, &mut self.selection);
            return Err("Select at most 512 nodes".into());
        }
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
    pub fn selected_ids(&self) -> Vec<String> {
        self.ids
            .iter()
            .filter(|(_, render_id)| self.selection.contains(render_id))
            .map(|(id, _)| id.clone())
            .collect()
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
    fn grid_snap_matches_preview_across_zoom_pan_and_negative_coordinates() {
        for (zoom, xy, delta, expected) in [
            (1., [40., 80.], [37., 19.], [72., 96.]),
            (0.8, [-37., -51.], [8., 9.], [-24., -48.]),
            (1.6, [-37., -51.], [1., 15.], [-24., -24.]),
            (0.2, [40., 80.], [37., 19.], [120., 120.]),
        ] {
            let mut d = Document::demo();
            d.nodes
                .update("a", |n| {
                    n.x = xy[0];
                    n.y = xy[1];
                })
                .unwrap();
            let mut v = EditorView::default();
            v.viewport.zoom = zoom;
            v.viewport.origin = [-200., -180.];
            let point = |p: [f32; 2]| [(p[0] + 200.) * zoom, (p[1] + 180.) * zoom];
            let start = point([xy[0] + 60., xy[1] + 20.]);
            let end = [start[0] + delta[0] * zoom, start[1] + delta[1] * zoom];
            let down = PointerEvent::Down {
                pointer: 0,
                position: start,
                button: unge_interaction::PointerButton::Primary,
                additive: false,
            };
            v.pointer(&d, 0, down).unwrap();
            // Selection alone must never reposition an off-grid node.
            assert!(v
                .pointer(
                    &d,
                    0,
                    PointerEvent::Up {
                        pointer: 0,
                        position: start
                    }
                )
                .unwrap()
                .is_empty());
            v.pointer(&d, 0, down).unwrap();
            v.pointer(
                &d,
                0,
                PointerEvent::Move {
                    pointer: 0,
                    position: end,
                },
            )
            .unwrap();
            let preview = v.interaction.preview().placement[&v.ids["a"]];
            assert!((preview.x - expected[0]).abs() < 0.001);
            assert!((preview.y - expected[1]).abs() < 0.001);
            let ops = v
                .pointer(
                    &d,
                    0,
                    PointerEvent::Up {
                        pointer: 0,
                        position: end,
                    },
                )
                .unwrap();
            let moved = design_core::apply(&d, &ops).unwrap();
            assert_eq!(node_rect(moved.node("a").unwrap()), preview);
            v.pointer(&d, 0, down).unwrap();
            v.pointer(
                &d,
                0,
                PointerEvent::Move {
                    pointer: 0,
                    position: end,
                },
            )
            .unwrap();
            assert!(v.pointer(&d, 0, PointerEvent::Cancel).unwrap().is_empty());
            assert!(v.interaction.preview().placement.is_empty());
        }
    }
    #[test]
    fn movement_inside_same_grid_cell_has_no_commit() {
        let mut d = Document::demo();
        d.nodes
            .update("a", |n| {
                n.x = 48.;
                n.y = 96.;
            })
            .unwrap();
        let mut v = EditorView::default();
        v.viewport.zoom = 1.;
        v.viewport.origin = [0., 0.];
        v.pointer(
            &d,
            0,
            PointerEvent::Down {
                pointer: 0,
                position: [100., 116.],
                button: unge_interaction::PointerButton::Primary,
                additive: false,
            },
        )
        .unwrap();
        assert!(v
            .pointer(
                &d,
                0,
                PointerEvent::Up {
                    pointer: 0,
                    position: [105., 120.]
                }
            )
            .unwrap()
            .is_empty());
    }
    #[test]
    fn additive_and_marquee_selection_move_as_one_patch() {
        let d = Document::demo();
        let mut v = EditorView::default();
        v.viewport.zoom = 1.;
        v.viewport.origin = [0., 0.];
        let click = |v: &mut EditorView, p, additive| {
            v.pointer(
                &d,
                0,
                PointerEvent::Down {
                    pointer: 0,
                    position: p,
                    button: unge_interaction::PointerButton::Primary,
                    additive,
                },
            )
            .unwrap();
            v.pointer(
                &d,
                0,
                PointerEvent::Up {
                    pointer: 0,
                    position: p,
                },
            )
            .unwrap();
        };
        click(&mut v, [70., 100.], false);
        click(&mut v, [70., 420.], true);
        assert_eq!(v.selected_ids(), vec!["a", "b"]);
        v.pointer(
            &d,
            0,
            PointerEvent::Down {
                pointer: 0,
                position: [70., 100.],
                button: unge_interaction::PointerButton::Primary,
                additive: false,
            },
        )
        .unwrap();
        v.pointer(
            &d,
            0,
            PointerEvent::Move {
                pointer: 0,
                position: [110., 130.],
            },
        )
        .unwrap();
        let ops = v
            .pointer(
                &d,
                0,
                PointerEvent::Up {
                    pointer: 0,
                    position: [110., 130.],
                },
            )
            .unwrap();
        assert_eq!(ops.len(), 2);
        let moved = design_core::apply(&d, &ops).unwrap();
        for id in ["a", "b"] {
            assert_eq!(moved.node(id).unwrap().x, d.node(id).unwrap().x + 32.);
            assert_eq!(moved.node(id).unwrap().y, d.node(id).unwrap().y + 40.);
        }
        assert_eq!(moved.node("add").unwrap(), d.node("add").unwrap());
        click(&mut v, [70., 420.], true);
        assert_eq!(v.selected_ids(), vec!["a"]);
        v.pointer(
            &d,
            0,
            PointerEvent::Down {
                pointer: 0,
                position: [20., 50.],
                button: unge_interaction::PointerButton::Primary,
                additive: false,
            },
        )
        .unwrap();
        v.pointer(
            &d,
            0,
            PointerEvent::Move {
                pointer: 0,
                position: [350., 700.],
            },
        )
        .unwrap();
        v.pointer(
            &d,
            0,
            PointerEvent::Up {
                pointer: 0,
                position: [350., 700.],
            },
        )
        .unwrap();
        assert_eq!(v.selected_ids(), vec!["a", "b"]);
    }
    #[test]
    fn drag_commits_once_and_conflicts_cancel() {
        let d = Document::demo();
        let mut v = EditorView::default();
        v.viewport.zoom = 1.;
        v.viewport.origin = [0., 0.];
        let down = PointerEvent::Down {
            pointer: 0,
            position: [440., 90.],
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
                    position: [480., 110.]
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
                    position: [480., 110.],
                },
            )
            .unwrap();
        assert_eq!(ops.len(), 1);
        let changed = design_core::apply(&d, &ops).unwrap();
        assert_eq!(changed.node("add").unwrap().x, 432.);
        v.pointer(&d, 0, down).unwrap();
        assert!(v
            .pointer(
                &d,
                1,
                PointerEvent::Up {
                    pointer: 0,
                    position: [480., 110.]
                }
            )
            .is_err());
    }
    #[test]
    fn native_preview_rejects_bus_width_mismatch_before_commit() {
        let mut doc = Document::demo();
        doc.edges
            .retain(|edge| !(edge.to == "select" && edge.port == "sel"));
        for (source, compatible) in [("a", false), ("op", true)] {
            let mut view = EditorView::default();
            view.viewport.zoom = 1.;
            view.viewport.origin = [0., 0.];
            let a = doc.node(source).unwrap();
            let mux = doc.node("select").unwrap();
            let from = unge_core::port_anchor(node_rect(a), 0, 1, true);
            let to = unge_core::port_anchor(node_rect(mux), 3, 4, false);
            view.pointer(
                &doc,
                0,
                PointerEvent::Down {
                    pointer: 0,
                    position: from,
                    button: unge_interaction::PointerButton::Primary,
                    additive: false,
                },
            )
            .unwrap();
            view.pointer(
                &doc,
                0,
                PointerEvent::Move {
                    pointer: 0,
                    position: to,
                },
            )
            .unwrap();
            assert_eq!(
                view.interaction.preview().cable.as_ref().unwrap().valid,
                compatible
            );
            let result = view.pointer(
                &doc,
                0,
                PointerEvent::Up {
                    pointer: 0,
                    position: to,
                },
            );
            assert_eq!(result.is_ok(), compatible);
            if let Ok(operations) = result {
                assert_eq!(operations.len(), 1);
                design_core::apply(&doc, &operations).unwrap();
            }
        }
    }
    #[test]
    fn native_feedback_reconnect_stops_cycle_search_at_registers() {
        let doc = Document {
            name: "feedback".into(),
            nodes: vec![
                design_core::Node {
                    id: "sum".into(),
                    name: "sum".into(),
                    kind: Kind::Add,
                    width: 3,
                    x: 100.,
                    y: 100.,
                },
                design_core::Node {
                    id: "acc".into(),
                    name: "acc".into(),
                    kind: Kind::Register,
                    width: 3,
                    x: 500.,
                    y: 100.,
                },
            ]
            .into(),
            edges: vec![
                design_core::Edge {
                    id: "feedback".into(),
                    from: "acc".into(),
                    to: "sum".into(),
                    port: "b".into(),
                },
                design_core::Edge {
                    id: "data".into(),
                    from: "sum".into(),
                    to: "acc".into(),
                    port: "d".into(),
                },
            ]
            .into(),
        };
        for (remove, source, target, input, valid) in [
            ("data", "sum", "acc", 0, true),
            ("feedback", "acc", "sum", 1, true),
            ("data", "acc", "acc", 0, true),
            ("feedback", "sum", "sum", 1, false),
        ] {
            let mut d = doc.clone();
            d.edges.retain(|e| e.id != remove);
            let mut view = EditorView::default();
            view.viewport.zoom = 1.;
            view.viewport.origin = [0., 0.];
            let from = unge_core::port_anchor(node_rect(d.node(source).unwrap()), 0, 1, true);
            let target_node = d.node(target).unwrap();
            let to = unge_core::port_anchor(
                node_rect(target_node),
                input,
                target_node.inputs().len(),
                false,
            );
            view.pointer(
                &d,
                0,
                PointerEvent::Down {
                    pointer: 0,
                    position: from,
                    button: unge_interaction::PointerButton::Primary,
                    additive: false,
                },
            )
            .unwrap();
            view.pointer(
                &d,
                0,
                PointerEvent::Move {
                    pointer: 0,
                    position: to,
                },
            )
            .unwrap();
            assert_eq!(
                view.interaction.preview().cable.as_ref().unwrap().valid,
                valid
            );
            let result = view.pointer(
                &d,
                0,
                PointerEvent::Up {
                    pointer: 0,
                    position: to,
                },
            );
            assert_eq!(result.is_ok(), valid);
            if let Ok(ops) = result {
                design_core::apply(&d, &ops).unwrap();
            }
        }
    }
}
