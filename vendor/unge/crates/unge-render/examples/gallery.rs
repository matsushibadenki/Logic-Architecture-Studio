//! Real wgpu snapshots for visual review; PPM avoids extra runtime dependencies.
//! cargo run -p unge-render --example gallery -- /tmp/unge-gallery
use std::{collections::BTreeSet, io::Write, path::Path};
use unge_core::*;
use unge_render::*;
fn text(en: &str, ja: &str, zh: &str) -> LabelText {
    LabelText {
        en: en.into(),
        ja: ja.into(),
        zh_cn: zh.into(),
    }
}
fn fixture() -> (Document, LabelCatalog) {
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    let mut catalog = LabelCatalog::new();
    let fixtures = [
        (
            "source",
            text("Image source", "画像ソース", "图像来源"),
            NodeTone::Mint,
            64.,
            116.,
            0,
        ),
        (
            "adjust",
            text("Color adjustment", "色を調整", "颜色调整"),
            NodeTone::Blue,
            384.,
            116.,
            1,
        ),
        (
            "mask",
            text("Layer mask", "レイヤーマスク", "图层蒙版"),
            NodeTone::Violet,
            384.,
            388.,
            0,
        ),
        (
            "composite",
            text("Composite", "合成", "合成"),
            NodeTone::Amber,
            704.,
            248.,
            2,
        ),
        (
            "output",
            text("Image output", "画像出力", "图像输出"),
            NodeTone::Rose,
            1024.,
            248.,
            1,
        ),
    ];
    let port = |name: &str| Port {
        name: name.into(),
        data_type: DataType::Image,
        cardinality: Cardinality::Single,
        required: false,
    };
    for (i, (kind, title, tone, x, y, count)) in fixtures.into_iter().enumerate() {
        catalog.insert(
            kind.into(),
            NodeLabels {
                symbol: ["I", "C", "M", "+", "E"][i].into(),
                caption: [
                    text("Source image", "元の画像", "原始图像"),
                    text("Color processing", "カラー処理", "颜色处理"),
                    text("Mask input", "マスク入力", "蒙版输入"),
                    text("Combine layers", "レイヤーを合成", "合并图层"),
                    text("Output image", "処理結果", "输出图像"),
                ][i]
                    .clone(),
                tone,
                title,
                inputs: [
                    ("image".into(), text("Image", "画像", "图像")),
                    ("mask".into(), text("Mask", "マスク", "蒙版")),
                ]
                .into(),
                outputs: [("result".into(), text("Result", "出力", "输出"))].into(),
            },
        );
        editor
            .execute(Command::AddNode {
                node: Node {
                    id: Id::from_u128(i as u128 + 1),
                    type_id: kind.into(),
                    inputs: ["image", "mask"]
                        .into_iter()
                        .take(count)
                        .map(port)
                        .collect(),
                    outputs: vec![port("result")],
                    properties: Properties::new(),
                },
                rect: Rect {
                    x,
                    y,
                    width: 208.,
                    height: 128.,
                },
            })
            .unwrap();
    }
    for (n, (from, to, port)) in [
        (1, 2, "image"),
        (2, 4, "image"),
        (3, 4, "mask"),
        (4, 5, "image"),
    ]
    .into_iter()
    .enumerate()
    {
        editor
            .execute(Command::Connect {
                edge: Edge {
                    id: Id::from_u128(100 + n as u128),
                    from: Endpoint {
                        node: Id::from_u128(from),
                        port: "result".into(),
                    },
                    to: Endpoint {
                        node: Id::from_u128(to),
                        port: port.into(),
                    },
                },
            })
            .unwrap();
    }
    (editor.document().clone(), catalog)
}
fn main() {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/unge-gallery".into());
    std::fs::create_dir_all(&directory).unwrap();
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .expect("desktop GPU required");
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (document, catalog) = fixture();
        let index = SceneIndex::new(&document);
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        for (theme, name) in [(Theme::Dark, "dark"), (Theme::Light, "light")] {
            for (locale, language) in [(Locale::En, "en"), (Locale::Ja, "ja"), (Locale::ZhCn, "zh")]
            {
                let view = Viewport {
                    origin: [0., 0.],
                    zoom: 1.,
                    size: [1280., 640.],
                };
                let scene = index
                    .scene_with_theme(
                        view,
                        &BTreeSet::from([Id::from_u128(2)]),
                        &Default::default(),
                        &catalog,
                        locale,
                        theme,
                    )
                    .unwrap();
                let pixels = render(&device, &queue, &mut renderer, &scene, view);
                assert_eq!(
                    renderer.text_stats().missing_glyphs,
                    0,
                    "CJK fonts required"
                );
                let path = Path::new(&directory).join(format!("nodes-{name}-{language}.ppm"));
                let mut file = std::fs::File::create(&path).unwrap();
                write!(file, "P6\n1280 640\n255\n").unwrap();
                for p in pixels.as_chunks::<4>().0 {
                    file.write_all(&p[..3]).unwrap();
                }
                println!("{}", path.display());
            }
        }
        assert!(device.pop_error_scope().await.is_none());
    });
}
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut GpuRenderer,
    scene: &Scene,
    view: Viewport,
) -> Vec<u8> {
    let size = wgpu::Extent3d {
        width: 1280,
        height: 640,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("node gallery"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    renderer.prepare(device, queue, scene, view).unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render(&mut encoder, &texture.create_view(&Default::default()));
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 1280 * 640 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1280 * 4),
                rows_per_image: Some(640),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    pixels
}
