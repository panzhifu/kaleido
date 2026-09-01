# Kaleido

> AI 原生图像工作站 —— 基于 [Cordis](https://github.com/dshbox/cordis-rs) 和 [GPUI](https://github.com/zed-industries/zed) 的插件驱动图像编辑器。

Kaleido 是一个正在建设中的图像编辑器。它的架构刻意采用**插件优先**的设计：宿主（CLI / 桌面端）只提供窗口、画布和服务容器；**每一个用户可见的功能都是插件**，通过 Cordis 管理的工具注册表动态加载。

## 五种模式，统一数据模型

Kaleido 支持**五种编辑模式**——矢量、像素、绘画、排版、动画——全部运行在统一的场景图文档模型上。切换模式只是改变编辑视角，底层数据结构不变。

## 功能特性

### 核心库（`kaleido-core`）—— 纯数据结构

本 crate **只包含数据结构**——无服务逻辑、无插件框架、无依赖注入。服务契约在 `kaleido-traits`，实现在 `kaleido-services`。

- **基础类型**（`types.rs`）— `Point` / `Size` / `Color`（f32 RGBA）/ `Transform2D`（平移+旋转+缩放，动画友好）/ `BlendMode`（16 种变体）/ 稳定 ID（`NodeId` / `DocumentId` / `ResourceId` / `EffectId`）
- **瓦片光栅**（`tile_core.rs` + `tile.rs`）— `Tile`（256×256 固定缓冲，**Arc 写时复制 + dirty 脏标记**）、`TiledImage`（稀疏瓦片图：只分配实际绘制区域）、像素读写、批量填充、裁剪、区域拷贝、格式转换
- **场景图**（`scene.rs`）— `Scene` 对象树，支持增删/重挂/环引用拒绝/重排序/子树移除/树完整性校验；`Node` 含 `transform` / `opacity` / `visible` / `locked` / `blend` / `mask` / `effects`
- **节点内容** — `PixelLayer` + `FramePixels`（逐帧瓦片快照：静态 1 帧，动画多帧，未改帧 Arc 共享）、`VectorObject`（节点式贝塞尔路径：锚点 + 入/出控制点，FillStyle / StrokeStyle）、`TextObject`（富文本 `TextRun`：字体/字号/粗斜体，对齐，定宽文本框）
- **蒙版与选区**（`mask.rs`）— `Mask`（图层蒙版 / 矢量蒙版）+ `SelectionMask`（灰度蒙版，None = 全选）——**同一套灰度结构互转**（PS 模型）
- **动画**（`timeline.rs`）— 双轨：**逐帧手绘**（Krita 式，走 `PixelLayer.frames`）+ **属性关键帧**（AE 式，走 `Timeline.tracks` + `Keyframe` / `Easing` / `AnimValue`）
- **效果链**（`effects.rs`）— `EffectBinding`（插件提供的效果 ID + JSON 参数 + 作用域）/ `EffectScope`（SelfOnly 滤镜 / Subtree 调整层语义）；调整层**不做内置节点**，改为插件效果链
- **色彩管理**（`color_profile.rs`）— `ColorSpace`（sRGB / linear / CMYK / Lab）+ 位深 + ICC 引用
- **文档格式**（`format.rs`）— `.kld` 原生格式：魔数 + 版本 + 分块（文档 + 缩略图）
- **文档**（`document.rs`）— 顶层聚合：`size` / `dpi` / `color_profile` / `scene` / `selection` / `history` / `timeline` / `resources` / `metadata`

### 服务层（`kaleido-traits` + `kaleido-services`）—— 12 管理器

| # | 管理器 | service id | 职责 |
|---|--------|-----------|------|
| 1 | **数据 Data** | `data_service` | 文档生命周期、单一写路径（`apply_mutation`）、撤销快照恢复、导出 |
| 2 | **历史 History** | `history_service` | 基于 COW 快照恢复的撤销/重做 |
| 3 | **图层 Layer** | `layer_service` | 场景节点的增删/重排序/重命名/不透明度/混合模式 |
| 4 | **选区 Selection** | `selection_service` | 选区蒙版：设置/清除/反转/相加/相交/相减 |
| 5 | **颜色 Color** | `color_service` | 色彩配置、色卡管理 |
| 6 | **渲染 Render** | `render_service` | 场景合成（自底向上，混合+不透明度）、导出扁平化 |
| 7 | **插件 Plugin** | `plugin_service` | 插件清单/加载/生命周期、WASM 运行时（wasmtime）、工具注册表 |
| 8 | **软件 App** | `app_service` | 应用名称/版本、编辑模式、通知 |
| 9 | **资源 Resource** | `resource_service` | 字体/色卡/笔刷资源管理 |
| 10 | **快捷键 Shortcut** | `shortcut_service` | 全局/模式/插件快捷键注册与解析 |
| 11 | **UI** | `ui_service` | 状态栏、通知、面板注册 |
| 12 | **任务 Task** | `task_service` | 后台任务 spawn/进度/取消/等待 |

**核心设计 —— 单一写路径**：所有文档级变更**只允许**通过 `DataService::apply_mutation(label, f)` 进入：

1. COW clone 当前 Document 为 before 快照（Arc 共享瓦片，零成本）
2. 执行 `f(&mut Document)`（若失败：不动、不记录）
3. before 快照压入 undo 栈；清空 redo 栈
4. 发出 `document_changed` 事件

**旧模型服务**（基于旧 `TiledImage` 模型）保留以兼容：`ImageStore`、`HistoryKeeper`、`LayerStore`、`FileCodecRegistry`、`ToolRegistry`、`PanelRegistry`、`AIAgent`、`InteractiveToolRunner`、`OpGraph`、`BlendMode SIMD`、`AsyncImageLoader`、`BackgroundSaver`。待桌面端迁移到新模型后可移除。

### 应用层
- **`kaleido-cli`** — 图像信息 / 格式转换 / 列出格式 / 亮度 / 反相 / 缩放 / 灰度化，以及插件命令：`list-tools`、`tool-schema`、`run`（自定义参数）、`create-tool`（AI 生成工具）
- **`kaleido-desktop`** — GPUI 宿主：画布直接从 `ImageStore` 渲染，**从插件注册表动态生成的工具栏**，活动 `InteractiveTool` 接收指针事件，完整的**键盘快捷键**（Ctrl+Z 撤销、Ctrl+Shift+Z 重做、Ctrl+O 打开、Ctrl+S 保存、Ctrl+Shift+S 另存为），**菜单栏**（文件 / 编辑 / 视图 / 模式 / 帮助），**状态栏**显示撤销/重做步数和文件操作反馈，** Dock 系统**（替代旧固定面板布局）实现灵活的面板排布

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
- **WASM 运行时** — 编译好的 `.wasm` 插件通过 **wasmtime** 加载执行：`WasmPluginManager` 扫描插件目录、实例化模块（C ABI：`plugin_init` / `tool_apply` 等）、把所有工具注册进注册表；宿主函数（`host_log`、`host_emit_event`）已链接
- **WASM 选区优化** — 设置选区时仅交换选区包围盒区域（非全图），局部操作数据传输量降低 99%+
- **插件 SDK**（`kaleido-sdk`）— `ToolPlugin<T>` builder + `define_tool!` 宏
- **AI 工具生成** — `KaleidoApp::create_ai_tool(描述, 执行函数)` 从 JSON 描述注册工具并发出 `tool_upgraded` 事件
- Cordis 服务插件：依赖注入（`Inject`）+ fiber 生命周期管理
- 示例插件：[`plugins/examples/tga`](plugins/examples/tga)（TGA 格式编解码插件）、[`plugins/wasm/simple_format`](plugins/wasm/simple_format)（WASM 插件示例）
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
│  │                    12 个服务管理器                            │ │
│  │  Data · History · Layer · Selection · Color · Render        │ │
│  │  Plugin · App · Resource · Shortcut · UI · Task             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                        │
│                          ▼                                        │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Document（统一数据模型）                    │ │
│  │  Scene Graph → Node [PixelLayer | VectorObject | Text | Group]│
│  │  256×256 稀疏瓦片 · Arc COW · 脏瓦片追踪                     │ │
│  │  Timeline（双轨）· Mask/Selection · Effects                  │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

- **核心服务不是插件** —— 它们是宿主基础设施，插件通过 `Inject` 依赖它们。
- **工具才是插件** —— 每个菜单命令都来自注册表，宿主从不硬编码。
- **单一写路径** —— 所有文档变更通过 `DataService::apply_mutation` 进入；撤销 = COW 快照恢复。
- **五种模式，统一模型** —— 矢量 / 像素 / 绘画 / 排版 / 动画全部运行在同一场景图上。
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

完整示例见 [`plugins/examples/tga`](plugins/examples/tga)。

## 项目结构

```
crates/
  kaleido-core/        文档数据模型（纯数据结构）
                         types · tile_core · tile · pixel · pixel_layer
                         scene · vector · text · mask · timeline · effects
                         color_profile · document · format · conversion
  kaleido-traits/      契约：12 个服务 trait + 旧版工具契约
                         services/ (data, history, layer, selection, color,
                                    render, plugin, app, resource, shortcut,
                                    ui, task)
                         (旧版: tool, interactive_tool, panel, selection_tool,
                                  analysis_tool, image_store, history_keeper,
                                  file_codec, ai_agent, events, keyboard)
  kaleido-services/    实现：12 个服务管理器 + 旧版服务
                         managers/ (data, history, layer, selection, color,
                                    render, app, resource, shortcut, ui, task)
                         plugin_service/ (manifest, loader, manager, wasmtime)
                         (旧版: image_store, file_codec, history_keeper,
                                  layer_store, tool_registry, panel_registry,
                                  ai_agent, interactive_tool, op_graph,
                                  blend_simd, async_io)
  kaleido-sdk/         插件 SDK：ToolPlugin builder + define_tool! 宏
apps/
  cli/                 命令行图像工具
  desktop/             GPUI 桌面宿主（画布、工具栏、状态栏、Dock）
plugins/
  examples/tga/        TGA 格式编解码插件
  wasm/simple_format/  WASM 插件示例（编译好的 .wasm + .wit）
wit/                  WASM 接口定义（tool、lifecycle、host functions）
docs/                 架构文档（重构总览、数据模型、服务布局、服务重构）
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
- [x] 插件宿主框架（并入 `kaleido-services` 的 `plugin_service`）+ `AIToolGenerator`
- [x] WIT 接口定义（WASM 边界）
- [x] WASM 运行时（wasmtime）— 加载编译好的 `.wasm` 工具插件
- [x] GPUI 桌面宿主 + 动态插件工具栏 + 菜单栏 + 键盘快捷键 + 文件 I/O
- [x] **统一 Document 数据模型**（Scene Graph、PixelLayer、VectorObject、TextObject、Mask/Selection、Timeline、Effects、ColorProfile）
- [x] **五种编辑模式**（矢量 / 像素 / 绘画 / 排版 / 动画）统一模型
- [x] **12 个服务管理器**（Data、History、Layer、Selection、Color、Render、Plugin、App、Resource、Shortcut、UI、Task）+ 单一写路径架构
- [x] **Dock 系统**替代桌面端旧固定面板布局
- [x] **TGA 编解码插件**示例
- [x] **WASM simple_format 插件**示例（编译为 `.wasm` 并通过 wasmtime 加载）
- [x] **文档格式**（`.kld` 分块原生格式）

### 待做
- [ ] 桌面端迁移到新 Document 模型
- [ ] AI 生成工具端到端（生成 → 编译 → 加载 → `tool_upgraded`）
- [ ] 高级混合模式（Hard Light 等 SIMD 优化）
- [ ] 蒙版系统增强（羽化、矢量蒙版等）
- [ ] 选区叠加渲染（marching ants 动画）
- [ ] 笔刷引擎预设（纹理、动态、混合）
- [ ] 文本引擎细节：竖排 / RTL / 行距字距
- [ ] 逐帧动画内存策略（帧上限、未修改帧共享细节）
- [ ] `.kld` 序列化格式定稿

## 许可证

MIT — 见 [LICENSE](LICENSE)。
