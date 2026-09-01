# Kaleido

> AI 原生图像工作站 —— 基于 [Cordis](https://github.com/dshbox/cordis-rs) 和 [GPUI](https://github.com/zed-industries/zed) 的插件驱动图像编辑器。

Kaleido 是一个正在建设中的图像编辑器。它的架构刻意采用**插件优先**的设计：宿主（CLI / 桌面端）只提供窗口、画布和服务容器；**每一个用户可见的功能都是插件**，通过 Cordis 管理的工具注册表动态加载。

## 五种模式，统一数据模型

Kaleido 支持**五种编辑模式**——矢量、像素、绘画、排版、动画——全部运行在统一的场景图文档模型上。切换模式只是改变编辑视角，底层数据结构不变。

## 功能特性

### 核心库（`kaleido-core`）—— 纯数据结构

本 crate **只包含数据结构**——无服务逻辑、无插件框架、无依赖注入。

- **基础类型**（`types.rs`）— `Point` / `Size` / `Color`（f32 RGBA）/ `Transform2D` / `BlendMode`（16 种变体）/ 稳定 ID
- **瓦片光栅**（`tile_core.rs` + `tile.rs`）— `Tile`（256×256 固定缓冲，**Arc 写时复制 + dirty 脏标记**）、`TiledImage`（稀疏瓦片图）
- **场景图**（`scene.rs`）— `Scene` 对象树，支持增删/重挂/环引用拒绝/重排序/子树移除/树完整性校验
- **节点内容** — `PixelLayer` + `FramePixels`、`VectorObject`（贝塞尔路径）、`TextObject`（富文本）
- **蒙版与选区**（`mask.rs`）— `Mask` + `SelectionMask`（灰度蒙版，同一套结构互转）
- **动画**（`timeline.rs`）— 双轨：逐帧手绘 + 属性关键帧
- **效果链**（`effects.rs`）— `EffectBinding` / `EffectScope`；调整层改为插件效果链
- **色彩管理**（`color_profile.rs`）— `ColorSpace`（sRGB / linear / CMYK / Lab）
- **文档格式**（`format.rs`）— `.kld` 原生格式（魔数 + 版本 + 分块）
- **文档**（`document.rs`）— 顶层聚合：`size` / `dpi` / `color_profile` / `scene` / `selection` / `history` / `timeline` / `resources` / `metadata`

### 服务层（`kaleido-traits` + `kaleido-services`）—— 12 管理器

| # | 管理器 | service id | 职责 |
|---|--------|-----------|------|
| 1 | **数据 Data** | `data_service` | 文档生命周期、文件 I/O、撤销快照恢复 |
| 2 | **历史 History** | `history_service` | 基于 COW 快照恢复的撤销/重做 |
| 3 | **图层 Layer** | `layer_service` | 场景节点的增删/重排序/重命名/不透明度/混合模式 |
| 4 | **选区 Selection** | `selection_service` | 选区蒙版操作 |
| 5 | **颜色 Color** | `color_service` | 色彩配置、色卡管理 |
| 6 | **渲染 Render** | `render_service` | 场景合成、导出扁平化 |
| 7 | **插件 Plugin** | `plugin_service` | 插件清单/加载/生命周期、WASM 运行时 |
| 8 | **软件 App** | `app_service` | 应用名称/版本、编辑模式、通知、**软件配置管理** |
| 9 | **资源 Resource** | `resource_service` | 字体/色卡/笔刷资源管理 |
| 10 | **快捷键 Shortcut** | `shortcut_service` | 全局/模式/插件快捷键注册与解析 |
| 11 | **UI** | `ui_service` | 状态栏、通知、面板注册 |
| 12 | **任务 Task** | `task_service` | 后台任务 spawn/进度/取消/等待 |

**核心设计 —— 单一写路径**：所有文档变更通过 `DataService` 进入：
1. COW clone 当前 Document 为 before 快照
2. 执行变更（若失败：不动、不记录）
3. before 快照压入 undo 栈；清空 redo 栈

### 插件契约（`kaleido-traits/plugins/`）

- `Tool` trait + 参数 schema（`ParamType` / `ParamSchema` / `ToolSchema`）
- **`Tool` 元数据** — `icon()`、`category()`（11 类）、`is_enabled()`、`cursor()`（18 种）
- **`InteractiveTool` trait** — 指针事件流
- **`SelectionTool` trait` — 生成 Selection 像素遮罩
- **`Panel` trait` — 插件侧面板 UI
- **`AnalysisTool` trait` — 只读工具
- **WIT 接口**（`wit/kaleido.wit`）— WASM 边界
- **插件 SDK**（`kaleido-sdk`）— `ToolPlugin<T>` builder + `define_tool!` 宏

### 应用层
- **`kaleido-cli`** — 演示 12 管理器架构，提供 `status` 和 `demo` 命令
- **`kaleido-desktop`** — GPUI 宿主：画布渲染、Dock 系统、快捷键、文件打开/保存、撤销/重做

### 插件体系
- 示例插件：[`plugins/examples/tga`](plugins/examples/tga)（TGA 格式编解码）、[`plugins/wasm/simple_format`](plugins/wasm/simple_format)（WASM 插件）

## 架构

```
┌──────────────────────────────────────────────────────────────────┐
│                        宿主（CLI / GPUI 桌面端）                   │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    12 个服务管理器                            │ │
│  │  Data · History · Layer · Selection · Color · Render        │ │
│  │  Plugin · App · Resource · Shortcut · UI · Task             │ │
│  │                    (Cordis 服务)                              │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                       │
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Document（统一数据模型）                    │ │
│  │  Scene Graph → Node [PixelLayer | VectorObject | Text | Group]│
│  │  256×256 稀疏瓦片 · Arc COW · 脏瓦片追踪                     │ │
│  │  Timeline（双轨）· Mask/Selection · Effects                  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │              插件注册表（Cordis 服务）                         │ │
│  │  ← WASM / 原生 / AI 生成工具                                  │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

- **核心服务不是插件** —— 它们是宿主基础设施，插件通过 `Inject` 依赖它们。
- **工具才是插件** —— 每个菜单命令都来自注册表，宿主从不硬编码。
- **单一写路径** —— 所有文档变更通过 `DataService`；撤销 = COW 快照恢复。
- **五种模式，统一模型** —— 矢量 / 像素 / 绘画 / 排版 / 动画全部运行在同一场景图上。
- 所有事件经由 Cordis 分发（`Context::emit` / `Context::on`）。

## 快速开始

```sh
# 构建 & 测试
cargo build --workspace
cargo test --workspace

# CLI
cargo run -p kaleido-cli -- status     # 查看服务层状态
cargo run -p kaleido-cli -- demo       # 运行端到端工作流演示

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
use kaleido_traits::plugins::{Tool, ToolParams};
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

完整示例见 [`plugins/examples/tga`](plugins/examples/tga)。

## 项目结构

```
crates/
  kaleido-core/        文档数据模型（纯数据结构）
                         types · tile_core · tile · pixel · pixel_layer
                         scene · vector · text · mask · timeline · effects
                         color_profile · document · format · conversion
  kaleido-traits/      服务契约 + 插件契约
                         services/      (data, history, layer, selection, color,
                                         render, plugin, app, resource, shortcut,
                                         ui, task)
                         plugins/       (tool, panel, events, category, cursor)
  kaleido-services/    12 个服务管理器的实现
                         app/ · color/ · data/ · history/ · layer/
                         plugin/ · render/ · resource/ · selection/
                         shortcut/ · task/ · ui/
  kaleido-sdk/         插件 SDK：ToolPlugin builder + define_tool! 宏
apps/
  cli/                 CLI 演示 12 管理器架构
  desktop/             GPUI 桌面宿主（画布、Dock、状态栏）
plugins/
  examples/tga/        TGA 格式编解码插件
  wasm/simple_format/  WASM 插件示例（编译好的 .wasm + .wit）
wit/                  WASM 接口定义（tool、lifecycle、host functions）
docs/                 架构文档
tests/                集成测试夹具（占位）
```

## 路线图

### 已完成
- [x] 核心图像库（Tile + TiledImage + SIMD 格式转换）
- [x] **统一 Document 数据模型**（Scene Graph、PixelLayer、VectorObject、TextObject、Mask/Selection、Timeline、Effects、ColorProfile）
- [x] **12 个服务管理器**（Data、History、Layer、Selection、Color、Render、Plugin、App、Resource、Shortcut、UI、Task）
- [x] **五种编辑模式**（矢量 / 像素 / 绘画 / 排版 / 动画）统一模型
- [x] **桌面端** —— 画布渲染、Dock 系统、文件 I/O、撤销/重做
- [x] **AppSettings 管理**（默认画布尺寸、撤销步数、自动保存、插件目录、默认模式）
- [x] **插件契约**归入 `plugins/` 子模块
- [x] **TGA 编解码插件**示例
- [x] **WASM simple_format 插件**示例
- [x] **文档格式**（`.kld` 分块原生格式）
- [x] **190 个测试**覆盖全工作区

### 待做
- [ ] 高级混合模式（Hard Light 等 SIMD 优化）
- [ ] 蒙版系统增强（羽化、矢量蒙版）
- [ ] 选区叠加渲染（marching ants 动画）
- [ ] 笔刷引擎预设（纹理、动态、混合）
- [ ] 文本引擎细节：竖排 / RTL / 行距字距
- [ ] 逐帧动画内存策略（帧上限、未修改帧共享细节）
- [ ] `.kld` 序列化格式定稿
- [ ] AI 生成工具端到端

## 许可证

MIT — 见 [LICENSE](LICENSE)。
