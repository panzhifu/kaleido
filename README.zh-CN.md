# Kaleido

> AI 原生图像工作站 —— 基于 [Cordis](https://github.com/dshbox/cordis-rs) 和 [GPUI](https://github.com/zed-industries/zed) 的插件驱动图像编辑器。

Kaleido 是一个正在建设中的图像编辑器。它的架构刻意采用**插件优先**的设计：宿主（CLI / 桌面端）只提供窗口、画布和服务容器；**每一个用户可见的功能都是插件**，通过 Cordis 管理的工具注册表动态加载。

## 功能特性

### 核心库（`kaleido-core`）
- `Tile` 使用 `Arc<Vec<u8>>` 实现零拷贝克隆与写时复制（COW）
- `TiledImage`（`HashMap<TileCoord, Tile>`）128×128 分块存储：稀疏分配、tile 级并行、脏 tile 追踪
- 5 种像素格式（RGBA8 / RGB8 / Gray8 / GrayA8 / RGBA16）
- **SIMD 格式转换**：RGBA↔Gray、RGBA↔GrayA 共 4 条路径 SIMD 加速（`wide` crate，8 像素/次）；RGBA↔RGB 为标量实现
- 裁剪、带重叠保护的区域复制、格式转换
- SIMD 友好的行对齐、RGBA16 全精度映射

### 服务层（`kaleido-traits` + `kaleido-services`）
- **ImageStore** — 当前图像的"单一数据源"（单一写路径）
- **FileCodec** — JPEG / PNG / WebP / TIFF 读写，BMP / GIF 只读
- **FileCodecRegistry** — 按格式的编解码插件系统（`FormatCodec` trait），作为 **Cordis 服务**暴露；第三方插件可通过依赖注入在运行时注册新格式（如 AVIF）
- **HistoryKeeper** — 基于有界快照命令的撤销/重做（默认 50 步）
- **TileHistoryKeeper** — **脏 tile 撤销**：只存储修改的 tile，内存 ∝ 修改区域（非全图）
- **ToolRegistry** — 插件提供的工具动态注册表
- **InteractiveTool** — 指针事件流契约，供笔刷类工具使用（`on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`）；**宿主**负责屏幕→图像坐标转换、撤销快照和脏 tile 追踪 — 插件只需在 `ToolContext` 中绘制
- **InteractiveToolRunner** — 笔触执行器，位于画布和插件之间：每笔触独立工作缓冲区，`begin_stroke` 时自动捕获撤销 tile，累积脏 tile，`end_stroke` 时提交到 `HistoryKeeper`
- **Op Graph** — 类 GEGL 的操作图：DAG 结构、拓扑排序、ROI 驱动懒求值
- **GraphExecutor** — Tile 级并行执行（rayon）、相邻 point-op 自动融合
- **CanvasService** — 画布服务：视口变换数学（zoom/pan/rotate）与可见 tile 计算；实际 GPU 渲染由宿主（桌面端）负责
- **ProgressiveRenderer** — 渐进渲染：Low → Medium → High 质量
- **AsyncImageLoader** — tokio 异步加载：渐进预览（512px → 全分辨率）、三种优先级策略
- **BackgroundSaver** — 后台保存不阻塞 UI
- **LayerStack** — 图层系统：像素层 + 调整层（非破坏性）、13 种混合模式、基础蒙版支持（含蒙版反转）
- **BlendMode SIMD** — 11 种混合模式 SIMD 优化（Normal/Multiply/Screen/Overlay/Darken/Lighten/Difference/Exclusion/ColorDodge/ColorBurn/SoftLight）
- **AIAgent** — 模板驱动规划器（MVP）：关键词 → 工具序列；接口预留 LLM 模式（`AgentMode::Template/Llm/Hybrid`）
- 类型化事件系统统一在 Cordis 之上（14 种事件名 + 类型化 payload，订阅随生命周期自动清理）

### 应用层
- **`kaleido-cli`** — 图像信息 / 格式转换 / 列出格式 / 亮度 / 反相 / 缩放 / 灰度化，以及插件命令：`list-tools`、`tool-schema`、`run`（自定义参数）、`create-tool`（AI 生成工具）
- **`kaleido-desktop`** — GPUI 宿主：画布直接从 `ImageStore` 渲染，**从插件注册表动态生成的工具栏**，活动 `InteractiveTool` 接收指针事件，完整的**键盘快捷键**（Ctrl+Z 撤销、Ctrl+Shift+Z 重做、Ctrl+O 打开、Ctrl+S 保存、Ctrl+Shift+S 另存为），**菜单栏**（文件 / 编辑 / 视图 / 模式 / 帮助），**状态栏**显示撤销/重做步数和文件操作反馈

### 插件体系
- `Tool` 契约（`kaleido-traits`）— 插件实现 `name` / `menu_path` / `description` / `apply`
- `Tool` 元数据 — `icon()` 工具栏图标、`category()` 功能分类（11 类：选择/变换/绘画/调色/修饰/填充/矢量/文字/分析/导航/其他）、`is_enabled()` 上下文可用性、`cursor()` 光标外观（18 种光标）
- `InteractiveTool` 契约 — 指针驱动工具实现 `on_mouse_down` / `on_mouse_drag` / `on_mouse_up` / `on_stroke_end`
- `InteractiveTool` 键盘 — `on_key_down()` / `on_key_up()` 接收 `KeyEvent` / `KeyCode` / `KeyModifiers`；`on_escape()` 取消描边；`is_stroke_active()` 查询描边状态
- `InteractiveTool` 生命周期 — `on_activate()` / `on_deactivate()` 工具切换时的状态管理
- `SelectionTool` 契约 — 生成 `Selection`（像素遮罩）而非修改像素；`on_begin()` / `on_update()` / `on_end()` + `SelectionMode`（替换/相加/相减/相交）
- `Panel` 契约 — 插件通过 12 种元素类型（标签/标题/数字输入/复选框/下拉/颜色选择器/按钮行/画布/进度条/分区）在宿主侧面板渲染自定义 UI；`on_change()` / `on_button()` 处理交互
- `AnalysisTool` 契约 — 只读工具（直方图、颜色拾取器、测量）；`analyze()` 返回 JSON 结果
- `SelectionState` — 共享选区状态，`contains()` / `bounds()` / `invert()`；平坦 `Vec<bool>` 遮罩实现 O(1) 像素检测
- **选区约束渲染** — `apply_to_selection()` 仅处理与选区相交的 tile（10000×10000 图像中 500×500 选区：6100 tile → 16 tile）
- **参数 schema**（`ParamType` / `ParamSchema` / `ToolSchema`）— 自动生成 UI 表单、参数校验与默认值
- **WIT 接口**（`wit/kaleido.wit`）— WASM 边界：`tool`、`plugin-lifecycle`、`host-functions` 接口 + `world kaleido-plugin`
- **插件宿主**（`kaleido-plugin-host`）— `PluginManifest`、`Plugin`/`PluginLoader` trait、`PluginManager`、`AIToolGenerator`（动态生成工具）
- **WASM 运行时** — 编译好的 `.wasm` 插件通过 **wasmtime** 加载执行：`WasmPluginManager` 扫描插件目录、实例化模块（C ABI：`plugin_init` / `tool_apply` 等）、把所有工具注册进注册表；宿主函数（`host_log`、`host_emit_event`）已链接
- **WASM 选区优化** — 设置选区时仅交换选区包围盒区域（非全图），局部操作数据传输量降低 99%+
- **插件 SDK**（`kaleido-sdk`）— `ToolPlugin<T>` builder + `define_tool!` 宏
- **AI 工具生成** — `KaleidoApp::create_ai_tool(描述, 执行函数)` 从 JSON 描述注册工具并发出 `tool_upgraded` 事件
- Cordis 服务插件：依赖注入（`Inject`）+ fiber 生命周期管理
- 示例插件：[`plugins/examples/brightness`](plugins/examples/brightness)、[`plugins/examples/invert`](plugins/examples/invert)、[`plugins/examples/brush`](plugins/examples/brush)（交互式圆形笔刷，支持压感）
- **安装/卸载插件会动态增删命令，宿主零改动**

## 架构

```
┌──────────────────────────────────────────────────────────────────┐
│                        宿主（CLI / GPUI 桌面端）                  │
│                                                                   │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ AI Agent    │  │ Canvas       │  │ Tool Registry           │ │
│  │ Service     │  │ Service      │  │ (Cordis 服务)            │ │
│  │ (模板规划)   │  │ (视口变换)   │  │ ← WASM / 原生 / AI 工具 │ │
│  └──────┬──────┘  └──────┬───────┘  └─────────────────────────┘ │
│         │                │                                        │
│         ▼                ▼                                        │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Op Graph Executor                         │ │
│  │   [原图] → [brightness] → [blur] → [sharpen] → [输出]       │ │
│  │   ROI 驱动 │ 自动合并 point-op │ 并行 tile 处理             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                        │
│                          ▼                                        │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Tile Store (TiledImage)                   │ │
│  │   HashMap<TileCoord, Arc<Tile>>                             │ │
│  │   128×128 tiles │ LRU 脏 tile 缓存 │ 撤销只存脏 tile 旧版本  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                        │
│                          ▼                                        │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ FileCodec   │  │ pixel_convert│  │ HistoryKeeper           │ │
│  │ Registry    │  │ (SIMD)       │  │ (脏 tile 快照)           │ │
│  └─────────────┘  └──────────────┘  └─────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

- **核心服务不是插件** —— 它们是宿主基础设施，插件通过 `Inject` 依赖它们。
- **工具才是插件** —— 每个菜单命令都来自注册表，宿主从不硬编码。
- **Op Graph 统一处理引擎** —— 自动融合相邻 point-op，ROI 驱动只计算可见区域。
- **Tile 级并行** —— rayon 多核处理不同 tile，SIMD 加速 tile 内像素操作。
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
use kaleido_core::{ImageResult, Pixel, TiledImage};
use kaleido_traits::{Tool, ToolParams, ToolRegistry};
use std::sync::Arc;

pub struct InvertTool;

impl Tool for InvertTool {
    fn name(&self) -> &str { "invert" }
    fn menu_path(&self) -> String { "调整/反相".into() }
    fn description(&self) -> String { "反转所有像素颜色".into() }
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

完整示例见 [`plugins/examples/invert`](plugins/examples/invert)。

## 项目结构

```
crates/
  kaleido-core/       图像数据模型（TiledImage、Tile、Pixel、SIMD 格式转换）
  kaleido-traits/     契约：FileCodec、ImageStore、HistoryKeeper、Tool、InteractiveTool、事件
  kaleido-services/   实现 + Cordis 插件 + 应用容器（KaleidoApp）
                      （InteractiveToolRunner、Op Graph、Layer、Tile History、Blend SIMD）
  kaleido-sdk/        插件 SDK：ToolPlugin builder + define_tool! 宏
  kaleido-plugin-host/插件宿主：manifest/loader/manager + wasmtime 运行时 + AIToolGenerator
apps/
  cli/                命令行图像工具
  desktop/            GPUI 桌面宿主（画布、工具栏、菜单栏、状态栏）
plugins/examples/
  brightness/         亮度工具插件（带参数 schema）
  invert/             反相工具插件
  brush/              交互式圆形笔刷插件（支持压感、描边插值）
wit/                  WASM 接口定义（tool、lifecycle、host functions）
docs/                架构文档（architecture.md）
tests/                集成测试夹具（占位）
```

## 路线图

### 已完成
- [x] 核心图像库（Tile + TiledImage + SIMD 格式转换）
- [x] 服务层（存储 / 编解码 / 历史 / 事件）基于 Cordis
- [x] **Op Graph 执行引擎**（DAG、ROI 驱动、tile 并行、point-op 融合）
- [x] **Canvas 服务**（视口变换数学、渐进渲染；GPU 渲染由桌面端负责）
- [x] **异步 I/O**（AsyncImageLoader + BackgroundSaver）
- [x] **脏 tile 撤销**（TileHistoryKeeper，内存 ∝ 修改区域）
- [x] **图层系统**（LayerStack + 13 种混合模式）
- [x] **SIMD 混合模式**（11 种模式，8 像素/次）
- [x] Tool 插件契约 + 示例插件（原生、进程内）
- [x] **InteractiveTool 契约**（指针事件流、`ToolContext`、脏 tile 追踪）
- [x] **InteractiveToolRunner**（笔触执行器，含撤销、工作缓冲区、脏 tile 追踪）
- [x] **笔刷参考插件**（圆形笔刷、压感、描边插值）
- [x] 工具参数 schema（自动生成 UI 表单）
- [x] 文件格式编解码插件系统
- [x] 插件 SDK（`kaleido-sdk`）：`ToolPlugin` builder + `define_tool!` 宏
- [x] 插件宿主框架（`kaleido-plugin-host`）+ `AIToolGenerator`
- [x] WIT 接口定义（WASM 边界）
- [x] WASM 运行时（wasmtime）— 加载编译好的 `.wasm` 工具插件
- [x] GPUI 桌面宿主 + 动态插件工具栏 + 菜单栏 + 键盘快捷键 + 文件 I/O

### 待做
- [ ] 示例 WASM 工具插件（把工具编译成 `.wasm` 并加载）
- [ ] AI 生成工具端到端（生成 → 编译 → 加载 → `tool_upgraded`）
- [ ] 高级混合模式（Hard Light 等 SIMD 优化）
- [ ] 蒙版系统增强（羽化、矢量蒙版等）
- [ ] 选区叠加渲染（marching ants 动画）
- [ ] 笔刷引擎预设（纹理、动态、混合）

## 许可证

MIT — 见 [LICENSE](LICENSE)。
