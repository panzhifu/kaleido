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
| 1 | **Data** | `data_service` | Document lifecycle, file I/O, undo snapshot restore |
| 2 | **History** | `history_service` | Undo/redo with COW snapshot restoration |
| 3 | **Layer** | `layer_service` | Add/remove/reorder/rename/opacity/blend on scene nodes |
| 4 | **Selection** | `selection_service` | Selection mask: set/clear/invert/union/intersect/subtract |
| 5 | **Color** | `color_service` | Color profile, swatch management |
| 6 | **Render** | `render_service` | Scene compositing (bottom-up, blend+opacity), export flattened |
| 7 | **Plugin** | `plugin_service` | Plugin manifest/loading/lifecycle, WASM runtime (wasmtime), tool registry |
| 8 | **App** | `app_service` | App name/version, editing mode, notifications, **software configuration** |
| 9 | **Resource** | `resource_service` | Font/swatch/brush resource management |
| 10 | **Shortcut** | `shortcut_service` | Global/mode/plugin keyboard shortcut registration & resolution |
| 11 | **UI** | `ui_service` | Status bar, notifications, panel registration |
| 12 | **Task** | `task_service` | Background task spawn/progress/cancel/join |

**Core design — single write path**: All document mutations go through `DataService`:
1. COW-clone current Document as "before" snapshot (Arc-shared tiles, zero-cost)
2. Execute mutation (if it fails: no change, no record)
3. Push "before" snapshot to undo stack; clear redo stack

### Plugin contracts (`kaleido-traits/plugins/`)

- `Tool` trait with **parameter schemas** (`ParamType` / `ParamSchema` / `ToolSchema`)
- **`Tool` metadata** — `icon()`, `category()` (11 categories), `is_enabled()`, `cursor()` (18 cursor types)
- **`InteractiveTool` trait** — pointer-event stream (`on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`)
- **`SelectionTool` trait** — produces `Selection` pixel mask
- **`Panel` trait** — plugins render custom UI in side panels
- **`AnalysisTool` trait** — read-only tools (histogram, color picker, measurement)
- **WIT interface** (`wit/kaleido.wit`) — WASM boundary for sandboxed plugins
- **Plugin SDK** (`kaleido-sdk`) — `ToolPlugin<T>` builder + `define_tool!` macro

### Applications
- **`kaleido-cli`** — demonstrates the 12-manager architecture with `status` and `demo` commands
- **`kaleido-desktop`** — GPUI host with canvas rendering, resizable dock panels, keyboard shortcuts, file open/save, undo/redo, zoom, layer management

### Desktop UI
- **Resizable dock panels** — library-powered `DockArea` + `DockLayout` with drag-to-resize splits (left tools | center canvas | right layers/color)
- **Layers panel** — add/remove/select layers, visual layer stack with active highlighting
- **Color panel** — foreground color swatch, document info (dimensions, layer count)
- **Tool panel** — tool buttons (move tool) with active state
- **Status bar** — live display of editing mode, layer count, history depth (undo/redo), zoom level
- **Menu bar** — File (open/save/save as/exit), Edit (undo/redo), View (zoom in/out/fit), Mode (pixel/vector/paint/type/animation), Help (about)
- **Canvas zoom** — zoom in/out/fit-to-window with real-time preview
- **16 blend modes** — Normal, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity
- **Bitmap export** — save to PNG/JPEG/WebP/TIFF via codec registry

### Plugin system
- Example plugins: [`plugins/examples/tga`](plugins/examples/tga) (TGA format codec), [`plugins/wasm/simple_format`](plugins/wasm/simple_format) (WASM plugin)
- Installing / uninstalling a plugin adds / removes commands dynamically

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Host (CLI / GPUI desktop)                 │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    12 Service Managers                      │ │
│  │  Data · History · Layer · Selection · Color · Render        │ │
│  │  Plugin · App · Resource · Shortcut · UI · Task             │ │
│  │                    (Cordis services)                         │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                       │
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Document (unified model)                  │ │
│  │  Scene Graph → Node [PixelLayer | VectorObject | Text | Group]│
│  │  256×256 sparse tiles · Arc COW · dirty tracking            │ │
│  │  Timeline (dual-track) · Mask/Selection · Effects           │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │              Plugin Registry (Cordis service)                │ │
│  │  ← WASM / native / AI-generated tools                       │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

- **Core services are not plugins** — they are host infrastructure that plugins depend on via `Inject`.
- **Tools are plugins** — every menu command comes from the registry; the host never hard-codes one.
- **Single write path** — all document mutations go through `DataService`; undo is COW snapshot restoration.
- **Five modes, one model** — Vector / Pixel / Painting / Typography / Animation all operate on the same scene graph.
- All events are dispatched through Cordis (`Context::emit` / `Context::on`).

## Quick start

```sh
# Build & test
cargo build --workspace
cargo test --workspace

# CLI
cargo run -p kaleido-cli -- status     # Show service layer status
cargo run -p kaleido-cli -- demo       # Run end-to-end workflow demo

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
use kaleido_traits::plugins::{Tool, ToolParams};
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
        let registry = ctx.require::<Arc<dyn kaleido_traits::plugins::ToolRegistry>>("tool_registry")?;
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
  kaleido-traits/       Service contracts + plugin contracts
                         services/      (data, history, layer, selection, color,
                                         render, plugin, app, resource, shortcut,
                                         ui, task)
                         plugins/       (tool, panel, events, category, cursor)
  kaleido-services/     Implementations of 12 service managers
                         app/ · color/ · data/ · history/ · layer/
                         plugin/ · render/ · resource/ · selection/
                         shortcut/ · task/ · ui/
  kaleido-sdk/          Plugin SDK: ToolPlugin builder + define_tool! macro
apps/
  cli/                  CLI demo of 12-manager architecture
  desktop/              GPUI desktop host (canvas, resizable dock, layers, status bar)
  desktop/src/dock/     Dock panels (tool panel, layers panel, color panel) using library DockArea
plugins/
  examples/tga/         TGA format codec plugin
  wasm/simple_format/   WASM plugin example (compiled .wasm + .wit)
wit/                    WASM interface definitions (tool, lifecycle, host functions)
docs/                   Architecture docs
tests/                  Integration test fixtures (placeholder)
```

## Roadmap

### Completed
- [x] Core image library (Tile + TiledImage + SIMD pixel conversion)
- [x] **Unified Document data model** (Scene Graph, PixelLayer, VectorObject, TextObject, Mask/Selection, Timeline, Effects, ColorProfile)
- [x] **12 service managers** (Data, History, Layer, Selection, Color, Render, Plugin, App, Resource, Shortcut, UI, Task)
- [x] **Five editing modes** (Vector / Pixel / Painting / Typography / Animation) on one unified model
- [x] **Desktop host** with Canvas rendering, dock system, file I/O, undo/redo
- [x] **AppSettings management** (default canvas size, undo limit, auto-save, plugin dirs, default mode)
- [x] **Plugin contracts** organized into `plugins/` submodule
- [x] **TGA codec plugin** example
- [x] **WASM simple_format plugin** example
- [x] **Document format** (`.kld` chunk-based native format)
- [x] **197 tests** across workspace
- [x] **16 blend modes** (Normal, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity)
- [x] **Resizable dock panels** with visible drag handles
- [x] **Layers panel** with add/remove/select
- [x] **Bitmap export** (PNG/JPEG/WebP/TIFF)
- [x] **Canvas zoom** (zoom in/out/fit)
- [x] **WASM plugin** fixed (proper bump allocator)

### TODO
- [ ] Mask system enhancements (feathering, vector masks)
- [ ] Selection overlay rendering (marching ants animation)
- [ ] Brush engine presets (texture, dynamics, blending)
- [ ] Text engine details: vertical layout / RTL / line-letter spacing
- [ ] Animation memory strategy (frame limit, unmodified frame sharing)
- [ ] `.kld` serialization format finalization
- [ ] AI-generated tools end-to-end

## License

MIT — see [LICENSE](LICENSE).
