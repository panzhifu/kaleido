# Kaleido

> AI-native image workstation — a plugin-driven image editor built on [Cordis](https://github.com/dshbox/cordis-rs) and [GPUI](https://github.com/zed-industries/zed).

**English** | [简体中文](README.zh-CN.md)

Kaleido is an image editor under construction. Its architecture is deliberately plugin-first: the host (CLI / desktop) provides only the window, canvas and service container; **every user-facing feature is a plugin** registered into a Cordis-managed tool registry.

## Features

### Core library (`kaleido-core`)
- `Tile` with zero-copy `Arc<Vec<u8>>` cloning and copy-on-write
- `TiledImage` (`HashMap<TileCoord, Tile>`) 128×128 tile-based storage: sparse allocation, tile-parallel processing, dirty tile tracking
- 5 pixel formats (RGBA8 / RGB8 / Gray8 / GrayA8 / RGBA16)
- **SIMD pixel conversion**: RGBA↔Gray, RGBA↔GrayA — 4 paths SIMD-accelerated (`wide` crate, 8 pixels/iteration); RGBA↔RGB is scalar
- Crop, region copy with overlap protection, format conversion
- SIMD-friendly aligned row strides, full-precision RGBA16 mapping

### Service layer (`kaleido-traits` + `kaleido-services`)
- **ImageStore** — single source of truth for the current image (single write path)
- **FileCodec** — JPEG / PNG / WebP / TIFF read+write, BMP / GIF read-only
- **FileCodecRegistry** — per-format codec plugin system (`FormatCodec` trait) exposed as a **Cordis service**; third-party plugins can register new formats (e.g. AVIF) at runtime via dependency injection
- **HistoryKeeper** — undo / redo with bounded snapshot-based commands (default 50 steps)
- **TileHistoryKeeper** — **dirty-tile undo**: stores only modified tiles, memory ∝ modified region (not full image)
- **ToolRegistry** — dynamic registry of tools provided by plugins
- **InteractiveTool** — pointer-event stream contract for brush-like tools (`on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`); the **host** owns screen→image coordinate conversion, undo snapshots, and dirty-tile tracking — plugins only paint into their `ToolContext`
- **InteractiveToolRunner** — stroke executor that sits between the canvas and the plugin: per-stroke working buffer, automatic undo-tile capture on `begin_stroke`, dirty-tile accumulation, and `HistoryKeeper` commit on `end_stroke`
- **Op Graph** — GEGL-like operation graph: DAG structure, topological sort, ROI-driven lazy evaluation
- **GraphExecutor** — Tile-parallel execution (rayon), automatic point-op fusion
- **CanvasService** — canvas service: viewport math (zoom/pan/rotate) and visible tile calculation; actual GPU rendering is delegated to the host (desktop)
- **ProgressiveRenderer** — Progressive rendering: Low → Medium → High quality
- **AsyncImageLoader** — tokio async loading: progressive preview (512px → full res), 3 priority strategies
- **BackgroundSaver** — Background save without blocking UI
- **LayerStack** — Layer system: pixel layers + adjustment layers (non-destructive), 13 blend modes, basic mask support (with mask inversion)
- **BlendMode SIMD** — 11 blend modes SIMD-optimized (Normal/Multiply/Screen/Overlay/Darken/Lighten/Difference/Exclusion/ColorDodge/ColorBurn/SoftLight)
- **AIAgent** — template-driven planner (MVP): keyword → tool sequence; interface reserves LLM mode (`AgentMode::Template/Llm/Hybrid`)
- Typed event system unified on Cordis (14 event names + typed payloads, lifecycle-managed subscriptions)

### Plugin contracts
- `Tool` trait with **parameter schemas** (`ParamType` / `ParamSchema` / `ToolSchema`): auto-generated UI forms, validation and default values
- **`InteractiveTool` trait** — extends `Tool` with a pointer-event stream (`on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`); delivers pre-converted image-space coordinates, pressure, button and modifier state via `PointerEvent`; plugins paint into `ToolContext` and record dirty tiles, while the host owns undo and repaint
- **WIT interface** (`wit/kaleido.wit`) — WASM boundary: `tool`, `plugin-lifecycle`, `host-functions` interfaces + `world kaleido-plugin`
- **Plugin host** (`kaleido-plugin-host`) — `PluginManifest`, `Plugin`/`PluginLoader` traits, `PluginManager`, and `AIToolGenerator` for dynamically generated tools
- **WASM runtime** — compiled `.wasm` plugins are loaded and executed via **wasmtime**: `WasmPluginManager` scans plugin directories, instantiates modules (C ABI: `plugin_init` / `tool_apply` / …), and registers every tool into the registry. Host functions (`host_log`, `host_emit_event`) are linked in
- **Plugin SDK** (`kaleido-sdk`) — `ToolPlugin<T>` builder + `define_tool!` macro
- **AI tool generation** — `KaleidoApp::create_ai_tool(description, apply_fn)` registers a tool from a JSON description and emits `tool_upgraded`

### Applications
- **`kaleido-cli`** — image info / convert / list-formats / brightness / invert / resize / grayscale, plus plugin commands: `list-tools`, `tool-schema`, `run` (custom params), `create-tool` (AI-generated tools)
- **`kaleido-desktop`** — GPUI host with a canvas rendering directly from the `ImageStore`, a **toolbar generated dynamically from the plugin registry**, an active `InteractiveTool` receiving pointer events, full **keyboard shortcuts** (Ctrl+Z undo, Ctrl+Shift+Z redo, Ctrl+O open, Ctrl+S save, Ctrl+Shift+S save as), a **menu bar** (File / Edit / View / Mode / Help), and a **status bar** showing undo/redo step counts and file operation feedback

### Plugin system
- `Tool` contract (`kaleido-traits`) — plugins implement `name` / `menu_path` / `description` / `apply`
- `InteractiveTool` contract — pointer-driven tools implement `on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`
- Cordis service plugins with dependency injection (`Inject`) and fiber-managed lifetimes
- Example plugins: [`plugins/examples/brightness`](plugins/examples/brightness), [`plugins/examples/invert`](plugins/examples/invert), [`plugins/examples/brush`](plugins/examples/brush) (interactive round brush with pressure sensitivity)
- Installing / uninstalling a plugin adds / removes commands dynamically — no host changes

## Architecture

```
                    ┌────────────────────────────────────────┐
                    │  Host (CLI / GPUI desktop)             │
                    │  window · canvas · service container   │
                    └───────────────┬────────────────────────┘
                                    │
                          ToolRegistry (Cordis service)
                    ┌───────────────┼───────────────┐
                    ↓               ↓               ↓
             Tool plugins      Core services     Future: WASM plugins
        (brightness, invert)  ImageStore · FileCodec
                              HistoryKeeper · ToolRegistry
```

- **Core services are not plugins** — they are host infrastructure that plugins depend on via `Inject`.
- **Tools are plugins** — every menu command comes from the registry; the host never hard-codes one.
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
    fn menu_path(&self) -> String { "调整/反相".into() }
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

See [`plugins/examples/invert`](plugins/examples/invert) for the complete example.

## Project layout

```
crates/
  kaleido-core/        Image data model (TiledImage, Tile, Pixel, SIMD conversion)
  kaleido-traits/      Contracts: FileCodec, ImageStore, HistoryKeeper, Tool, InteractiveTool, events
  kaleido-services/    Implementations + Cordis plugins + application container (KaleidoApp)
                      (InteractiveToolRunner, Op Graph, Layer, Tile History, Blend SIMD)
  kaleido-sdk/         Plugin SDK: ToolPlugin builder + define_tool! macro
  kaleido-plugin-host/ Plugin host: manifest/loader/manager + wasmtime runtime + AIToolGenerator
apps/
  cli/                Command-line image tool
  desktop/            GPUI desktop host (canvas, toolbar, menu bar, status bar)
plugins/examples/
  brightness/         Brightness tool plugin (with parameter schema)
  invert/             Invert tool plugin
  brush/              Interactive round brush plugin (pressure-sensitive, stroke interpolation)
wit/                  WASM interface definitions (tool, lifecycle, host functions)
docs/                 Architecture docs
tests/                Integration test fixtures (placeholder)
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
- [x] Plugin host framework (`kaleido-plugin-host`) + `AIToolGenerator`
- [x] WIT interface definitions for the WASM boundary
- [x] WASM runtime (`wasmtime`) — load compiled `.wasm` tool plugins
- [x] GPUI desktop host with dynamic plugin toolbar, menu bar, keyboard shortcuts, and file I/O

### TODO
- [ ] Example WASM tool plugin (compile a tool to `.wasm` and load it)
- [ ] AI-generated tools end-to-end (generate → compile → load → `tool_upgraded`)
- [ ] Plugin UI panels
- [ ] Advanced blend modes (Hard Light SIMD optimization)
- [ ] Mask system enhancements (feathering, vector masks, etc.)

## License

MIT — see [LICENSE](LICENSE).
