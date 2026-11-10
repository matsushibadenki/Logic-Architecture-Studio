pub mod editor;
pub mod retained;
pub use unge_core::Viewport as NativeViewport;
pub use unge_interaction;
// UNGE rendering adapter. The design service remains the only editable authority.
// This crate derives an immutable render scene, never a second editable workspace.
use design_core::{Document, Kind};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use unge_core::{
    Cardinality, Command, DataType, Editor, Endpoint, Id, Locale, Port, Properties, Rect, Viewport,
};
use unge_render::{
    LabelCatalog, LabelText, NodeLabels, NodeTone, SceneIndex, SurfaceRenderer, Theme,
};

pub const NODE_WIDTH: f32 = 208.;
pub fn node_height(kind: &Kind) -> f32 {
    match kind {
        Kind::Mux => 208.,
        Kind::Input | Kind::Output => 128.,
        _ => 144.,
    }
}
fn text(en: &str, ja: &str, zh: &str) -> LabelText {
    LabelText {
        en: en.into(),
        ja: ja.into(),
        zh_cn: zh.into(),
    }
}
fn labels(kind: &Kind) -> (NodeTone, &'static str, LabelText) {
    match kind {
        Kind::Input => (NodeTone::Mint, "I", text("Input", "入力", "输入")),
        Kind::Output => (NodeTone::Rose, "O", text("Output", "出力", "输出")),
        Kind::Add => (
            NodeTone::Blue,
            "+",
            text("y = a + b", "y = a + b", "y = a + b"),
        ),
        Kind::Sub => (
            NodeTone::Blue,
            "−",
            text("y = a − b", "y = a − b", "y = a − b"),
        ),
        Kind::And => (
            NodeTone::Violet,
            "&",
            text("y = a & b", "y = a & b", "y = a & b"),
        ),
        Kind::Or => (
            NodeTone::Violet,
            "|",
            text("y = a | b", "y = a | b", "y = a | b"),
        ),
        Kind::Xor => (
            NodeTone::Violet,
            "^",
            text("y = a ^ b", "y = a ^ b", "y = a ^ b"),
        ),
        Kind::Not => (NodeTone::Violet, "~", text("y = ~a", "y = ~a", "y = ~a")),
        Kind::Mux => (
            NodeTone::Amber,
            "M",
            text(
                "sel → d0 / d1 / d2",
                "sel → d0 / d1 / d2",
                "sel → d0 / d1 / d2",
            ),
        ),
    }
}
fn stable_id(domain: &str, id: &str) -> Id {
    let digest = Sha256::digest(format!("{domain}:{id}").as_bytes());
    Id::from_bytes(digest[..16].try_into().unwrap())
}
fn node_rect(n: &design_core::Node) -> Rect {
    Rect {
        x: n.x,
        y: n.y,
        width: NODE_WIDTH,
        height: node_height(&n.kind),
    }
}
fn render_node(n: &design_core::Node) -> unge_core::Node {
    let port = |name: &str, width: u8| Port {
        name: name.into(),
        data_type: DataType::Custom(format!("bits{width}")),
        cardinality: Cardinality::Single,
        required: false,
    };
    unge_core::Node {
        id: stable_id("node", &n.id),
        type_id: n.id.clone(),
        inputs: n.inputs().into_iter().map(|(p, w)| port(p, w)).collect(),
        outputs: if n.kind == Kind::Output {
            vec![]
        } else {
            vec![port("y", n.width)]
        },
        properties: Properties::new(),
    }
}
fn render_edge(e: &design_core::Edge) -> unge_core::Edge {
    unge_core::Edge {
        id: stable_id("edge", &e.id),
        from: Endpoint {
            node: stable_id("node", &e.from),
            port: "y".into(),
        },
        to: Endpoint {
            node: stable_id("node", &e.to),
            port: e.port.clone(),
        },
    }
}
fn node_labels(n: &design_core::Node) -> NodeLabels {
    let (tone, symbol, caption) = labels(&n.kind);
    let title = format!("{} · {}b", n.name, n.width);
    NodeLabels {
        tone,
        symbol: symbol.into(),
        caption,
        title: text(&title, &title, &title),
        inputs: n
            .inputs()
            .into_iter()
            .map(|(p, w)| {
                let label = format!("{p} · {w}");
                (p.into(), text(&label, &label, &label))
            })
            .collect(),
        outputs: [("y".into(), text("y", "y", "y"))].into(),
    }
}
/// Stateless projection using UNGE's validated Batch and native port geometry.
pub fn project(
    document: &Document,
) -> Result<(unge_core::Document, LabelCatalog, BTreeMap<String, Id>), String> {
    document.validate(false)?;
    let ids: BTreeMap<_, _> = document
        .nodes
        .iter()
        .map(|n| (n.id.clone(), stable_id("node", &n.id)))
        .collect();
    let mut commands = vec![];
    let mut catalog = LabelCatalog::new();
    for n in &document.nodes {
        catalog.insert(n.id.clone(), node_labels(n));
        commands.push(Command::AddNode {
            node: render_node(n),
            rect: node_rect(n),
        });
    }
    for e in &document.edges {
        commands.push(Command::Connect {
            edge: unge_core::Edge {
                id: stable_id("edge", &e.id),
                from: Endpoint {
                    node: ids[&e.from],
                    port: "y".into(),
                },
                to: Endpoint {
                    node: ids[&e.to],
                    port: e.port.clone(),
                },
            },
        });
    }
    let mut editor = Editor::new(unge_core::Document::default(), 0).map_err(|e| e.to_string())?;
    if !commands.is_empty() {
        editor
            .execute(Command::Batch { commands })
            .map_err(|e| e.to_string())?;
    }
    Ok((editor.document().clone(), catalog, ids))
}

pub struct Renderer {
    pub dock_target: Option<[f32; 4]>,
    surface: SurfaceRenderer,
    size: [u32; 2],
    projection: Option<retained::RetainedProjection>,
}
impl Renderer {
    pub fn new<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static>(
        window: Arc<W>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            dock_target: None,
            surface: pollster::block_on(SurfaceRenderer::new(window, [width, height]))?,
            size: [width, height],
            projection: None,
        })
    }
    fn sync_document(&mut self, document: &Document) -> Result<(), String> {
        if let Some(projection) = &mut self.projection {
            projection.sync(document)?;
        } else {
            self.projection = Some(retained::RetainedProjection::new(document)?);
        }
        Ok(())
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.size == [width, height] {
            return;
        }
        self.size = [width, height];
        if let Err(e) = self.surface.resize(self.size) {
            eprintln!("Renderer resize: {e}");
        }
    }
    pub fn draw(
        &mut self,
        document: &Document,
        selected: Option<&str>,
        locale: &str,
    ) -> Result<(), String> {
        if self.size.contains(&0) {
            return Ok(());
        }
        self.sync_document(document)?;
        let projection = self.projection.as_ref().unwrap();
        let minx = document.nodes.iter().map(|n| n.x).fold(0., f32::min) - 30.;
        let miny = document.nodes.iter().map(|n| n.y).fold(0., f32::min) - 30.;
        let maxx = document
            .nodes
            .iter()
            .map(|n| n.x + NODE_WIDTH + 60.)
            .fold(1100., f32::max);
        let maxy = document
            .nodes
            .iter()
            .map(|n| n.y + node_height(&n.kind) + 60.)
            .fold(740., f32::max);
        let zoom = (self.size[0] as f32 / (maxx - minx))
            .min(self.size[1] as f32 / (maxy - miny))
            .clamp(0.02, 16.);
        let viewport = Viewport {
            origin: [
                minx - (self.size[0] as f32 / zoom - (maxx - minx)) / 2.,
                miny - (self.size[1] as f32 / zoom - (maxy - miny)) / 2.,
            ],
            zoom,
            size: self.size.map(|v| v as f32),
        };
        let selection: BTreeSet<_> = selected
            .and_then(|id| projection.ids.get(id))
            .copied()
            .into_iter()
            .collect();
        let scene = projection
            .index
            .scene_with_theme(
                viewport,
                &selection,
                &Default::default(),
                &projection.labels,
                match locale {
                    "ja" => Locale::Ja,
                    "zh" => Locale::ZhCn,
                    _ => Locale::En,
                },
                Theme::Dark,
            )
            .map_err(|e| e.to_string())?;
        self.surface.draw(&scene, viewport)?;
        Ok(())
    }
    pub fn draw_editor(
        &mut self,
        document: &Document,
        locale: &str,
        view: &editor::EditorView,
        graph: [u32; 2],
    ) -> Result<(), String> {
        self.sync_document(document)?;
        let projection = self.projection.as_ref().unwrap();
        let mut scene = projection
            .index
            .scene_with_theme(
                view.viewport,
                &view.selection,
                view.interaction.preview(),
                &projection.labels,
                match locale {
                    "ja" => Locale::Ja,
                    "zh" => Locale::ZhCn,
                    _ => Locale::En,
                },
                Theme::Dark,
            )
            .map_err(|e| e.to_string())?;
        if let Some([x, y, w, h]) = self.dock_target {
            let p = view.viewport.to_world([x, y]);
            let mut color = Theme::Dark.palette().accent.linear();
            color[3] = 0.3;
            let width = w / view.viewport.zoom;
            let height = h / view.viewport.zoom;
            scene.quads.push(unge_render::Quad {
                rect: [p[0] + width / 2., p[1] + height / 2., width, height],
                color,
                params: [0., 6., 0., 0.],
            });
        }
        self.surface.draw_region(&scene, view.viewport, graph)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overview_keeps_formula_and_width_legible_in_all_locales() {
        let (doc, catalog, _) = project(&Document::demo()).unwrap();
        let index = SceneIndex::new(&doc);
        for locale in [Locale::En, Locale::Ja, Locale::ZhCn] {
            for zoom in [0.4, 0.6, 1.0] {
                let scene = index
                    .scene_with_labels(
                        Viewport {
                            origin: [0., 0.],
                            size: [1600., 1000.],
                            zoom,
                        },
                        &BTreeSet::new(),
                        &Default::default(),
                        &catalog,
                        locale,
                    )
                    .unwrap();
                let formula = scene.labels.iter().find(|l| l.text == "y = a + b").unwrap();
                assert!(formula.font_size * zoom >= 11.0 - 0.001);
                let title = scene.labels.iter().find(|l| l.text == "add · 32b").unwrap();
                assert!(title.font_size * zoom >= 12.0 - 0.001);
                assert!(formula.rect.y >= title.rect.y + title.rect.height);
            }
        }
    }
    #[test]
    fn projection_preserves_graph_and_widths() {
        let source = Document::demo();
        let (projection, _, ids) = project(&source).unwrap();
        assert_eq!(ids.len(), source.nodes.len());
        assert_eq!(projection.graph().nodes().len(), source.nodes.len());
        assert_eq!(projection.graph().edges().len(), source.edges.len());
        assert_eq!(source, Document::demo());
    }
}
