use std::collections::BTreeSet;
use unge_core::*;
use unge_interaction::*;
struct Host {
    editor: Editor,
    interaction: Interaction,
    viewport: Viewport,
    selection: BTreeSet<Id>,
    index: SpatialIndex,
    ids: Vec<Id>,
}
impl Host {
    fn new() -> Self {
        let mut editor = Editor::new(Document::default(), 16).unwrap();
        let mut ids = vec![];
        for x in [20., 300., 580.] {
            let p = Port {
                name: "p".into(),
                data_type: DataType::Float,
                required: false,
                cardinality: Cardinality::Single,
            };
            let n = Node {
                id: Id::new_v4(),
                type_id: "test".into(),
                inputs: vec![p.clone()],
                outputs: vec![p],
                properties: Properties::new(),
            };
            ids.push(n.id);
            editor
                .execute(Command::AddNode {
                    node: n,
                    rect: Rect {
                        x,
                        y: 20.,
                        ..Rect::default()
                    },
                })
                .unwrap();
        }
        let index = SpatialIndex::new(editor.document());
        Self {
            editor,
            interaction: Interaction::default(),
            viewport: Viewport {
                origin: [0., 0.],
                zoom: 1.,
                size: [800., 600.],
            },
            selection: BTreeSet::new(),
            index,
            ids,
        }
    }
    fn event(&mut self, event: PointerEvent) -> InteractionResult<Option<Command>> {
        self.interaction.handle(
            self.editor.document(),
            self.editor.revision(),
            &self.index,
            &mut self.viewport,
            &mut self.selection,
            event,
        )
    }
    fn down(&mut self, position: [f32; 2], additive: bool) {
        self.event(PointerEvent::Down {
            pointer: 1,
            position,
            button: PointerButton::Primary,
            additive,
        })
        .unwrap();
    }
    fn motion(&mut self, position: [f32; 2]) {
        self.event(PointerEvent::Move {
            pointer: 1,
            position,
        })
        .unwrap();
    }
    fn up(&mut self, position: [f32; 2]) -> InteractionResult<Option<Command>> {
        self.event(PointerEvent::Up {
            pointer: 1,
            position,
        })
    }
    fn connect(&mut self, a: usize, b: usize) {
        self.down([200. + a as f32 * 280., 65.], false);
        let command = self.up([20. + b as f32 * 280., 65.]).unwrap().unwrap();
        self.editor.execute(command).unwrap();
    }
}
#[test]
fn drag_is_preview_until_release_then_one_undo_restores_every_selected_node() {
    let mut h = Host::new();
    h.selection.extend([h.ids[0], h.ids[1]]);
    let before = h.editor.document().clone();
    let revision = h.editor.revision();
    h.down([60., 50.], false);
    h.motion([160., 100.]);
    assert_eq!(h.editor.document(), &before);
    assert_eq!(h.editor.revision(), revision);
    assert_eq!(h.interaction.preview().placement[&h.ids[0]].x, 120.);
    assert_eq!(h.interaction.preview().placement[&h.ids[1]].x, 400.);
    let command = h.up([180., 110.]).unwrap().unwrap();
    assert!(h.interaction.preview().placement.is_empty());
    h.editor.execute(command).unwrap();
    assert_eq!(h.editor.revision(), revision + 1);
    assert_eq!(h.editor.document().placement()[&h.ids[1]].x, 420.);
    h.editor.undo().unwrap();
    assert_eq!(h.editor.document(), &before);
}
#[test]
fn jitter_and_return_to_origin_do_not_create_history_and_zoom_scales_drag() {
    let mut h = Host::new();
    h.viewport.zoom = 2.;
    h.viewport.origin = [10., 5.];
    h.down([60., 60.], false);
    assert!(h.up([61., 61.]).unwrap().is_none());
    h.down([60., 60.], false);
    h.motion([160., 100.]);
    assert_eq!(h.interaction.preview().placement[&h.ids[0]].x, 70.);
    assert!(h.up([60., 60.]).unwrap().is_none());
}
#[test]
fn box_and_shift_selection_cancel_restore_and_never_edit_document() {
    let mut h = Host::new();
    let rev = h.editor.revision();
    h.selection.insert(h.ids[2]);
    h.down([0., 0.], true);
    h.motion([500., 130.]);
    assert_eq!(h.selection.len(), 3);
    assert!(h.interaction.preview().marquee.is_some());
    h.event(PointerEvent::Cancel).unwrap();
    assert_eq!(h.selection, BTreeSet::from([h.ids[2]]));
    h.down([500., 130.], false);
    h.up([0., 0.]).unwrap();
    assert_eq!(h.selection, BTreeSet::from([h.ids[0], h.ids[1]]));
    h.down([60., 50.], true);
    h.up([60., 50.]).unwrap();
    assert_eq!(h.selection, BTreeSet::from([h.ids[1]]));
    assert_eq!(h.editor.revision(), rev);
}
#[test]
fn connect_from_either_direction_rejects_occupied_ports_and_cycles() {
    let mut h = Host::new();
    h.connect(0, 1);
    h.down([760., 65.], false);
    h.motion([300., 65.]);
    assert!(!h.interaction.preview().cable.as_ref().unwrap().valid);
    assert!(matches!(
        h.up([300., 65.]),
        Err(InteractionError::Connection)
    ));
    // Reverse gesture: input of C to output of B creates B -> C.
    h.down([580., 65.], false);
    let command = h.up([480., 65.]).unwrap().unwrap();
    h.editor.execute(command).unwrap();
    h.down([760., 65.], false);
    assert!(matches!(
        h.up([20., 65.]),
        Err(InteractionError::Connection)
    ));
    assert_eq!(h.editor.document().graph().edges().len(), 2);
    h.editor.undo().unwrap();
    assert_eq!(h.editor.document().graph().edges().len(), 1);
}
#[test]
fn no_target_same_side_and_hidden_ports_do_not_create_edges() {
    let mut h = Host::new();
    h.down([200., 65.], false);
    assert!(h.up([240., 200.]).unwrap().is_none());
    h.down([200., 65.], false);
    assert!(matches!(
        h.up([480., 65.]),
        Err(InteractionError::Connection)
    ));
    h.viewport.zoom = 0.2;
    h.down([40., 13.], false);
    h.motion([60., 13.]);
    assert!(h.interaction.preview().cable.is_none());
    h.event(PointerEvent::Cancel).unwrap();
    assert!(h.editor.document().graph().edges().is_empty());
}
#[test]
fn ownership_nonfinite_and_concurrent_edit_are_handled_without_partial_commits() {
    let mut h = Host::new();
    h.down([60., 50.], false);
    assert!(matches!(
        h.event(PointerEvent::Move {
            pointer: 2,
            position: [f32::NAN, 70.]
        }),
        Err(InteractionError::PointerBusy)
    ));
    assert!(h.interaction.is_active());
    h.editor
        .execute(Command::RemoveNode { id: h.ids[0] })
        .unwrap();
    assert!(matches!(h.up([160., 50.]), Err(InteractionError::Conflict)));
    assert!(!h.interaction.is_active());
    assert!(h.selection.is_empty());
    h.index = SpatialIndex::new(h.editor.document());
    h.down([350., 50.], false);
    assert!(matches!(
        h.up([f32::NAN, 0.]),
        Err(InteractionError::Invalid)
    ));
    assert!(!h.interaction.is_active());
}
#[test]
fn pan_is_view_only_and_cancel_restores_origin() {
    let mut h = Host::new();
    let revision = h.editor.revision();
    h.viewport.zoom = 2.;
    h.event(PointerEvent::Down {
        pointer: 1,
        position: [50., 50.],
        button: PointerButton::Pan,
        additive: false,
    })
    .unwrap();
    h.motion([90., 70.]);
    assert_eq!(h.viewport.origin, [-20., -10.]);
    h.event(PointerEvent::Cancel).unwrap();
    assert_eq!(h.viewport.origin, [0., 0.]);
    assert_eq!(h.editor.revision(), revision);
}
