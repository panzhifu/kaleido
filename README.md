# Kaleido

> AI-native image workstation — a plugin-driven image editor built on [Cordis](https://github.com/dshbox/cordis-rs) and [GPUI](https://github.com/zed-industries/zed).

**English** | [简体中文](README.zh-CN.md)

Kaleido is an image editor under construction. Its architecture is deliberately plugin-first: the host (CLI / desktop) provides only the window, canvas and service container; **every user-facing feature is a plugin** registered into a Cordis-managed tool registry.

## Features

### Core library (`kaleido-core`)
- `Image` with zero-copy `Arc<Vec<u8>>` cloning and copy-on-write
- 5 pixel formats (RGBA8 / RGB8 / Gray8 / GrayA8 / RGBA16)
- Zero-copy sub-views, crop, region copy with overlap protection, format conversion
- SIMD-friendly aligned row strides, full-precision RGBA16 mapping

### Service layer (`kaleido-traits` + `kaleido-services`)
- **ImageStore** — single source of truth for the current image (single write path)
- **FileCodec** — JPEG / PNG / WebP read+write, BMP / GIF read-only
- **FileCodecRegistry** — per-format codec plugin system (`FormatCodec` trait); third-party plugins can add new formats (TIFF, AVIF, …) at runtime
- **HistoryKeeper** — undo / redo with bounded snapshot-based commands (default 50 steps)
- **ToolRegistry** — dynamic registry of tools provided by plugins
- Typed event system unified on Cordis (14 event names + typed payloads, lifecycle-managed subscriptions)

### Plugin contracts
- `Tool` trait with **parameter schemas** (`ParamType` / `ParamSchema` / `ToolSchema`): auto-generated UI forms, validation and default values
- **WIT interface** (`wit/kaleido.wit`) — WASM boundary: `tool`, `plugin-lifecycle`, `host-functions` interfaces + `world kaleido-plugin`
- **Plugin host** (`kaleido-plugin-host`) — `PluginManifest`, `Plugin`/`PluginLoader` traits, `PluginManager`, and `AIToolGenerator` for dynamically generated tools
- **Plugin SDK** (`kaleido-sdk`) — `ToolPlugin<T>` builder + `define_tool!` macro
- **AI tool generation** — `KaleidoApp::create_ai_tool(description, apply_fn)` registers a tool from a JSON description and emits `tool_upgraded`

### Applications
- **`kaleido-cli`** — image info / convert / list-formats / brightness / invert / resize / grayscale
- **`kaleido-desktop`** — GPUI host with a canvas and a **toolbar generated dynamically from the plugin registry**

### Plugin system
- `Tool` contract (`kaleido-traits`) — plugins implement `name` / `menu_path` / `description` / `apply`
- Cordis service plugins with dependency injection (`Inject`) and fiber-managed lifetimes
- Example plugins: [`plugins/examples/brightness`](plugins/examples/brightness), [`plugins/examples/invert`](plugins/examples/invert)
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
use kaleido_core::{Image, ImageResult, Pixel};
use kaleido_traits::{Tool, ToolParams, ToolRegistry};
use std::sync::Arc;

pub struct InvertTool;

impl Tool for InvertTool {
    fn name(&self) -> &str { "invert" }
    fn menu_path(&self) -> String { "调整/反相".into() }
    fn description(&self) -> String { "Invert all pixel colours".into() }
    fn apply(&self, image: &mut Image, _params: &ToolParams) -> ImageResult<()> {
        for y in 0..image.height() {
            for x in 0..image.width() {
                let p = image.get_pixel(x, y)?;
                image.set_pixel(x, y, Pixel::new(255 - p.r, 255 - p.g, 255 - p.b, p.a))?;
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
  kaleido-core/        Image data model (pixel buffers, formats, geometry)
  kaleido-traits/      Contracts: FileCodec, ImageStore, HistoryKeeper, Tool, events
  kaleido-services/    Implementations + Cordis plugins + application container (KaleidoApp)
  kaleido-sdk/         Plugin SDK: ToolPlugin builder + define_tool! macro
  kaleido-plugin-host/ Plugin host: manifest/loader/manager + AIToolGenerator
apps/
  cli/                Command-line image tool
  desktop/            GPUI desktop host
plugins/examples/
  brightness/         Brightness tool plugin (with parameter schema)
  invert/             Invert tool plugin
wit/                  WASM interface definitions (tool, lifecycle, host functions)
tests/                Integration test fixtures (placeholder)
```

## Roadmap

- [x] Core image library
- [x] Service layer (store / codec / history / events) on Cordis
- [x] Tool plugin contract + example plugins (native, in-process)
- [x] Tool parameter schemas (auto-generated UI forms)
- [x] File format codec plugin system
- [x] Plugin SDK (`kaleido-sdk`): `ToolPlugin` builder + `define_tool!` macro
- [x] Plugin host framework (`kaleido-plugin-host`) + `AIToolGenerator`
- [x] WIT interface definitions for the WASM boundary
- [x] GPUI desktop host with dynamic plugin toolbar
- [ ] WASM runtime (`wasmtime`) in `kaleido-plugin-host`
- [ ] AI-generated tools end-to-end (generate → compile → load → `tool_upgraded`)
- [ ] Plugin UI panels

## License

MIT — see [LICENSE](LICENSE).
