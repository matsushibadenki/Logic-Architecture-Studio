use crate::{LabelCatalog, TextLabel, Theme, labels::label_text};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use unge_core::{Command, Document, Id, Locale, Rect, SpatialIndex, Viewport, port_anchor};
use unge_interaction::{MAX_SELECTION, Preview};

const MAX_MOVED_NODES: usize = 1024;
const MAX_REFIT_EDGES: usize = 4096;
const NODE_CORNER_RADIUS: f32 = 10.0;
const SYMBOL_SIZE: f32 = 52.0;

// Spatial queries already return ID order. Retain that order without rebuilding
// a tree of individual allocations; preview IDs may overlap or arrive unsorted.
fn visible_ids(mut ids: Vec<Id>, extra: impl Iterator<Item = Id>) -> Vec<Id> {
    let count = ids.len();
    ids.extend(extra);
    if ids.len() != count {
        ids.sort_unstable();
    }
    ids.dedup();
    ids
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub params: [f32; 4],
}
impl Quad {
    fn rectangle(rect: Rect, color: [f32; 4], radius: f32) -> Self {
        Self {
            rect: [
                rect.x + rect.width / 2.0,
                rect.y + rect.height / 2.0,
                rect.width,
                rect.height,
            ],
            color,
            params: [0.0, radius, 0.0, 0.0],
        }
    }
    fn line(a: [f32; 2], b: [f32; 2], color: [f32; 4]) -> Self {
        Self::stroke(a, b, color, 3.0)
    }
    fn stroke(a: [f32; 2], b: [f32; 2], color: [f32; 4], width: f32) -> Self {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        Self {
            rect: [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                dx.hypot(dy).max(0.001) + width,
                width,
            ],
            color,
            params: [dy.atan2(dx), width * 0.5, -1.0, 0.0],
        }
    }
}
struct RenderNode {
    rect: Rect,
    inputs: usize,
    outputs: usize,
    type_id: String,
    input_names: Vec<String>,
    required_inputs: Vec<usize>,
    output_names: Vec<String>,
}
#[derive(Clone)]
struct RenderEdge {
    points: Vec<[f32; 2]>,
    bridges: Vec<[f32; 2]>,
    blocked: bool,
    bounds: Rect,
    source_type: String,
    signal_width: Option<u8>,
    from: (Id, usize),
    to: (Id, usize),
}
impl RenderNode {
    fn from_document(doc: &Document, id: Id) -> Option<Self> {
        let node = doc.graph().nodes().get(&id)?;
        Some(Self {
            rect: doc.placement().get(&id).copied().unwrap_or_default(),
            inputs: node.inputs.len(),
            outputs: node.outputs.len(),
            type_id: node.type_id.clone(),
            input_names: node.inputs.iter().map(|p| p.name.clone()).collect(),
            required_inputs: node
                .inputs
                .iter()
                .enumerate()
                .filter_map(|(index, port)| port.required.then_some(index))
                .collect(),
            output_names: node.outputs.iter().map(|p| p.name.clone()).collect(),
        })
    }
}
impl RenderEdge {
    fn from_document(doc: &Document, id: Id) -> Option<Self> {
        let edge = doc.graph().edges().get(&id)?;
        let from = doc.graph().nodes().get(&edge.from.node)?;
        let to = doc.graph().nodes().get(&edge.to.node)?;
        let i = from.outputs.iter().position(|p| p.name == edge.from.port)?;
        let j = to.inputs.iter().position(|p| p.name == edge.to.port)?;
        let from_rect = doc.placement().get(&from.id).copied().unwrap_or_default();
        let to_rect = doc.placement().get(&to.id).copied().unwrap_or_default();
        let points = [
            port_anchor(from_rect, i, from.outputs.len(), true),
            port_anchor(to_rect, j, to.inputs.len(), false),
        ];
        Some(Self {
            points: points.to_vec(),
            bridges: Vec::new(),
            blocked: false,
            bounds: route_bounds(&points),
            source_type: from.type_id.clone(),
            signal_width: match from.outputs[i].data_type {
                unge_core::DataType::Bits { width, .. } => Some(width),
                _ => None,
            },
            from: (from.id, i),
            to: (to.id, j),
        })
    }
}
type NodeCacheKey = (Rect, bool, u32, u8, Theme, Option<crate::NodeLabels>, bool);
struct CachedNode {
    key: NodeCacheKey,
    quads: Vec<Quad>,
    labels: Vec<TextLabel>,
}
struct CachedEdge {
    points: Vec<[f32; 2]>,
    bridges: Vec<[f32; 2]>,
    zoom: f32,
    color: [f32; 4],
    width: u8,
    quads: Vec<Quad>,
}
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct GeometryStats {
    pub rebuilt_nodes: usize,
    pub reused_nodes: usize,
    pub rebuilt_edges: usize,
    pub reused_edges: usize,
}
/// Retained geometry. Small committed edits update bounded BVH overlays.
pub struct SceneIndex {
    nodes: BTreeMap<Id, RenderNode>,
    edges: BTreeMap<Id, RenderEdge>,
    node_index: SpatialIndex,
    edge_index: SpatialIndex,
    incident: BTreeMap<Id, BTreeSet<Id>>,
    groups: crate::groups::GroupIndex,
    node_cache: RefCell<BTreeMap<Id, CachedNode>>,
    edge_cache: RefCell<BTreeMap<Id, CachedEdge>>,
}
/// Capture before consuming a Command; apply only after that edit commits.
#[derive(Debug, Default)]
pub struct SceneChanges {
    nodes: BTreeSet<Id>,
    edges: BTreeSet<Id>,
    rebuild: bool,
    groups_changed: bool,
    geometry_only: bool,
}
impl SceneChanges {
    pub fn from_command(command: &Command) -> Self {
        fn collect(command: &Command, changes: &mut SceneChanges) {
            if changes.rebuild {
                return;
            }
            match command {
                Command::MoveNode { id, .. } | Command::RemoveNode { id } => {
                    changes.nodes.insert(*id);
                    if matches!(command, Command::RemoveNode { .. }) {
                        changes.geometry_only = false;
                    }
                }
                Command::AddNode { node, .. } => {
                    changes.geometry_only = false;
                    changes.nodes.insert(node.id);
                }
                Command::Connect { edge } => {
                    changes.geometry_only = false;
                    changes.edges.insert(edge.id);
                }
                Command::Disconnect { id } => {
                    changes.geometry_only = false;
                    changes.edges.insert(*id);
                }
                Command::SetProperty { .. } => {}
                Command::SetGroup { .. } => changes.groups_changed = true,
                Command::Batch { commands } => {
                    for command in commands {
                        collect(command, changes);
                        if changes.rebuild {
                            break;
                        }
                    }
                }
            }
            let node_limit = if changes.geometry_only {
                MAX_MOVED_NODES
            } else {
                128
            };
            if changes.nodes.len() > node_limit || changes.edges.len() > 128 {
                changes.rebuild = true;
                changes.nodes.clear();
                changes.edges.clear();
            }
        }
        let mut changes = Self {
            geometry_only: true,
            ..Self::default()
        };
        collect(command, &mut changes);
        changes
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneUpdate {
    Unchanged,
    Incremental,
    Rebuilt,
}

/// Explicit CPU diagnostics; excludes upload, GPU, IPC and shared locks.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SceneTimings {
    pub groups_ms: f64,
    pub edge_query_ms: f64,
    pub edge_geometry_ms: f64,
    pub node_query_ms: f64,
    pub node_geometry_ms: f64,
    pub overlays_ms: f64,
    pub total_ms: f64,
}
fn elapsed_ms(start: Option<Instant>) -> f64 {
    start.map_or(0., |s| s.elapsed().as_secs_f64() * 1000.)
}

#[derive(Debug)]
pub struct Scene {
    /// Linear RGBA, matching the target's GPU colour space.
    pub background: [f32; 4],
    pub grid_color: [f32; 4],
    pub quads: Vec<Quad>,
    pub labels: Vec<TextLabel>,
    pub visible_nodes: usize,
    pub visible_edges: usize,
    pub visible_groups: usize,
    pub geometry_stats: GeometryStats,
}
impl Default for Scene {
    fn default() -> Self {
        let palette = Theme::Dark.palette();
        Self {
            background: palette.background.linear(),
            grid_color: palette.grid.linear(),
            quads: Vec::new(),
            labels: Vec::new(),
            visible_nodes: 0,
            visible_edges: 0,
            visible_groups: 0,
            geometry_stats: GeometryStats::default(),
        }
    }
}
fn curve(a: [f32; 2], b: [f32; 2]) -> [[f32; 2]; 4] {
    let dx = ((b[0] - a[0]).abs() * 0.5).max(50.0);
    [a, [a[0] + dx, a[1]], [b[0] - dx, b[1]], b]
}
fn route_bounds(points: &[[f32; 2]]) -> Rect {
    let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min) - 2.0;
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        + 2.0;
    let min_y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min) - 2.0;
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        + 2.0;
    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}
// One stroke policy for nets, internal leads, gate outlines and previews.
fn wire_width(zoom: f32) -> f32 {
    3.0_f32.max(1.5 / zoom)
}
fn expanded(rect: Rect, padding: f32) -> Rect {
    Rect {
        x: rect.x - padding,
        y: rect.y - padding,
        width: rect.width + padding * 2.0,
        height: rect.height + padding * 2.0,
    }
}
fn push_curve(scene: &mut Scene, points: [[f32; 2]; 4], zoom: f32, color: [f32; 4]) {
    crate::curves::for_each_segment(points, zoom, |a, b| {
        scene
            .quads
            .push(Quad::stroke(a, b, color, wire_width(zoom)));
    });
}
impl SceneIndex {
    fn route_edges(
        nodes: &BTreeMap<Id, RenderNode>,
        edges: &mut BTreeMap<Id, RenderEdge>,
        placement: &BTreeMap<Id, Rect>,
    ) {
        let rect = |id: &Id| placement.get(id).copied().unwrap_or(nodes[id].rect);
        let obstacles = nodes
            .keys()
            .map(|id| {
                let r = rect(id);
                studio_routing::Obstacle {
                    id: id.as_u128(),
                    rect: [r.x, r.y, r.width, r.height],
                }
            })
            .collect();
        let links: Vec<_> = edges
            .iter()
            .map(|(id, e)| studio_routing::Link {
                id: id.as_u128(),
                source: (e.from.0.as_u128(), e.from.1),
                target: (e.to.0.as_u128(), e.to.1),
                start: port_anchor(rect(&e.from.0), e.from.1, nodes[&e.from.0].outputs, true),
                end: port_anchor(rect(&e.to.0), e.to.1, nodes[&e.to.0].inputs, false),
            })
            .collect();
        let mut routes = studio_routing::Router::new(obstacles).route_all(&links);
        for link in links {
            let route = routes.remove(&link.id).unwrap();
            let edge = edges.get_mut(&Id::from_u128(link.id)).unwrap();
            edge.bounds = if route.blocked {
                route_bounds(&[link.start, link.end])
            } else {
                expanded(route_bounds(&route.points), 8.)
            };
            edge.points = route.points;
            edge.bridges = route.bridges;
            edge.blocked = route.blocked;
        }
    }
    fn reroute(&mut self) {
        Self::route_edges(&self.nodes, &mut self.edges, &BTreeMap::new());
        self.edge_index =
            SpatialIndex::from_rects(self.edges.iter().map(|(id, e)| (*id, e.bounds)).collect());
    }
    pub fn new(doc: &Document) -> Self {
        let nodes = doc
            .graph()
            .nodes()
            .keys()
            .filter_map(|id| RenderNode::from_document(doc, *id).map(|n| (*id, n)))
            .collect();
        let mut edges = BTreeMap::new();
        let mut bounds = Vec::new();
        let mut incident: BTreeMap<Id, BTreeSet<Id>> = BTreeMap::new();
        for id in doc.graph().edges().keys() {
            let Some(edge) = RenderEdge::from_document(doc, *id) else {
                continue;
            };
            bounds.push((*id, edge.bounds));
            for node in [edge.from.0, edge.to.0] {
                incident.entry(node).or_default().insert(*id);
            }
            edges.insert(*id, edge);
        }

        let mut index = Self {
            nodes,
            edges,
            incident,
            node_index: SpatialIndex::new(doc),
            edge_index: SpatialIndex::from_rects(bounds),
            groups: crate::groups::GroupIndex::new(doc),
            node_cache: RefCell::new(BTreeMap::new()),
            edge_cache: RefCell::new(BTreeMap::new()),
        };
        index.reroute();
        index
    }
    /// The index must correspond to the state immediately before the captured
    /// command. Document validation and revision checks remain the host's job.
    pub fn update(&mut self, doc: &Document, changes: &SceneChanges) -> SceneUpdate {
        if changes.geometry_only
            && !changes.rebuild
            && changes.edges.is_empty()
            && !changes.nodes.is_empty()
        {
            let rects: BTreeMap<_, _> = changes
                .nodes
                .iter()
                .filter_map(|id| doc.placement().get(id).map(|rect| (*id, *rect)))
                .collect();
            if rects.len() == changes.nodes.len()
                && rects.keys().all(|id| self.nodes.contains_key(id))
                && (self.node_index.update_rects(&rects) || self.node_index.refit_rects(&rects))
            {
                for (id, rect) in rects {
                    self.nodes.get_mut(&id).unwrap().rect = rect;
                    self.node_cache.get_mut().remove(&id);
                    for edge in self.incident.get(&id).into_iter().flatten() {
                        self.edge_cache.get_mut().remove(edge);
                    }
                }
                if changes.groups_changed || !doc.graph().groups().is_empty() {
                    self.groups = crate::groups::GroupIndex::new(doc);
                }
                // Routing considers all obstacles, including moves unrelated to an endpoint.
                // The final edge index is rebuilt once, from the routed bounds.
                self.reroute();
                return SceneUpdate::Incremental;
            }
        }
        let mut edge_ids = changes.edges.clone();
        let edge_limit = if changes.geometry_only {
            MAX_REFIT_EDGES
        } else {
            128
        };
        if !changes.rebuild {
            for id in &changes.nodes {
                for edge in self.incident.get(id).into_iter().flatten() {
                    edge_ids.insert(*edge);
                    if edge_ids.len() > edge_limit {
                        break;
                    }
                }
                if edge_ids.len() > edge_limit {
                    break;
                }
            }
        }
        if changes.rebuild || edge_ids.len() > edge_limit {
            *self = Self::new(doc);
            return SceneUpdate::Rebuilt;
        }
        if changes.nodes.is_empty() && edge_ids.is_empty() {
            if changes.groups_changed {
                self.groups = crate::groups::GroupIndex::new(doc);
                return SceneUpdate::Incremental;
            }
            return SceneUpdate::Unchanged;
        }
        // Read final committed geometry. This handles removal/recreation of the
        // same ID in a Batch, including changed ports and implicit edge deletion.
        let nodes: BTreeMap<_, _> = changes
            .nodes
            .iter()
            .map(|id| (*id, RenderNode::from_document(doc, *id)))
            .collect();
        let edges: BTreeMap<_, _> = edge_ids
            .iter()
            .map(|id| (*id, RenderEdge::from_document(doc, *id)))
            .collect();
        // Only existing geometry can use refitting. Keep the sparse overlay and
        // structural-edit behavior; bulk moves update bounds in retained trees.
        let bulk_move = changes.geometry_only && (nodes.len() > 128 || edges.len() > 128);
        let updated = if bulk_move {
            match (
                nodes
                    .iter()
                    .map(|(id, n)| n.as_ref().map(|n| (*id, n.rect)))
                    .collect::<Option<BTreeMap<_, _>>>(),
                edges
                    .iter()
                    .map(|(id, e)| e.as_ref().map(|e| (*id, e.bounds)))
                    .collect::<Option<BTreeMap<_, _>>>(),
            ) {
                (Some(rects), Some(bounds)) => {
                    self.node_index.refit_rects(&rects) && self.edge_index.refit_rects(&bounds)
                }
                _ => false,
            }
        } else {
            let rects = nodes
                .iter()
                .map(|(id, node)| (*id, node.as_ref().map(|n| n.rect)))
                .collect();
            let bounds = edges
                .iter()
                .map(|(id, edge)| (*id, edge.as_ref().map(|e| e.bounds)))
                .collect();
            self.node_index.update_entries(&rects) && self.edge_index.update_entries(&bounds)
        };
        if !updated {
            *self = Self::new(doc);
            return SceneUpdate::Rebuilt;
        }
        for id in &changes.nodes {
            self.node_cache.get_mut().remove(id);
        }
        for id in &edge_ids {
            self.edge_cache.get_mut().remove(id);
        }
        // Remove old incidence before installing final endpoints: rewiring an ID
        // must not leave stale references that break a later move or deletion.
        // Moves keep endpoints/port membership unchanged.
        for id in edge_ids.iter().filter(|_| !changes.geometry_only) {
            if let Some(old) = self.edges.remove(id) {
                for node in [old.from.0, old.to.0] {
                    if let Some(set) = self.incident.get_mut(&node) {
                        set.remove(id);
                        if set.is_empty() {
                            self.incident.remove(&node);
                        }
                    }
                }
            }
        }
        for (id, node) in nodes {
            if let Some(node) = node {
                self.nodes.insert(id, node);
            } else {
                self.nodes.remove(&id);
                self.incident.remove(&id);
            }
        }
        for (id, edge) in edges {
            if let Some(edge) = edge {
                if !changes.geometry_only {
                    for node in [edge.from.0, edge.to.0] {
                        self.incident.entry(node).or_default().insert(id);
                    }
                }
                self.edges.insert(id, edge);
            }
        }
        if !changes.nodes.is_empty() || changes.groups_changed {
            self.groups = crate::groups::GroupIndex::new(doc);
        }
        self.reroute();
        SceneUpdate::Incremental
    }
    pub fn scene(&self, viewport: Viewport, selection: &BTreeSet<Id>) -> unge_core::Result<Scene> {
        self.scene_with_preview(viewport, selection, &Preview::default())
    }
    pub fn spatial_index(&self) -> &SpatialIndex {
        &self.node_index
    }
    /// Reuse committed BVHs; only moved nodes and incident edges bypass old bounds.
    pub fn scene_with_preview(
        &self,
        viewport: Viewport,
        selection: &BTreeSet<Id>,
        preview: &Preview,
    ) -> unge_core::Result<Scene> {
        self.scene_with_labels(
            viewport,
            selection,
            preview,
            &LabelCatalog::new(),
            Locale::En,
        )
    }
    pub fn scene_with_labels(
        &self,
        viewport: Viewport,
        selection: &BTreeSet<Id>,
        preview: &Preview,
        catalog: &LabelCatalog,
        locale: Locale,
    ) -> unge_core::Result<Scene> {
        self.scene_with_theme(viewport, selection, preview, catalog, locale, Theme::Dark)
    }
    /// Reuse geometry indexes while resolving view-local presentation colours.
    pub fn scene_with_theme(
        &self,
        viewport: Viewport,
        selection: &BTreeSet<Id>,
        preview: &Preview,
        catalog: &LabelCatalog,
        locale: Locale,
        theme: Theme,
    ) -> unge_core::Result<Scene> {
        self.generate::<false>(viewport, selection, preview, catalog, locale, theme)
            .map(|(scene, _)| scene)
    }
    /// Same scene as scene_with_theme; normal calls do not read the clock.
    pub fn scene_with_theme_timed(
        &self,
        viewport: Viewport,
        selection: &BTreeSet<Id>,
        preview: &Preview,
        catalog: &LabelCatalog,
        locale: Locale,
        theme: Theme,
    ) -> unge_core::Result<(Scene, SceneTimings)> {
        self.generate::<true>(viewport, selection, preview, catalog, locale, theme)
    }
    fn generate<const TIMED: bool>(
        &self,
        viewport: Viewport,
        selection: &BTreeSet<Id>,
        preview: &Preview,
        catalog: &LabelCatalog,
        locale: Locale,
        theme: Theme,
    ) -> unge_core::Result<(Scene, SceneTimings)> {
        let total = TIMED.then(Instant::now);
        let mut timings = SceneTimings::default();
        viewport.validate()?;
        if preview.placement.len() > MAX_SELECTION
            || preview
                .placement
                .iter()
                .any(|(id, rect)| !self.nodes.contains_key(id) || !rect.valid())
            || preview.marquee.is_some_and(|rect| !rect.valid())
            || preview.cable.as_ref().is_some_and(|c| {
                c.from
                    .iter()
                    .chain(c.to.iter())
                    .any(|v| !v.is_finite() || v.abs() > 1.0e9)
            })
        {
            return Err(unge_core::Error::Invalid(
                "invalid interaction preview".into(),
            ));
        }
        let area = viewport.world_rect();
        let palette = theme.palette();
        let mut scene = Scene {
            background: palette.background.linear(),
            grid_color: palette.grid.linear(),
            ..Scene::default()
        };
        let mut grid = Quad::rectangle(area, scene.background, 0.0);
        grid.params[2] = 1.0;
        grid.params[3] = unge_core::grid_spacing(viewport.zoom);
        scene.quads.push(grid);
        let start = TIMED.then(Instant::now);
        for (rect, label) in self.groups.visible(area, preview, |id| {
            self.nodes.get(&id).map(|node| node.rect)
        }) {
            scene
                .quads
                .push(Quad::rectangle(rect, palette.border.linear(), 10.0));
            let background = [0, 1, 2, 3]
                .map(|i| (palette.background.linear()[i] + palette.node.linear()[i]) * 0.5);
            scene.quads.push(Quad::rectangle(
                Rect {
                    x: rect.x + 1.5,
                    y: rect.y + 1.5,
                    width: rect.width - 3.0,
                    height: rect.height - 3.0,
                },
                background,
                8.5,
            ));
            if viewport.zoom >= 0.6 && !label.is_empty() {
                scene.labels.push(TextLabel {
                    text: label.to_owned(),
                    rect: Rect {
                        x: rect.x + 12.0,
                        y: rect.y + 6.0,
                        width: rect.width - 24.0,
                        height: 22.0,
                    },
                    font_size: if rect.height >= 120.0 { 15.0 } else { 14.0 },
                    right_aligned: false,
                    centered: false,
                    bold: false,
                    color: palette.text.linear(),
                    after_quad: scene.quads.len(),
                });
            }
            scene.visible_groups += 1;
        }
        timings.groups_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        let cable_area = expanded(area, wire_width(viewport.zoom) * 0.5 + 1.0 / viewport.zoom);
        let preview_edges = (!preview.placement.is_empty()).then(|| {
            let mut edges = self.edges.clone();
            Self::route_edges(&self.nodes, &mut edges, &preview.placement);
            edges
        });
        // A moved obstacle can displace a wire that has no endpoint on it.
        let edge_map = preview_edges.as_ref().unwrap_or(&self.edges);
        let edges: Vec<_> = if preview_edges.is_some() {
            edge_map
                .iter()
                .filter_map(|(id, e)| e.bounds.intersects(cable_area).then_some(*id))
                .collect()
        } else {
            self.edge_index.query(cable_area)
        };
        timings.edge_query_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        let mut blocked_ports = BTreeSet::new();
        for id in edges {
            let edge = &edge_map[&id];
            let points = &edge.points;
            let bounds = edge.bounds;
            if !bounds.intersects(cable_area) {
                continue;
            }
            if edge.blocked {
                for (node_id, port, output) in [
                    (edge.from.0, edge.from.1, true),
                    (edge.to.0, edge.to.1, false),
                ] {
                    // Multiple blocked branches from the same port share one warning.
                    if !blocked_ports.insert((node_id, port, output)) {
                        continue;
                    }
                    let node = &self.nodes[&node_id];
                    let rect = preview
                        .placement
                        .get(&node_id)
                        .copied()
                        .unwrap_or(node.rect);
                    let anchor = port_anchor(
                        rect,
                        port,
                        if output { node.outputs } else { node.inputs },
                        output,
                    );
                    scene.labels.push(TextLabel {
                        text: "!".into(),
                        rect: Rect {
                            x: anchor[0] + if output { 4. } else { -18. },
                            y: anchor[1] - 24.,
                            width: 16.,
                            height: 24.,
                        },
                        font_size: 20.,
                        right_aligned: false,
                        centered: true,
                        bold: true,
                        color: palette.cable_invalid.linear(),
                        after_quad: scene.quads.len(),
                    });
                }
            }
            let signal = catalog.get(&edge.source_type);
            let width = edge
                .signal_width
                .unwrap_or_else(|| signal.map_or(1, |label| label.signal_width))
                .max(1);
            let color = catalog
                .get(&edge.source_type)
                .filter(|labels| labels.color.is_some() || labels.tone != crate::NodeTone::Neutral)
                .map_or(palette.edge, |labels| {
                    labels
                        .color
                        .map(crate::ThemeColor::new)
                        .unwrap_or_else(|| palette.tone(labels.tone))
                });
            let color = color.linear();
            let cache = self.edge_cache.borrow();
            if let Some(cached) = cache.get(&id).filter(|c| {
                c.points == *points
                    && c.bridges == edge.bridges
                    && c.zoom == viewport.zoom
                    && c.color == color
                    && c.width == width
            }) {
                scene.quads.extend_from_slice(&cached.quads);
                scene.geometry_stats.reused_edges += 1;
            } else {
                drop(cache);
                let start = scene.quads.len();
                let bridged;
                let display = if edge.bridges.is_empty() {
                    points.as_slice()
                } else {
                    bridged = studio_routing::Route {
                        points: points.clone(),
                        bridges: edge.bridges.clone(),
                        blocked: edge.blocked,
                    }
                    .display_points();
                    &bridged
                };
                for segment in display.windows(2) {
                    if segment[0] != segment[1] {
                        scene.quads.push(Quad::stroke(
                            segment[0],
                            segment[1],
                            color,
                            wire_width(viewport.zoom),
                        ));
                    }
                }
                self.edge_cache.borrow_mut().insert(
                    id,
                    CachedEdge {
                        points: points.clone(),
                        bridges: edge.bridges.clone(),
                        zoom: viewport.zoom,
                        color,
                        width,
                        quads: scene.quads[start..].to_vec(),
                    },
                );
                scene.geometry_stats.rebuilt_edges += 1;
            }
            if viewport.zoom >= 0.7 {
                // Use a horizontal run outside endpoint nodes. Labels remain derived
                // metadata so a sample selection never invalidates retained wire geometry.
                if let Some(segment) = points
                    .windows(2)
                    .filter(|p| p[0][1] == p[1][1] && (p[1][0] - p[0][0]).abs() >= 55.0)
                    .max_by(|a, b| {
                        (a[1][0] - a[0][0])
                            .abs()
                            .total_cmp(&(b[1][0] - b[0][0]).abs())
                    })
                {
                    let x = (segment[0][0] + segment[1][0]) * 0.5;
                    let y = segment[0][1];
                    if width > 1 {
                        scene
                            .quads
                            .push(Quad::line([x - 4.0, y + 5.0], [x + 4.0, y - 5.0], color));
                    }
                    let mut text = match signal.and_then(|l| l.state.as_ref()) {
                        Some(value) if width > 1 => format!("/{width} · {value}"),
                        Some(value) => value.clone(),
                        None if width > 1 => format!("/{width}"),
                        None => String::new(),
                    };
                    let available = (segment[1][0] - segment[0][0]).abs().min(190.0);
                    // Never show a clipped numeric value that could be read as a
                    // different number; the full value remains in the node and trace.
                    if text.len() as f32 * 7.2 > available {
                        text = if width > 1 {
                            format!("/{width}")
                        } else {
                            String::new()
                        };
                    }
                    if !text.is_empty() {
                        scene.labels.push(TextLabel {
                            text,
                            rect: Rect {
                                x: x - available * 0.5,
                                y: y - 22.0,
                                width: available,
                                height: 17.0,
                            },
                            font_size: 12.0_f32.max(10.0 / viewport.zoom),
                            right_aligned: false,
                            centered: false,
                            bold: false,
                            color: palette.text.linear(),
                            after_quad: scene.quads.len(),
                        });
                    }
                }
            }
            scene.visible_edges += 1;
        }
        timings.edge_geometry_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        // Keep screen-sized overview ports visible when node bodies are offscreen.
        let padded = expanded(area, 12.0_f32.max(3.0 / viewport.zoom));
        let nodes = visible_ids(
            self.node_index.query(padded),
            preview.placement.keys().copied(),
        );
        timings.node_query_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        for id in nodes {
            let node = &self.nodes[&id];
            let rect = preview.placement.get(&id).copied().unwrap_or(node.rect);
            if !rect.intersects(padded) {
                continue;
            }
            let selected = selection.contains(&id);
            let tiny = viewport.zoom < 0.35;
            let labels = catalog.get(&node.type_id);
            let tone = labels
                .and_then(|l| l.color)
                .map(crate::ThemeColor::new)
                .unwrap_or_else(|| {
                    palette.tone(labels.map_or(crate::NodeTone::Neutral, |l| l.tone))
                })
                .linear();
            let missing_input = node.required_inputs.iter().any(|port| {
                !self
                    .incident
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .any(|edge| self.edges[edge].to == (id, *port))
            });
            let hit = self.node_cache.borrow().get(&id).is_some_and(|c| {
                c.key.0 == rect
                    && c.key.1 == selected
                    && c.key.2 == viewport.zoom.to_bits()
                    && c.key.3 == locale as u8
                    && c.key.4 == theme
                    && c.key.5.as_ref() == labels
                    && c.key.6 == missing_input
            });
            if hit {
                let cache = self.node_cache.borrow();
                let cached = &cache[&id];
                let base = scene.quads.len();
                scene.quads.extend_from_slice(&cached.quads);
                scene
                    .labels
                    .extend(cached.labels.iter().cloned().map(|mut l| {
                        l.after_quad += base;
                        l
                    }));
                scene.geometry_stats.reused_nodes += 1;
                scene.visible_nodes += 1;
                continue;
            }
            let frame_free = labels.is_some_and(|l| l.gate.is_some() && l.hide_frame);
            let outline_gate =
                tiny && theme == Theme::Light && labels.and_then(|l| l.gate).is_some();
            let qstart = scene.quads.len();
            let lstart = scene.labels.len();
            // A single soft SDF instance for depth; skip ornament at overview zoom.
            if viewport.zoom >= 0.6 && !frame_free {
                let mut shadow = palette.shadow.linear();
                shadow[3] = 0.24;
                let mut q = Quad::rectangle(
                    Rect {
                        x: rect.x - 6.0,
                        y: rect.y - 2.0,
                        width: rect.width + 12.0,
                        height: rect.height + 12.0,
                    },
                    shadow,
                    16.0,
                );
                q.params[3] = 7.0;
                scene.quads.push(q);
            }
            if selected && !outline_gate && !frame_free {
                let mut halo = palette.accent.linear();
                halo[3] = 0.22;
                scene.quads.push(Quad::rectangle(
                    Rect {
                        x: rect.x - 4.0,
                        y: rect.y - 4.0,
                        width: rect.width + 8.0,
                        height: rect.height + 8.0,
                    },
                    halo,
                    14.0,
                ));
            }
            // Keep the role hue, with enough depth for a white overview symbol.
            let overview_fill = [tone[0] * 0.32, tone[1] * 0.32, tone[2] * 0.32, 1.0];
            let border = if selected {
                palette.accent.linear()
            } else if tiny {
                overview_fill
            } else {
                palette.border.linear()
            };
            let inset = if selected { 2.0 } else { 1.0 };
            if frame_free {
                if selected {
                    draw_selection_corners(
                        &mut scene.quads,
                        rect,
                        palette.accent.linear(),
                        2.0 / viewport.zoom,
                    );
                }
            } else if outline_gate {
                draw_node_outline(
                    &mut scene.quads,
                    rect,
                    if selected {
                        palette.accent.linear()
                    } else {
                        tone
                    },
                    (if selected { 2.0 } else { 1.0 }) / viewport.zoom,
                );
            } else {
                scene
                    .quads
                    .push(Quad::rectangle(rect, border, NODE_CORNER_RADIUS));
                scene.quads.push(Quad::rectangle(
                    Rect {
                        x: rect.x + inset,
                        y: rect.y + inset,
                        width: (rect.width - 2.0 * inset).max(0.1),
                        height: (rect.height - 2.0 * inset).max(0.1),
                    },
                    if tiny {
                        overview_fill
                    } else {
                        palette.node.linear()
                    },
                    9.0,
                ));
            }
            if tiny && labels.and_then(|l| l.gate).is_some() {
                let gate = labels.unwrap().gate.unwrap();
                let bounds = Rect {
                    x: rect.x + rect.width * 0.16,
                    y: rect.y + rect.height * 0.16,
                    width: rect.width * 0.68,
                    height: rect.height * 0.68,
                };
                let stroke = wire_width(viewport.zoom);
                draw_gate(
                    &mut scene.quads,
                    gate,
                    bounds,
                    if outline_gate || frame_free {
                        tone
                    } else {
                        [1.0; 4]
                    },
                    stroke,
                );
                if outline_gate || frame_free {
                    draw_gate_leads(
                        &mut scene.quads,
                        gate,
                        rect,
                        bounds,
                        node.inputs,
                        tone,
                        stroke,
                    );
                }
            } else if tiny {
                let symbol = labels.map(|l| l.symbol.as_str()).unwrap_or("?");
                let symbol = match symbol {
                    "M" => "MUX",
                    "|" => "≥1",
                    "^" => "⊕",
                    "~" => "¬",
                    _ => symbol,
                };
                let padding = rect.width.min(rect.height) * 0.12;
                let width = rect.width - padding * 2.0;
                scene.labels.push(TextLabel {
                    text: symbol.into(),
                    rect: Rect {
                        x: rect.x + padding,
                        y: rect.y + padding,
                        width,
                        height: rect.height - padding * 2.0,
                    },
                    font_size: (rect.width.min(rect.height) * 0.58)
                        .min(width / (symbol.chars().count().max(1) as f32 * 0.72))
                        .clamp(1.0, 512.0),
                    right_aligned: false,
                    centered: true,
                    bold: true,
                    color: [1.0; 4],
                    after_quad: scene.quads.len(),
                });
            }
            if !frame_free && viewport.zoom >= 0.6 && rect.width >= 48.0 && rect.height >= 40.0 {
                // Opaque tint in linear space: inexpensive header separation in both themes.
                // Keep the existing port anchors and dense-node label clearance unchanged.
                let mut header = palette.node.linear();
                for channel in 0..3 {
                    header[channel] = header[channel] * 0.88 + tone[channel] * 0.12;
                }
                let height = if rect.height >= 120.0 { 34.0 } else { 23.0 };
                for (y, height, radius) in
                    [(inset, height - inset, 9.0), (12.0, height - 12.0, 0.0)]
                {
                    scene.quads.push(Quad::rectangle(
                        Rect {
                            x: rect.x + inset,
                            y: rect.y + y,
                            width: rect.width - inset * 2.0,
                            height,
                        },
                        header,
                        radius,
                    ));
                }
                scene.quads.push(Quad::rectangle(
                    Rect {
                        x: rect.x + 12.0,
                        y: rect.y + 1.5,
                        width: rect.width - 24.0,
                        height: 2.0,
                    },
                    tone,
                    1.0,
                ));
                if rect.height >= 120.0 {
                    let mut divider = palette.border.linear();
                    divider[3] = 0.25;
                    scene.quads.push(Quad::rectangle(
                        Rect {
                            x: rect.x + 12.0,
                            y: rect.y + 33.0,
                            width: rect.width - 24.0,
                            height: 1.0,
                        },
                        divider,
                        0.0,
                    ));
                }
            }
            if missing_input {
                let radius = (2.5 / viewport.zoom).clamp(3.0, 10.0);
                scene.quads.push(Quad::rectangle(
                    Rect {
                        x: rect.x + rect.width - radius * 2.0 - 8.0,
                        y: rect.y + 8.0,
                        width: radius * 2.0,
                        height: radius * 2.0,
                    },
                    palette.cable_invalid.linear(),
                    radius,
                ));
            }
            for (count, output) in [(node.inputs, false), (node.outputs, true)] {
                for i in 0..count {
                    let p = port_anchor(rect, i, count, output);
                    // Overview dots retain a 3px diameter. At higher LODs keep
                    // the ring rim at least 1px, using the same cable anchor.
                    let outer = 6.0_f32.max(if tiny { 1.5 } else { 2.5 } / viewport.zoom);
                    let inner = outer - 1.5_f32.max(1.0 / viewport.zoom);
                    let center = 2.5_f32.max(0.8 / viewport.zoom).min(inner);
                    let rings = [
                        (outer, tone),
                        (inner, palette.node.linear()),
                        (center, tone),
                    ];
                    for &(radius, color) in &rings[..if tiny { 1 } else { 3 }] {
                        let mut port = Quad::rectangle(
                            Rect {
                                x: p[0] - radius,
                                y: p[1] - radius,
                                width: radius * 2.0,
                                height: radius * 2.0,
                            },
                            color,
                            radius,
                        );
                        port.params[2] = -1.0;
                        scene.quads.push(port);
                    }
                }
            }
            if viewport.zoom >= 0.35 && rect.width >= 48.0 && rect.height >= 40.0 {
                let labels = catalog.get(&node.type_id);
                let symbol = labels
                    .map(|l| label_text(None, &l.symbol, locale))
                    .unwrap_or_default();
                let has_symbol = !symbol.is_empty()
                    && rect.height >= 120.0
                    && rect.width >= 96.0
                    && viewport.zoom >= 0.35;
                // Preserve screen-space legibility in an overview. Keep text
                // within the node; fine port labels yield to title and formula.
                let title_font = 15.0_f32.max(12.0 / viewport.zoom);
                let detail_font = 12.0_f32.max(11.0 / viewport.zoom);
                let title_height = title_font * 1.3;
                let state = labels.and_then(|l| l.state.as_ref());
                let gate = labels.and_then(|l| l.gate);
                if let Some(gate) = gate {
                    let bounds = Rect {
                        x: rect.x + rect.width * 0.5 - 50.0,
                        y: rect.y + rect.height * 0.5 - 50.0,
                        width: 100.0,
                        height: 100.0,
                    };
                    let stroke = wire_width(viewport.zoom);
                    draw_gate(&mut scene.quads, gate, bounds, tone, stroke);
                    draw_gate_leads(
                        &mut scene.quads,
                        gate,
                        rect,
                        bounds,
                        node.inputs,
                        tone,
                        stroke,
                    );
                }
                if has_symbol && state.is_none() && gate.is_none() {
                    let center = rect.x + rect.width * 0.5;
                    let top = rect.y + (rect.height - SYMBOL_SIZE) * 0.5;
                    let is_register = symbol == "D/Q";
                    let badge = Rect {
                        x: center - SYMBOL_SIZE * 0.5,
                        y: top,
                        width: SYMBOL_SIZE,
                        height: SYMBOL_SIZE,
                    };
                    scene
                        .quads
                        .push(Quad::rectangle(badge, tone, NODE_CORNER_RADIUS));
                    let mut fill = palette.node.linear();
                    for channel in 0..3 {
                        fill[channel] = fill[channel] * 0.86 + tone[channel] * 0.14;
                    }
                    scene.quads.push(Quad::rectangle(
                        Rect {
                            x: badge.x + 1.5,
                            y: badge.y + 1.5,
                            width: SYMBOL_SIZE - 3.0,
                            height: SYMBOL_SIZE - 3.0,
                        },
                        fill,
                        NODE_CORNER_RADIUS - 1.5,
                    ));
                    if is_register {
                        for (a, b) in [
                            ([badge.x, top + 30.0], [badge.x + 8.0, top + 35.0]),
                            ([badge.x + 8.0, top + 35.0], [badge.x, top + 40.0]),
                        ] {
                            scene.quads.push(Quad::line(a, b, tone));
                        }
                    }
                    let symbol_font = if is_register {
                        18.0
                    } else {
                        fitted_symbol_size(&symbol, SYMBOL_SIZE - 8.0, 25.0)
                    };
                    scene.labels.push(TextLabel {
                        text: if is_register { "D   Q".into() } else { symbol },
                        rect: badge,
                        font_size: symbol_font,
                        right_aligned: false,
                        centered: true,
                        bold: true,
                        color: palette.text.linear(),
                        after_quad: scene.quads.len(),
                    });
                }
                let after_quad = scene.quads.len();
                let title_inset = if state.is_some() { 39.0 } else { 12.0 };
                if state.is_some() {
                    scene.labels.push(TextLabel {
                        text: labels.map(|l| l.symbol.clone()).unwrap_or_default(),
                        rect: Rect {
                            x: rect.x + 9.0,
                            y: rect.y + 8.0,
                            width: 29.0,
                            height: 20.0,
                        },
                        font_size: fitted_symbol_size(
                            labels.map(|l| l.symbol.as_str()).unwrap_or(""),
                            27.0,
                            12.0,
                        ),
                        right_aligned: false,
                        centered: false,
                        bold: false,
                        color: tone,
                        after_quad,
                    });
                }
                scene.labels.push(TextLabel {
                    text: label_text(labels.map(|v| &v.title), &node.type_id, locale),
                    rect: Rect {
                        x: rect.x + title_inset,
                        y: rect.y + if rect.height >= 120.0 { 5.0 } else { 4.0 },
                        width: rect.width - title_inset - if missing_input { 32.0 } else { 12.0 },
                        height: title_height,
                    },
                    font_size: title_font,
                    right_aligned: false,
                    centered: false,
                    bold: false,
                    color: palette.text.linear(),
                    after_quad,
                });
                let caption = labels
                    .map(|l| label_text(Some(&l.caption), "", locale))
                    .unwrap_or_default();
                let rows = node.inputs.max(node.outputs);
                let bottom_port = rect.height * rows as f32 / (rows + 1) as f32;
                if rect.height >= 90.0
                    && !caption.is_empty()
                    && (viewport.zoom < 0.85 || bottom_port + 10.0 <= rect.height - 28.0)
                {
                    scene.labels.push(TextLabel {
                        text: caption,
                        rect: Rect {
                            x: rect.x + 12.0,
                            y: rect.y + rect.height - detail_font * 1.3 - 5.0,
                            width: rect.width - 24.0,
                            height: detail_font * 1.3,
                        },
                        font_size: detail_font,
                        right_aligned: false,
                        centered: false,
                        bold: false,
                        color: palette.text.linear(),
                        after_quad,
                    });
                }
                if let Some(state) = state {
                    let is_register = labels.is_some_and(|l| l.symbol == "D/Q");
                    let value = if is_register {
                        format!("Q = {state}")
                    } else {
                        state.clone()
                    };
                    let font = (176.0 / (value.len() as f32 * 0.61)).clamp(11.0, 22.0);
                    scene.labels.push(TextLabel {
                        text: value,
                        rect: Rect {
                            x: rect.x + 14.0,
                            y: rect.y
                                + rect.height * 0.5
                                + if gate.is_some() { 65.0 } else { -8.0 },
                            width: rect.width - 28.0,
                            height: 29.0,
                        },
                        font_size: font,
                        right_aligned: false,
                        centered: false,
                        bold: false,
                        color: palette.text.linear(),
                        after_quad,
                    });
                    if let Some(next) = labels.and_then(|l| l.next_state.as_ref()) {
                        scene.labels.push(TextLabel {
                            text: format!("D = {next}"),
                            rect: Rect {
                                x: rect.x + 14.0,
                                y: rect.y + rect.height * 0.5 + 20.0,
                                width: rect.width - 28.0,
                                height: 18.0,
                            },
                            font_size: 12.0,
                            right_aligned: false,
                            centered: false,
                            bold: false,
                            color: tone,
                            after_quad,
                        });
                    }
                }
                if viewport.zoom >= 0.4 {
                    for (names, output) in [(&node.input_names, false), (&node.output_names, true)]
                    {
                        // Dense port rows would overlap; retain circles and omit labels.
                        if rect.height / ((names.len() + 1) as f32) < 32.0 {
                            continue;
                        }
                        for (i, name) in names.iter().enumerate() {
                            let anchor = port_anchor(rect, i, names.len(), output)[1];
                            let center = rect.y + rect.height * 0.5;
                            let mut y = anchor - if gate.is_some() { 34.0 } else { 15.0 };
                            if state.is_some() && gate.is_none() {
                                y = if anchor <= center {
                                    y.min(center - 43.0)
                                } else {
                                    y.max(center + 24.0)
                                };
                            }
                            let header_bottom =
                                rect.y + if rect.height >= 120.0 { 34.0 } else { 23.0 };
                            if y < header_bottom || y + 31.0 > rect.y + rect.height - 5.0 {
                                continue;
                            }
                            let localized = labels.and_then(|v| {
                                if output {
                                    v.outputs.get(name)
                                } else {
                                    v.inputs.get(name)
                                }
                            });
                            scene.labels.push(TextLabel {
                                text: if viewport.zoom >= 0.65 {
                                    label_text(localized, name, locale)
                                } else {
                                    if output && labels.is_some_and(|label| label.symbol == "D/Q") {
                                        "q".into()
                                    } else {
                                        name.clone()
                                    }
                                },
                                rect: Rect {
                                    x: rect.x
                                        + if output {
                                            rect.width - if gate.is_some() { 88.0 } else { 116.0 }
                                        } else {
                                            12.0
                                        },
                                    y,
                                    width: if gate.is_some() { 76.0 } else { 104.0 },
                                    height: 31.0,
                                },
                                font_size: 24.0,
                                right_aligned: output,
                                centered: false,
                                bold: false,
                                color: palette.text.linear(),
                                after_quad,
                            });
                        }
                    }
                }
            }
            self.node_cache.borrow_mut().insert(
                id,
                CachedNode {
                    key: (
                        rect,
                        selected,
                        viewport.zoom.to_bits(),
                        locale as u8,
                        theme,
                        labels.cloned(),
                        missing_input,
                    ),
                    quads: scene.quads[qstart..].to_vec(),
                    labels: scene.labels[lstart..]
                        .iter()
                        .cloned()
                        .map(|mut l| {
                            l.after_quad -= qstart;
                            l
                        })
                        .collect(),
                },
            );
            scene.geometry_stats.rebuilt_nodes += 1;
            scene.visible_nodes += 1;
        }
        timings.node_geometry_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        if let Some(cable) = &preview.cable {
            push_curve(
                &mut scene,
                curve(cable.from, cable.to),
                viewport.zoom,
                if cable.valid {
                    palette.cable_valid.linear()
                } else {
                    palette.cable_invalid.linear()
                },
            );
        }
        if let Some(rect) = preview.marquee {
            let mut fill = palette.accent.linear();
            fill[3] = 0.15;
            scene.quads.push(Quad::rectangle(rect, fill, 0.0));
            let width = (1.0 / viewport.zoom).min(rect.width.min(rect.height));
            for border in [
                Rect {
                    height: width,
                    ..rect
                },
                Rect {
                    y: rect.y + rect.height - width,
                    height: width,
                    ..rect
                },
                Rect { width, ..rect },
                Rect {
                    x: rect.x + rect.width - width,
                    width,
                    ..rect
                },
            ] {
                scene
                    .quads
                    .push(Quad::rectangle(border, palette.accent.linear(), 0.0));
            }
        }
        timings.overlays_ms = elapsed_ms(start);
        timings.total_ms = elapsed_ms(total);
        Ok((scene, timings))
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    #[test]
    fn preview_union_matches_ordered_set_without_duplicates() {
        for base in [vec![], vec![1, 1, 3, 5, 8], (1..=10000).collect()] {
            for extra in [
                vec![],
                vec![8, 3, 3, 2, 11, 1],
                (9000..=11000).rev().collect(),
            ] {
                let base: Vec<_> = base.iter().copied().map(Id::from_u128).collect();
                let extra: Vec<_> = extra.iter().copied().map(Id::from_u128).collect();
                let expected: BTreeSet<_> = base.iter().chain(&extra).copied().collect();
                assert_eq!(
                    visible_ids(base, extra.into_iter()),
                    expected.into_iter().collect::<Vec<_>>()
                );
            }
        }
    }
}

fn draw_gate(
    quads: &mut Vec<Quad>,
    gate: crate::GateSymbol,
    rect: Rect,
    color: [f32; 4],
    width: f32,
) {
    for path in gate.paths() {
        for pair in path.windows(2) {
            let point = |p: [f32; 2]| {
                [
                    rect.x + p[0] * rect.width / 100.0,
                    rect.y + p[1] * rect.height / 100.0,
                ]
            };
            quads.push(Quad::stroke(point(pair[0]), point(pair[1]), color, width));
        }
    }
}

// Leave side bearings inside the rounded badge, including four-letter gate names.
fn fitted_symbol_size(symbol: &str, available: f32, preferred: f32) -> f32 {
    preferred.min(available / (symbol.chars().count().max(1) as f32 * 0.8))
}

// Outline only: leave the canvas grid visible through light-mode overview gates.
fn draw_node_outline(quads: &mut Vec<Quad>, rect: Rect, color: [f32; 4], width: f32) {
    let r = NODE_CORNER_RADIUS
        .min(rect.width * 0.5)
        .min(rect.height * 0.5);
    let mut points = Vec::with_capacity(21);
    for (x, y, start) in [
        (rect.x + rect.width - r, rect.y + r, -90.0_f32),
        (rect.x + rect.width - r, rect.y + rect.height - r, 0.0),
        (rect.x + r, rect.y + rect.height - r, 90.0),
        (rect.x + r, rect.y + r, 180.0),
    ] {
        for i in 0..=4 {
            let a = (start + 22.5 * i as f32).to_radians();
            points.push([x + r * a.cos(), y + r * a.sin()]);
        }
    }
    points.push(points[0]);
    for p in points.windows(2) {
        quads.push(Quad::stroke(p[0], p[1], color, width));
    }
}

// Use the same port-to-symbol routing at every LOD; normalized gate coordinates
// must scale with the larger overview symbol, including its inversion bubble.
fn draw_gate_leads(
    quads: &mut Vec<Quad>,
    gate: crate::GateSymbol,
    rect: Rect,
    bounds: Rect,
    inputs: usize,
    color: [f32; 4],
    width: f32,
) {
    for i in 0..inputs {
        let anchor = port_anchor(rect, i, inputs, false);
        let y = if inputs == 1 {
            50.0
        } else if i == 0 {
            25.0
        } else {
            75.0
        };
        let target = [
            bounds.x + gate.input_x(y) * bounds.width / 100.0,
            bounds.y + y * bounds.height / 100.0,
        ];
        let elbow = bounds.x - 12.0;
        for (a, b) in [
            (anchor, [elbow, anchor[1]]),
            ([elbow, anchor[1]], [elbow, target[1]]),
            ([elbow, target[1]], target),
        ] {
            quads.push(Quad::stroke(a, b, color, width));
        }
    }
    quads.push(Quad::stroke(
        [
            bounds.x + (if gate.inverted() { 94.0 } else { 82.0 }) * bounds.width / 100.0,
            bounds.y + bounds.height / 2.0,
        ],
        port_anchor(rect, 0, 1, true),
        color,
        width,
    ));
}

fn draw_selection_corners(quads: &mut Vec<Quad>, rect: Rect, color: [f32; 4], width: f32) {
    for (x, y, dx, dy) in [
        (rect.x, rect.y, 1.0, 1.0),
        (rect.x + rect.width, rect.y, -1.0, 1.0),
        (rect.x, rect.y + rect.height, 1.0, -1.0),
        (rect.x + rect.width, rect.y + rect.height, -1.0, -1.0),
    ] {
        quads.push(Quad::stroke([x, y], [x + dx * 18.0, y], color, width));
        quads.push(Quad::stroke([x, y], [x, y + dy * 18.0], color, width));
    }
}
