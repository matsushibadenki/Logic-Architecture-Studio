use crate::{LabelCatalog, TextLabel, Theme, labels::label_text};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use unge_core::{Command, Document, Id, Locale, Rect, SpatialIndex, Viewport, port_anchor};
use unge_interaction::{MAX_SELECTION, PORT_LOD_ZOOM, Preview};

const MAX_MOVED_NODES: usize = 1024;
const MAX_REFIT_EDGES: usize = 4096;

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
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        Self {
            rect: [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                dx.hypot(dy).max(0.001) + 3.0,
                3.0,
            ],
            color,
            params: [dy.atan2(dx), 1.5, 0.0, 0.0],
        }
    }
}
struct RenderNode {
    rect: Rect,
    inputs: usize,
    outputs: usize,
    type_id: String,
    input_names: Vec<String>,
    output_names: Vec<String>,
}
struct RenderEdge {
    points: [[f32; 2]; 4],
    bounds: Rect,
    source_type: String,
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
        let points = curve(
            port_anchor(
                doc.placement().get(&from.id).copied().unwrap_or_default(),
                i,
                from.outputs.len(),
                true,
            ),
            port_anchor(
                doc.placement().get(&to.id).copied().unwrap_or_default(),
                j,
                to.inputs.len(),
                false,
            ),
        );
        Some(Self {
            points,
            bounds: curve_bounds(points),
            source_type: from.type_id.clone(),
            from: (from.id, i),
            to: (to.id, j),
        })
    }
}
type NodeCacheKey = (Rect, bool, u32, u8, Theme, Option<crate::NodeLabels>);
struct CachedNode {
    key: NodeCacheKey,
    quads: Vec<Quad>,
    labels: Vec<TextLabel>,
}
struct CachedEdge {
    points: [[f32; 2]; 4],
    zoom: f32,
    color: [f32; 4],
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
fn curve_bounds(points: [[f32; 2]; 4]) -> Rect {
    let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min) - 2.0;
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        + 2.0;
    let min_y = points[0][1].min(points[3][1]) - 2.0;
    let max_y = points[0][1].max(points[3][1]) + 2.0;
    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}
fn push_curve(scene: &mut Scene, points: [[f32; 2]; 4], zoom: f32, color: [f32; 4]) {
    crate::curves::for_each_segment(points, zoom, |a, b| {
        scene.quads.push(Quad::line(a, b, color));
    });
}
impl SceneIndex {
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

        Self {
            nodes,
            edges,
            incident,
            node_index: SpatialIndex::new(doc),
            edge_index: SpatialIndex::from_rects(bounds),
            groups: crate::groups::GroupIndex::new(doc),
            node_cache: RefCell::new(BTreeMap::new()),
            edge_cache: RefCell::new(BTreeMap::new()),
        }
    }
    /// The index must correspond to the state immediately before the captured
    /// command. Document validation and revision checks remain the host's job.
    pub fn update(&mut self, doc: &Document, changes: &SceneChanges) -> SceneUpdate {
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
                    color: palette.text.linear(),
                    after_quad: scene.quads.len(),
                });
            }
            scene.visible_groups += 1;
        }
        timings.groups_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        let edges = visible_ids(
            self.edge_index.query(area),
            preview
                .placement
                .keys()
                .flat_map(|id| self.incident.get(id).into_iter().flatten().copied()),
        );
        timings.edge_query_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        for id in edges {
            let edge = &self.edges[&id];
            let moved = preview.placement.contains_key(&edge.from.0)
                || preview.placement.contains_key(&edge.to.0);
            let points = if moved {
                let from = &self.nodes[&edge.from.0];
                let to = &self.nodes[&edge.to.0];
                curve(
                    port_anchor(
                        preview
                            .placement
                            .get(&edge.from.0)
                            .copied()
                            .unwrap_or(from.rect),
                        edge.from.1,
                        from.outputs,
                        true,
                    ),
                    port_anchor(
                        preview
                            .placement
                            .get(&edge.to.0)
                            .copied()
                            .unwrap_or(to.rect),
                        edge.to.1,
                        to.inputs,
                        false,
                    ),
                )
            } else {
                edge.points
            };
            let bounds = if moved {
                curve_bounds(points)
            } else {
                edge.bounds
            };
            if !bounds.intersects(area) {
                continue;
            }
            let color = catalog
                .get(&edge.source_type)
                .filter(|labels| labels.tone != crate::NodeTone::Neutral)
                .map_or(palette.edge, |labels| palette.tone(labels.tone));
            let color = color.linear();
            let hit =
                self.edge_cache.borrow().get(&id).is_some_and(|c| {
                    c.points == points && c.zoom == viewport.zoom && c.color == color
                });
            if hit {
                scene
                    .quads
                    .extend_from_slice(&self.edge_cache.borrow()[&id].quads);
                scene.geometry_stats.reused_edges += 1;
            } else {
                let start = scene.quads.len();
                push_curve(&mut scene, points, viewport.zoom, color);
                self.edge_cache.borrow_mut().insert(
                    id,
                    CachedEdge {
                        points,
                        zoom: viewport.zoom,
                        color,
                        quads: scene.quads[start..].to_vec(),
                    },
                );
                scene.geometry_stats.rebuilt_edges += 1;
            }
            scene.visible_edges += 1;
        }
        timings.edge_geometry_ms = elapsed_ms(start);
        let start = TIMED.then(Instant::now);
        // Include ports/selection borders protruding from the node bounds.
        let padded = Rect {
            x: area.x - 12.0,
            y: area.y - 12.0,
            width: area.width + 24.0,
            height: area.height + 24.0,
        };
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
            let labels = catalog.get(&node.type_id);
            let tone = palette
                .tone(labels.map_or(crate::NodeTone::Neutral, |l| l.tone))
                .linear();
            let hit = self.node_cache.borrow().get(&id).is_some_and(|c| {
                c.key.0 == rect
                    && c.key.1 == selected
                    && c.key.2 == viewport.zoom.to_bits()
                    && c.key.3 == locale as u8
                    && c.key.4 == theme
                    && c.key.5.as_ref() == labels
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
            let qstart = scene.quads.len();
            let lstart = scene.labels.len();
            // A single soft SDF instance for depth; skip ornament at overview zoom.
            if viewport.zoom >= 0.6 {
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
            if selected {
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
            let border = if selected {
                palette.accent.linear()
            } else {
                palette.border.linear()
            };
            scene.quads.push(Quad::rectangle(rect, border, 10.0));
            let inset = if selected { 2.0 } else { 1.0 };
            scene.quads.push(Quad::rectangle(
                Rect {
                    x: rect.x + inset,
                    y: rect.y + inset,
                    width: (rect.width - 2.0 * inset).max(0.1),
                    height: (rect.height - 2.0 * inset).max(0.1),
                },
                palette.node.linear(),
                9.0,
            ));
            if viewport.zoom >= 0.6 && rect.width >= 48.0 && rect.height >= 40.0 {
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
                if rect.height >= 120.0
                    && labels.is_some_and(|l| !l.symbol.is_empty())
                    && rect.width >= 96.0
                {
                    let mut badge = tone;
                    badge[3] = 0.14;
                    scene.quads.push(Quad::rectangle(
                        Rect {
                            x: rect.x + 10.0,
                            y: rect.y + 8.0,
                            width: 22.0,
                            height: 22.0,
                        },
                        badge,
                        5.0,
                    ));
                }
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
            if viewport.zoom >= PORT_LOD_ZOOM {
                for (count, output) in [(node.inputs, false), (node.outputs, true)] {
                    for i in 0..count {
                        let p = port_anchor(rect, i, count, output);
                        // Ring centres use the exact same anchor as hit testing and cables.
                        for (radius, color) in
                            [(6.0, tone), (4.5, palette.node.linear()), (2.5, tone)]
                        {
                            scene.quads.push(Quad::rectangle(
                                Rect {
                                    x: p[0] - radius,
                                    y: p[1] - radius,
                                    width: radius * 2.0,
                                    height: radius * 2.0,
                                },
                                color,
                                radius,
                            ));
                        }
                    }
                }
            }
            if viewport.zoom >= 0.35 && rect.width >= 48.0 && rect.height >= 40.0 {
                let labels = catalog.get(&node.type_id);
                let after_quad = scene.quads.len();
                let symbol = labels
                    .map(|l| label_text(None, &l.symbol, locale))
                    .unwrap_or_default();
                let has_symbol = !symbol.is_empty()
                    && rect.height >= 120.0
                    && rect.width >= 96.0
                    && viewport.zoom >= 0.85;
                // Preserve screen-space legibility in an overview. Keep text
                // within the node; fine port labels yield to title and formula.
                let title_font = 15.0_f32.max(12.0 / viewport.zoom);
                let detail_font = 12.0_f32.max(11.0 / viewport.zoom);
                let title_height = title_font * 1.3;
                if has_symbol {
                    scene.labels.push(TextLabel {
                        text: symbol.chars().take(2).collect(),
                        rect: Rect {
                            x: rect.x + 15.0,
                            y: rect.y + 10.0,
                            width: 16.0,
                            height: 18.0,
                        },
                        font_size: 13.0,
                        right_aligned: false,
                        color: tone,
                        after_quad,
                    });
                }
                let title_inset = if has_symbol { 40.0 } else { 12.0 };
                scene.labels.push(TextLabel {
                    text: label_text(labels.map(|v| &v.title), &node.type_id, locale),
                    rect: Rect {
                        x: rect.x + title_inset,
                        y: rect.y + if rect.height >= 120.0 { 5.0 } else { 4.0 },
                        width: rect.width - title_inset - 12.0,
                        height: title_height,
                    },
                    font_size: title_font,
                    right_aligned: false,
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
                        color: palette.text.linear(),
                        after_quad,
                    });
                }
                if viewport.zoom >= 0.85 {
                    for (names, output) in [(&node.input_names, false), (&node.output_names, true)]
                    {
                        // Dense port rows would overlap; retain circles and omit labels.
                        if rect.height / ((names.len() + 1) as f32) < 16.0 {
                            continue;
                        }
                        for (i, name) in names.iter().enumerate() {
                            let y = port_anchor(rect, i, names.len(), output)[1] - 7.0;
                            let header_bottom =
                                rect.y + if rect.height >= 120.0 { 34.0 } else { 23.0 };
                            if y < header_bottom || y + 14.0 > rect.y + rect.height - 5.0 {
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
                                text: label_text(localized, name, locale),
                                rect: Rect {
                                    x: rect.x + if output { rect.width / 2.0 + 4.0 } else { 12.0 },
                                    y,
                                    width: rect.width / 2.0 - 16.0,
                                    height: 14.0,
                                },
                                font_size: 11.0,
                                right_aligned: output,
                                color: palette.muted.linear(),
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
