# Kaleido 项目长期笔记

## 产品定位（用户 2026-08-30 确认的核心构想）
- **宿主只负责核心基础设施**：图片数据管理（TiledImage / ImageStore）、撤销 / 重做（TileSnapshotCommand 脏瓦片 undo）、图层管理（LayerStack）、蒙版管理。这些属于第一方内置。
- **所有工具全部插件化**：画笔、喷涂等一律由插件提供，面向**第三方开发者**开发插件加载使用。
- 这是一个「平台 + 插件生态」模式（类 Photoshop 插件 / GIMP GEGL / VS Code 扩展）。

## 架构决策
- 第三方插件唯一入口应为 **WASM（wasmtime 沙箱）**；native in-process 插件（Rust `Tool` trait）只留给第一方/调试，因为 Rust 无法隔离不可信 native 代码。
- 插件 ABI（WIT `wit/kaleido.wit`）必须先冻结并与 wasmtime C ABI 对齐，否则第三方无法可靠开发。

## 已知关键缺口（2026-08-30 审查结论）
- 桌面端（apps/desktop）与服务层完全断开：`panels/` 等未声明 mod 不参与编译，toolbar 用硬编码枚举且无 on_click，ImageStore/HistoryKeeper/ToolRegistry 零调用。
- 当前 `Tool` 契约只有一次性 `apply(image, params)`，**无法表达交互式工具**（画笔需要鼠标事件流 on_down/on_drag/on_up + 增量刷新）——这是插件体系最大的架构洞。
- 图层类型在 `layer.rs` 与 `layer_types.rs` 重复定义（各自独立计数器，re-export 后类型不兼容）；混合模式三份实现（blend.rs / layer.rs / blend_simd.rs）。
- 渲染为每帧全图 clone + to_rgba_vec，无脏矩形/纹理缓存，画笔类插件性能命根子未解决。

## 重构分层与服务层划分（2026-08-31 确认）
- 顶层补 **Document 聚合概念**（瓦片 TiledImage / 存储 Storage / 颜色 Color / 图层 Layers / 选区 Selection / 历史 History）。
- 目录四大区域：**services（服务）/ traits（契约层，即口述 tra·TIAIT）/ plugins（插件）/ sdk（开发者组件）**。
- 服务层 **12 大管理器**：
  - 文档级 6：数据 / 历史 / 图层 / 选区 / 颜色 / 渲染
  - 应用级 6：插件 / 软件 / 资源 / 快捷键 / UI / 任务
- 关键决策：渲染的图层合成用 **OpGraph**（DAG + 节点缓存 + 脏传播，热路径走 blend_simd）；快捷键管理支持插件注册快捷键；选区 = 灰度蒙版放 Document（与图层蒙版同构互转）；调色板归颜色管理、色卡归资源管理。
- 重构顺序：先设计 Document 数据结构 → 服务层接口 → 目录归位。

## 数据模型设计（2026-08-31 定稿草案，见 docs/architecture-data-model.md）
- Document = Scene 对象树（Group / Pixel / Vector / Text 节点，slotmap 稳定 ID，transform/opacity/blend/蒙版）+ Selection（灰度蒙版，与图层蒙版同构）+ History + Timeline + ColorProfile + Resources。
- 像素层：256 稀疏瓦片 + Arc COW + frames 逐帧支持（手绘动画）；矢量：节点式贝塞尔路径；动画双轨（逐帧 + 属性关键帧）；调整层改为插件效果链（Node.effects / EffectBinding，宿主只定义 Effect 契约）。
- **core 已落地实现**（2026-08-31）：模块 types/scene/pixel_layer/vector/text/mask/timeline/effects/color_profile/document；TILE_SIZE=256，Tile 带 dirty 标记；`cargo check -p kaleido-core` 待用户验证。

## 路线图（优先级）
1. 修服务层 bug（合并图层类型、blend 统一、define_tool! 签名、blend_simd 优先级、tile_history 空 Vec panic、ai_agent Inject 缺 tool_registry）
2. 桌面端接入服务层（panels/ 编入编译、toolbar 用 ToolRegistry、接通 undo）
3. 冻结 WIT / ABI 对齐 + 第一个 WASM 示例插件（README TODO 第一项）
4. 扩展 Tool 契约为交互式工具（输入事件流）
5. 增量渲染（脏矩形）
6. AI 工具端到端（generate → compile → load）
7. 插件市场 / 安装 UI / 安全（签名校验）
