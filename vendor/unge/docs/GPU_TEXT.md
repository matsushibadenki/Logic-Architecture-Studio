# GPU text / GPU文字 / GPU文字渲染

## 日本語

`unge-render` はcosmic-text 0.19による文字整形・システムフォントのフォールバックと、wgpu 27のGlyph Atlasを使います。文字列の整形、ラスタライズ、キャッシュ、GPU転送はすべてRust内で行います。WebViewとの通信は言語コード等の小さな命令だけです。

cosmic-textの再公開型も0.19です。ホストで旧版のBufferを直接操作していた場合、`set_wrap`／`set_text`のFontSystem引数を取り除き、`shape_until_scroll`を呼んでから`layout_runs`を参照してください。内部も同じ順序で整形してからキャッシュします。システムフォントやfallbackの変更により、旧版と字形・幅が完全に一致することは保証しません。Rendererの呼び出し契約とAtlasの容量制限は維持します。

### ホストへの取り込み

1. `SceneIndex::new(document)` を編集確定時に構築します。`scene` / `scene_with_preview` は既定でtype_idとPort識別子を表示します。
2. 翻訳した表示名はホストから `LabelCatalog`（type_id → `NodeLabels`）で渡します。`title` と `inputs` / `outputs` の各値は `LabelText { en, ja, zh_cn }` です。Portマップのキーは接続に使う元のPort識別子です。空の翻訳・未登録の型/Portは元の識別子に戻ります。
3. Tauriでは `Engine::set_labels(catalog)` で登録します。Viewごとに `Request::SetLocale { locale }`、TypeScriptでは `client.locale('ja')` を呼びます。wire形式は `en` / `ja` / `zh-cn`。Documentのrevision・Undo・ACX契約は変わりません。Rustから設定した後の再描画はホストで要求します。
4. Tauriを使わない場合は `scene_with_labels(viewport, selection, preview, catalog, locale)` を使います。テクスチャ描画では `GpuRenderer::prepare_sized(device, queue, scene, viewport, physical_size)` → `render` → submitの順に呼びます。`prepare` は1論理px=1物理px用です。
5. `SurfaceRenderer::draw` は物理Surfaceサイズから文字解像度を決めます。Tauriホストは従来どおり `Engine::draw_scaled` を使用します。Viewportは論理座標です。縦横で異なる拡大率のテクスチャは受け付けません（丸め誤差1%以内）。

**フォントはホストの配布責任です。** 既定ではOSのフォントを読み込みます。日本語・简体中文フォントがない環境では、ホストで再配布可能なフォントを用意してください。このリポジトリはOSフォントをコピー・同梱しません。地域ごとの字形を固定する場合もホストのFontSystemを注入します。

```rust
use unge_render::cosmic_text::{FontSystem, fontdb};

let mut db = fontdb::Database::new();
// パスとファミリー名は、ホストが配布するライセンス確認済みフォントに合わせる。
db.load_font_file("assets/fonts/NotoSansCJK-Regular.ttc")?;
db.set_sans_serif_family("Noto Sans CJK JP");
let fonts = FontSystem::new_with_locale_and_db("ja-JP".into(), db);
let renderer = unge_render::GpuRenderer::with_font_system(&device, format, fonts);
```

既存のGpuRenderer / SurfaceRendererでは `set_font_system` を使えます。設定時に文字キャッシュが無効になり、次の描画前にprepareが必要です。`text_stats()` でグリフ数・欠落グリフ数・キャッシュ件数を照会できます。欠落数は整形結果のglyph ID 0と、フォント不在時の未整形文字を数えます。すべての異体字や結合文字の正しさを保証する検査ではありません。

### 表示と容量

- ノード名は14、Port名は11ワールド単位。タイトルはzoom ≥ 0.6、Port名は ≥ 0.75で表示します。小さいノードや密集したPort列では文字を省略し、円の位置とHit Testは維持します。
- 表示名は最大256 Unicode scalar値、改行・制御文字は空白に置換。1行表示で、長い名前は矩形内にクリップします。省略記号・全文ツールチップは未実装です。出力名は右寄せします。
- 移動プレビューの位置を使い、タイトルとPort名を各ノードの直後に描きます。前面ノードは背面文字を隠し、選択矩形・仮接続は文字の後に描きます。このため文字のあるノードごとに描画バッチを分けます。1 draw callで全ノードを描く構成ではありません。
- Atlasは固定2048²のR8（4MiB）、グリフインスタンスは最大65536（3MiB）。整形結果は最大1024ラベルのFIFO、Atlasキーは最大16384件です。CPU側にラスタ画像を重複保持しません。フォントデータと文字整形ライブラリ内部の保持量はこの上限に含みません。
- 1フレーム最大16384ラベル。Atlasが埋まったらUVを破棄し、フレーム全体を一度だけ再構築します。それでも収まらなければエラーを返します。未送信の描画コマンドがある間に次フレームのprepareを呼ばず、prepare→render→submitの順序を守ってください。
- ラスタサイズは最大512px。さらに拡大する場合はその画像を拡大表示します。色付き絵文字も単色の輪郭として扱います。テーマは [THEMES.md](THEMES.md) のDark/Lightに対応します。アクセシビリティ、独立したカラー絵文字表示は未実装です。

### 検証

通常テストは翻訳・識別子維持・クリップ矩形・描画順・LOD・プレビュー・View単位の言語を確認します。実GPUテストは別途実行します。

```sh
cargo test --locked --offline -p unge-render --test render -- --ignored
```

CJKフォントを持つ実機で、英語・日本語・简体中文のグリフ、文字のクリップ、重なったノードによる遮蔽、1倍/2倍の物理解像度、再描画時のキャッシュ再利用をピクセルで確認します。大規模グラフのFPSはまだ測定していません。

## English

The re-exported cosmic-text types are now version 0.19. Hosts using Buffer directly must remove FontSystem arguments from set_wrap/set_text and call shape_until_scroll before reading layout_runs. Font fallback and glyph metrics may differ from the older release; renderer APIs and atlas limits remain unchanged.

Node and port labels are shaped with cosmic-text and rendered through a Rust-owned wgpu glyph atlas. Provide `LabelCatalog` from the host; missing translations fall back to type/port identifiers. Use `Engine::set_labels` and the per-view `client.locale('en' | 'ja' | 'zh-cn')`. Language changes do not edit the Document, consume undo history or change ACX contracts.

Use `prepare_sized` for physical texture dimensions and logical Viewport coordinates; native SurfaceRenderer handles this automatically. Labels follow drag previews, clip to one line and respect node stacking. Title/port LOD thresholds are 0.6/0.75. Crowded port rows omit labels. Each labelled node adds ordered geometry/text batches; large-graph performance remains unmeasured.

System fonts are the default. Hosts must supply licensed fonts covering their required scripts for reproducible deployment; use `with_font_system` or `set_font_system`. Font files are not bundled here. `text_stats()` reports glyph/cache counts and missing glyphs. Font fallback does not guarantee every regional variant.

Limits: 256 Unicode scalars per label, 16384 labels and 65536 visible glyphs per frame, 1024 cached layouts, a 4MiB R8 atlas and a 3MiB instance buffer. Atlas exhaustion resets and retries the frame once, then returns an error. Font-library memory is outside these limits. Submit each prepared frame before preparing the next. Raster sizes above 512px are magnified from a bounded bitmap; colour glyphs render as monochrome silhouettes. Dark/Light themes are covered in [theme integration](THEMES.md#english). Ellipsis, full-name tooltips and accessibility remain planned.

## 简体中文

重新导出的cosmic-text类型已更新为0.19。直接操作Buffer的宿主需删除set_wrap/set_text中的FontSystem参数，并在读取layout_runs前调用shape_until_scroll。字体回退和字形宽度可能与旧版不同；Renderer API及图集容量限制保持不变。

节点与端口名称由cosmic-text整形，通过Rust持有的wgpu字形图集渲染。宿主提供 `LabelCatalog`，未提供翻译时使用原始类型或端口标识符。使用 `Engine::set_labels` 与每个视图的 `client.locale('zh-cn')`；切换语言不修改Document、撤销历史或ACX契约。

`prepare_sized` 接收物理纹理尺寸，Viewport使用逻辑坐标；原生SurfaceRenderer自动处理。文字跟随拖动预览，在节点内单行裁剪并保持遮挡顺序。标题/端口文字的LOD阈值为0.6/0.75；端口过密时省略文字。包含文字的节点按顺序分批绘制，大规模性能尚未测量。

默认读取系统字体。宿主需为部署环境提供有再分发许可、覆盖所需语言的字体，通过 `with_font_system` / `set_font_system` 注入。本仓库不附带系统字体。`text_stats()` 提供字形、缓存与缺失字形数量；字体回退不保证所有地区字形完全一致。

每个标签最多256个Unicode scalar值；每帧最多16384个标签、65536个可见字形；最多缓存1024个排版结果。R8图集4MiB，实例缓冲区3MiB。图集满时清空并重建该帧一次，仍不足则报错。字体库内部内存不包含在这些限制内。请按prepare→render→submit顺序逐帧调用。超过512px的字形使用有界位图放大；彩色字形显示为单色轮廓。深色/浅色主题见[主题集成](THEMES.md#简体中文)。省略号、完整名称提示和无障碍仍待实现。

## Sources

- [cosmic-text 0.19 source and API](https://docs.rs/crate/cosmic-text/0.19.0)
- [wgpu 27 API](https://docs.rs/wgpu/27.0.1/wgpu/)
