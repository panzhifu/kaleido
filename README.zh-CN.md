# Kaleido

> AI 原生图像工作站 —— 基于 [Cordis](https://github.com/dshbox/cordis-rs) 和 [GPUI](https://github.com/zed-industries/zed) 的插件驱动图像编辑器。

Kaleido 是一个正在建设中的图像编辑器。它的架构刻意采用**插件优先**的设计：宿主（CLI / 桌面端）只提供窗口、画布和服务容器；**每一个用户可见的功能都是插件**，通过 Cordis 管理的工具注册表动态加载。

## 功能特性

### 核心库（`kaleido-core`）
- `Image` 使用 `Arc<Vec<u8>>` 实现零拷贝克隆与写时复制（COW）
- 5 种像素格式（RGBA8 / RGB8 / Gray8 / GrayA8 / RGBA16）
- 零拷贝子视图、裁剪、带重叠保护的区域复制、格式转换
- SIMD 友好的行对齐、RGBA16 全精度映射

### 服务层（`kaleido-traits` + `kaleido-services`）
- **ImageStore** — 当前图像的"单一数据源"（单一写路径）
- **FileCodec** — JPEG / PNG / WebP 读写，BMP / GIF 只读
- **FileCodecRegistry** — 按格式的编解码插件系统（`FormatCodec` trait）；第三方插件可在运行时注册新格式（TIFF、AVIF…）
- **HistoryKeeper** — 基于有界快照命令的撤销/重做（默认 50 步）
- **ToolRegistry** — 插件提供的工具动态注册表
- 类型化事件系统统一在 Cordis 之上（14 种事件名 + 类型化 payload，订阅随生命周期自动清理）

### 应用层
- **`kaleido-cli`** — 图像信息 / 格式转换 / 列出格式 / 亮度 / 反相 / 缩放 / 灰度化
- **`kaleido-desktop`** — GPUI 宿主：画布 + **从插件注册表动态生成的工具栏**

### 插件体系
- `Tool` 契约（`kaleido-traits`）— 插件实现 `name` / `menu_path` / `description` / `apply`
- **参数 schema**（`ParamType` / `ParamSchema` / `ToolSchema`）— 自动生成 UI 表单、参数校验与默认值
- **WIT 接口**（`wit/kaleido.wit`）— WASM 边界：`tool`、`plugin-lifecycle`、`host-functions` 接口 + `world kaleido-plugin`
- **插件宿主**（`kaleido-plugin-host`）— `PluginManifest`、`Plugin`/`PluginLoader` trait、`PluginManager`、`AIToolGenerator`（动态生成工具）
- **插件 SDK**（`kaleido-sdk`）— `ToolPlugin<T>` builder + `define_tool!` 宏
- **AI 工具生成** — `KaleidoApp::create_ai_tool(描述, 执行函数)` 从 JSON 描述注册工具并发出 `tool_upgraded` 事件
- Cordis 服务插件：依赖注入（`Inject`）+ fiber 生命周期管理
- 示例插件：[`plugins/examples/brightness`](plugins/examples/brightness)、[`plugins/examples/invert`](plugins/examples/invert)
- **安装/卸载插件会动态增删命令，宿主零改动**

## 架构

```
                    ┌────────────────────────────────────────┐
                    │  宿主（CLI / GPUI 桌面端）               │
                    │  窗口 · 画布 · 服务容器                 │
                    └───────────────┬────────────────────────┘
                                    │
                          ToolRegistry（Cordis 服务）
                    ┌───────────────┼───────────────┐
                    ↓               ↓               ↓
             Tool 插件          核心服务        未来：WASM 插件
        （brightness, invert） ImageStore · FileCodec
                              HistoryKeeper · ToolRegistry
```

- **核心服务不是插件** —— 它们是宿主基础设施，插件通过 `Inject` 依赖它们。
- **工具才是插件** —— 每个菜单命令都来自注册表，宿主从不硬编码。
- 所有事件经由 Cordis 分发（`Context::emit` / `Context::on`）；插件 fiber 销毁时订阅自动移除。

## 快速开始

```sh
# 构建 & 测试
cargo build --workspace
cargo test --workspace

# CLI
cargo run -p kaleido-cli -- info path/to/image.png
cargo run -p kaleido-cli -- list-formats
cargo run -p kaleido-cli -- brightness --value 40 in.png out.png
cargo run -p kaleido-cli -- invert in.png out.png

# 桌面端（打开窗口；可传入图片路径）
cargo run -p kaleido-desktop [path/to/image.png]
```

> git 依赖（GPUI、gpui-component）通过 HTTPS 拉取；如果 GitHub HTTPS 不可达，可配置 git 走 SSH：
> `git config --global url."ssh://git@github.com/".insteadOf "https://github.com/"`

## 编写工具插件

工具是插件功能的最小单元。插件是一个实现 `Tool` trait 的 crate，并在其 Cordis fiber 激活时注册自己：

```rust
use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_core::{Image, ImageResult, Pixel};
use kaleido_traits::{Tool, ToolParams, ToolRegistry};
use std::sync::Arc;

pub struct InvertTool;

impl Tool for InvertTool {
    fn name(&self) -> &str { "invert" }
    fn menu_path(&self) -> String { "调整/反相".into() }
    fn description(&self) -> String { "反转所有像素颜色".into() }
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

完整示例见 [`plugins/examples/invert`](plugins/examples/invert)。

## 项目结构

```
crates/
  kaleido-core/       图像数据模型（像素缓冲、格式、几何操作）
  kaleido-traits/     契约：FileCodec、ImageStore、HistoryKeeper、Tool、事件
  kaleido-services/   实现 + Cordis 插件 + 应用容器（KaleidoApp）
  kaleido-sdk/        插件 SDK：ToolPlugin builder + define_tool! 宏
  kaleido-plugin-host/插件宿主：manifest/loader/manager + AIToolGenerator
apps/
  cli/                命令行图像工具
  desktop/            GPUI 桌面宿主
plugins/examples/
  brightness/         亮度工具插件（带参数 schema）
  invert/             反相工具插件
wit/                  WASM 接口定义（tool、lifecycle、host functions）
tests/                集成测试夹具（占位）
```

## 路线图

- [x] 核心图像库
- [x] 服务层（存储 / 编解码 / 历史 / 事件）基于 Cordis
- [x] Tool 插件契约 + 示例插件（原生、进程内）
- [x] 工具参数 schema（自动生成 UI 表单）
- [x] 文件格式编解码插件系统
- [x] 插件 SDK（`kaleido-sdk`）：`ToolPlugin` builder + `define_tool!` 宏
- [x] 插件宿主框架（`kaleido-plugin-host`）+ `AIToolGenerator`
- [x] WIT 接口定义（WASM 边界）
- [x] GPUI 桌面宿主 + 动态插件工具栏
- [ ] `kaleido-plugin-host` 接入 WASM 运行时（wasmtime）
- [ ] AI 生成工具端到端（生成 → 编译 → 加载 → `tool_upgraded`）
- [ ] 插件 UI 面板

## 许可证

MIT — 见 [LICENSE](LICENSE)。
