# Kaleido

> AI-native image workstation — a plugin-driven image editor built on [Cordis](https://github.com/dshbox/cordis-rs) and [GPUI](https://github.com/zed-industries/zed).

**English** | [简体中文](README.zh-CN.md)

Kaleido is an image editor under construction. Its architecture is deliberately plugin-first: the host (CLI / desktop) provides only the window, canvas and service container; **every user-facing feature is a plugin** registered into a Cordis-managed tool registry.

## Five Editing Modes, One Data Model

Kaleido supports **five editing modes** — Vector, Pixel, Painting, Typography, and Animation — all operating on a unified scene-graph document model. Switching modes is just changing the editing perspective; the underlying data structure stays the same.

## Features

### Core library (`kaleido-core`) — data structures only

This crate contains **only data structures** — no service logic, no plugin framework, no dependency injection.

- **Foundational types** (`types.rs`) — `Point` / `Size` / `Color` (f32 RGBA) / `Transform2D` (translate+rotate+scale, animation-friendly) / `BlendMode` (16 variants) / stable IDs (`NodeId` / `DocumentId` / `ResourceId` / `EffectId`)
- **Tile-based raster** (`tile_core.rs` + `tile.rs`) — `Tile` (256×256 fixed buffer, **Arc copy-on-write + dirty flag**), `TiledImage` (sparse tile map: only allocated for painted regions), pixel read/write, batch fill, crop, region copy, format conversion
- **Scene graph** (`scene.rs`) — `Scene` object tree with add/delete/reparent operations, cycle-reference prevention, reorder, subtree removal, tree validation; `Node` with `transform` / `opacity` / `visible` / `locked` / `blend` / `mask` / `effects`
- **Node contents** — `PixelLayer` + `FramePixels` (per-frame tile snapshots: static = 1 frame, animation = multi-frame, unmodified frames Arc-shared), `VectorObject` (node-style bezier paths with anchor + control points, FillStyle / StrokeStyle), `TextObject` (rich text `TextRun` with font/size/bold/italic, alignment, fixed-width frame)
- **Masks & selection** (`mask.rs`) — `Mask` (layer mask / vector mask) + `SelectionMask` (grayscale mask, None = full selection) — **same grayscale structure, interchangeable** (Photoshop model)
- **Animation** (`timeline.rs`) — dual-track: **frame-by-frame** (Krita-style, via `PixelLayer.frames`) + **property keyframes** (AE-style, via `Timeline.tracks` with `Keyframe` / `Easing` / `AnimValue`)
- **Effects** (`effects.rs`) — `EffectBinding` (plugin-provided effect ID + JSON params + scope) / `EffectScope` (SelfOnly filter / Subtree adjustment-layer semantics); adjustment layers are **not built-in nodes** but plugin effect chains
- **Color management** (`color_profile.rs`) — `ColorSpace` (sRGB / linear / CMYK / Lab) + bit depth + ICC profile reference
- **Document format** (`format.rs`) — `.kld` native format with magic number, version, chunk-based (document + thumbnail)
- **Document** (`document.rs`) — top-level aggregate: `size` / `dpi` / `color_profile` / `scene` / `selection` / `history` / `timeline` / `resources` / `metadata`

### Service layer (`kaleido-traits` + `kaleido-services`) — 12 managers

| # | Manager | Service ID | Responsibility |
|---|---------|-----------|----------------|
| 1 | **Data** | `data_service` | Document lifecycle, single write path (`apply_mutation`), undo snapshot restore, export |
| 2 | **History** | `history_service` | Undo/redo with COW snapshot restoration |
| 3 | **Layer** | `layer_service` | Add/remove/reorder/rename/opacity/blend on scene nodes |
| 4 | **Selection** | `selection_service` | Selection mask: set/clear/invert/union/intersect/subtract |
| 5 | **Color** | `color_service` | Color profile, swatch management |
| 6 | **Render** | `render_service` | Scene compositing (bottom-up, blend+opacity), export flattened |
| 7 | **Plugin** | `plugin_service` | Plugin manifest/loading/lifecycle, WASM runtime (wasmtime), tool registry |
| 8 | **App** | `app_service` | App name/version, editing mode, notifications |
| 9 | **Resource** | `resource_service` | Font/swatch/brush resource management |
| 10 | **Shortcut** | `shortcut_service` | Global/mode/plugin keyboard shortcut registration & resolution |
| 11 | **UI** | `ui_service` | Status bar, notifications, panel registration |
| 12 | **Task** | `task_service` | Background task spawn/progress/cancel/join |

**Core design — single write path**: All document mutations go through `DataService::apply_mutation(label, f)`:
1. COW-clone current Document as "before" snapshot (Arc-shared tiles, zero-cost)
2. Execute `f(&mut Document)` (if it fails: no change, no record)
3. Push "before" snapshot to undo stack; clear redo stack
4. Emit `document_changed`

**Legacy services** (based on the old `TiledImage` model) are preserved for backward compatibility: `ImageStore`, `HistoryKeeper`, `LayerStore`, `FileCodecRegistry`, `ToolRegistry`, `PanelRegistry`, `AIAgent`, `InteractiveToolRunner`, `OpGraph`, `BlendMode SIMD`, `AsyncImageLoader`, `BackgroundSaver`. These will be removed once the desktop host migrates to the new model.

### Plugin contracts
- `Tool` trait with **parameter schemas** (`ParamType` / `ParamSchema` / `ToolSchema`): auto-generated UI forms, validation and default values
- **`Tool` metadata** — `icon()` for toolbar display, `category()` for functional grouping (11 categories: Selection/Transform/Painting/ColorAdjustment/Retouch/Fill/Vector/Text/Analysis/Navigation/Other), `is_enabled()` for contextual availability, `cursor()` for cursor appearance (18 cursor types)
- **`InteractiveTool` trait** — extends `Tool` with a pointer-event stream (`on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`); delivers pre-converted image-space coordinates, pressure, button and modifier state via `PointerEvent`; plugins paint into `ToolContext` and record dirty tiles, while the host owns undo and repaint
- **`InteractiveTool` keyboard** — `on_key_down()` / `on_key_up()` with `KeyEvent` / `KeyCode` / `KeyModifiers`; `on_escape()` for stroke cancellation; `is_stroke_active()` for stroke state queries
- **`InteractiveTool` lifecycle** — `on_activate()` / `on_deactivate()` for state setup/cleanup when switching tools
- **`SelectionTool` trait** — produces a `Selection` (pixel mask) rather than modifying pixels; `on_begin()` / `on_update()` / `on_end()` with `SelectionMode` (Replace/Add/Subtract/Intersect)
- **`Panel` trait** — plugins render custom UI in the host's side panel via 12 element types (Label/Heading/NumberInput/Checkbox/Dropdown/ColorPicker/ButtonRow/Canvas/Progress/Section); `on_change()` / `on_button()` for interactivity
- **`AnalysisTool` trait** — read-only tools that inspect pixels (histogram, colour picker, measurement); `analyze()` returns JSON result
- **`SelectionState`** — shared selection state with `contains()`, `bounds()`, `invert()`; flat `Vec<bool>` mask for O(1) pixel testing
- **`Selection-constrained rendering`** — `apply_to_selection()` processes only tiles intersecting the selection (6100 tiles → 16 tiles for a 500×500 selection in 10000×10000 image)
- **WIT interface** (`wit/kaleido.wit`) — WASM boundary: `tool`, `plugin-lifecycle`, `host-functions` interfaces + `world kaleido-plugin`
- **WASM runtime** — compiled `.wasm` plugins are loaded and executed via **wasmtime**: `WasmPluginManager` scans plugin directories, instantiates modules (C ABI: `plugin_init` / `tool_apply` / …), and registers every tool into the registry. Host functions (`host_log`, `host_emit_event`) are linked in
- **WASM selection optimization** — when a selection is set, only the selection bounding box region is exchanged with WASM (not the full image), reducing data transfer by 99%+ for localized operations
- **Plugin SDK** (`kaleido-sdk`) — `ToolPlugin<T>` builder + `define_tool!` macro
- **AI tool generation** — `KaleidoApp::create_ai_tool(description, apply_fn)` registers a tool from a JSON description and emits `tool_upgraded`

### Applications
- **`kaleido-cli`** — image info / convert / list-formats / brightness / invert / resize / grayscale, plus plugin commands: `list-tools`, `tool-schema`, `run` (custom params), `create-tool` (AI-generated tools)
- **`kaleido-desktop`** — GPUI host with a canvas rendering directly from the `ImageStore`, a **toolbar generated dynamically from the plugin registry**, an active `InteractiveTool` receiving pointer events, full **keyboard shortcuts** (Ctrl+Z undo, Ctrl+Shift+Z redo, Ctrl+O open, Ctrl+S save, Ctrl+Shift+S save as), a **menu bar** (File / Edit / View / Mode / Help), a **status bar** showing undo/redo step counts and file operation feedback, and a **dock system** (replacing the old fixed panel layout) for flexible panel arrangement

### Plugin system
- `Tool` contract (`kaleido-traits`) — plugins implement `name` / `menu_path` / `description` / `apply`
- `InteractiveTool` contract — pointer-driven tools implement `on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`
- Cordis service plugins with dependency injection (`Inject`) and fiber-managed lifetimes
- Example plugins: [`plugins/examples/tga`](plugins/examples/tga) (TGA format codec plugin), [`plugins/wasm/simple_format`](plugins/wasm/simple_format) (WASM plugin example)
- Installing / uninstalling a plugin adds / removes commands dynamically — no host changes

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Host (CLI / GPUI desktop)                 │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ AI Agent    │  │ Canvas       │  │ Tool Registry           │ │
│  │ Service     │  │ Service      │  │ (Cordis service)        │ │
│  │ (template)  │  │ (viewport)   │  │ ← WASM / native / AI    │ │
│  └──────┬──────┘  └──────┬───────┘  └─────────────────────────┘ │
│         │                │                                       │
│         ▼                ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    12 Service Managers                      │ │
│  │  Data · History · Layer · Selection · Color · Render        │ │
│  │  Plugin · App · Resource · Shortcut · UI · Task             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                       │
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Document (unified model)                  │ │
│  │  Scene Graph → Node [PixelLayer | VectorObject | Text | Group]│
│  │  256×256 sparse tiles · Arc COW · dirty tracking            │ │
│  │  Timeline (dual-track) · Mask/Selection · Effects           │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

- **Core services are not plugins** — they are host infrastructure that plugins depend on via `Inject`.
- **Tools are plugins** — every menu command comes from the registry; the host never hard-codes one.
- **Single write path** — all document mutations go through `DataService::apply_mutation`; undo is COW snapshot restoration.
- **Five modes, one model** — Vector / Pixel / Painting / Typography / Animation all operate on the same scene graph.
- All events are dispatched through Cordis (`Context::emit` / `Context::on`); subscriptions are auto-removed when their plugin fiber is disposed.

## Quick start

```sh
# Build & test
cargo build --workspace
cargo test --workspace

# CLI
cargo run -p kaleido-cli -- info path/to/image.png
cargo run -p kaleido-cli -- list-formats
cargo run -p kaleido-cli -- brightness --value 40 in.png out.png
cargo run -p kaleido-cli -- invert in.png out.png

# Desktop (opens a window; optionally pass an image path)
cargo run -p kaleido-desktop [path/to/image.png]
```

> Git dependencies (GPUI, gpui-component) are fetched over HTTPS; if GitHub HTTPS is unreachable, configure git to route through SSH:
> `git config --global url."ssh://git@github.com/".insteadOf "https://github.com/"`

## Writing a tool plugin

Tools are the unit of plugin functionality. A plugin is a crate that implements the `Tool` trait and registers itself when its Cordis fiber activates:

```rust
use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_core::{ImageResult, Pixel, TiledImage};
use kaleido_traits::{Tool, ToolParams, ToolRegistry};
use std::sync::Arc;

pub struct InvertTool;

impl Tool for InvertTool {
    fn name(&self) -> &str { "invert" }
    fn menu_path(&self) -> String { "Adjust/Invert".into() }
    fn description(&self) -> String { "Invert all pixel colours".into() }
    fn apply(&self, image: &mut TiledImage, _params: &ToolParams) -> ImageResult<()> {
        for y in 0..image.height() {
            for x in 0..image.width() {
                let p = image.get_pixel(x, y);
                image.set_pixel(x, y, Pixel::new(255 - p.r, 255 - p.g, 255 - p.b, p.a));
            }
        }
        Ok(())
    }
}

pub fn invert_tool_plugin() -> PluginHandle {
    plugin_sync::<(), _>("tool.invert", Inject::new(["tool_registry"]), |ctx, _config| {
        let registry: Arc<dyn ToolRegistry> = kaleido_traits::resolve_tool_registry(&ctx)?;
        let tool: Arc<dyn Tool> = Arc::new(InvertTool);
        registry.register(Arc::downgrade(&tool));
        Ok(PluginOutput::disposer(move || {
            registry.unregister("invert");
            drop(tool);
            Ok(())
        }))
    })
}
```

See [`plugins/examples/tga`](plugins/examples/tga) for a complete format codec plugin example.

## Project layout

```
crates/
  kaleido-core/         Document data model (only data structures)
                         types · tile_core · tile · pixel · pixel_layer
                         scene · vector · text · mask · timeline · effects
                         color_profile · document · format · conversion
  kaleido-traits/       Contracts: 12 service traits + legacy tool contracts
                         services/ (data, history, layer, selection, color,
                                    render, plugin, app, resource, shortcut,
                                    ui, task)
                         (legacy: tool, interactive_tool, panel, selection_tool,
                                  analysis_tool, image_store, history_keeper,
                                  file_codec, ai_agent, events, keyboard)
  kaleido-services/     Implementations: 12 service managers + legacy services
                         managers/ (data, history, layer, selection, color,
                                    render, app, resource, shortcut, ui, task)
                         plugin_service/ (manifest, loader, manager, wasmtime)
                         (legacy: image_store, file_codec, history_keeper,
                                  layer_store, tool_registry, panel_registry,
                                  ai_agent, interactive_tool, op_graph,
                                  blend_simd, async_io)
  kaleido-sdk/          Plugin SDK: ToolPlugin builder + define_tool! macro
apps/
  cli/                  Command-line image tool
  desktop/              GPUI desktop host (canvas, toolbar, status bar, dock)
plugins/
  examples/tga/         TGA format codec plugin
  wasm/simple_format/   WASM plugin example (compiled .wasm + .wit)
wit/                    WASM interface definitions (tool, lifecycle, host functions)
docs/                   Architecture docs (refactor overview, data model,
                         service layout, services refactor)
tests/                  Integration test fixtures (placeholder)
```

## Roadmap

### Completed
- [x] Core image library (Tile + TiledImage + SIMD pixel conversion)
- [x] Service layer (store / codec / history / events) on Cordis
- [x] **Op Graph execution engine** (DAG, ROI-driven, tile-parallel, point-op fusion)
- [x] **Canvas service** (viewport math, progressive rendering; GPU rendering delegated to desktop)
- [x] **Async I/O** (AsyncImageLoader + BackgroundSaver)
- [x] **Dirty-tile undo** (TileHistoryKeeper, memory ∝ modified region)
- [x] **Layer system** (LayerStack + 13 blend modes)
- [x] **SIMD blend modes** (11 modes, 8 pixels/iteration)
- [x] Tool plugin contract + example plugins (native, in-process)
- [x] **InteractiveTool contract** (pointer-event stream, `ToolContext`, dirty-tile tracking)
- [x] **InteractiveToolRunner** (stroke executor with undo, working buffer, dirty tracking)
- [x] **Brush reference plugin** (round brush, pressure sensitivity, stroke interpolation)
- [x] Tool parameter schemas (auto-generated UI forms)
- [x] File format codec plugin system
- [x] Plugin SDK (`kaleido-sdk`): `ToolPlugin` builder + `define_tool!` macro
- [x] Plugin host framework (in `kaleido-services` / `plugin_service`) + `AIToolGenerator`
- [x] WIT interface definitions for the WASM boundary
- [x] WASM runtime (`wasmtime`) — load compiled `.wasm` tool plugins
- [x] GPUI desktop host with dynamic plugin toolbar, menu bar, keyboard shortcuts, and file I/O
- [x] **Unified Document data model** (Scene Graph, PixelLayer, VectorObject, TextObject, Mask/Selection, Timeline, Effects, ColorProfile)
- [x] **Five editing modes** (Vector / Pixel / Painting / Typography / Animation) on one unified model
- [x] **12 service managers** (Data, History, Layer, Selection, Color, Render, Plugin, App, Resource, Shortcut, UI, Task) with single-write-path architecture
- [x] **Dock system** replacing fixed panel layout in desktop host
- [x] **TGA codec plugin** example
- [x] **WASM simple_format plugin** example (compile to `.wasm` and load via wasmtime)
- [x] **Document format** (`.kld` chunk-based native format)

### TODO
- [ ] Desktop host migration to new Document model
- [ ] AI-generated tools end-to-end (generate → compile → load → `tool_upgraded`)
- [ ] Advanced blend modes (Hard Light SIMD optimization)
- [ ] Mask system enhancements (feathering, vector masks, etc.)
- [ ] Selection overlay rendering (marching ants animation)
- [ ] Brush engine presets (texture, dynamics, blending)
- [ ] Text engine details: vertical layout / RTL / line-letter spacing
- [ ] Animation memory strategy (frame limit, unmodified frame sharing details)
- [ ] `.kld` serialization format finalization

## License

MIT — see [LICENSE](LICENSE).
