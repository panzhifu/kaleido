# Kaleido 重构总览（2026-08-31）

> 本文档记录 kaleido 重构的目标、核心决策、当前实现状态与后续计划。
> 配套详细设计见 `docs/architecture-data-model.md`。

---

## 1. 重构目标

1. **宿主只做基础设施**：图片数据（TiledImage / PixelLayer）、撤销重做、图层、蒙版、选区、渲染管线属于第一方；**所有工具与效果全部插件化**（面向第三方开发者），类 Photoshop 插件 / VS Code 扩展。
2. **补上顶层 `Document` 概念**：一个统一的数据模型承载五种编辑模式——**矢量 / 像素 / 绘画 / 排版 / 动画**，模式只是编辑视角，不换数据结构。
3. **性能优先**：稀疏瓦片、Arc 写时复制、稳定节点 ID、脏瓦片追踪、缓存位、SIMD 友好布局。

## 2. 核心决策（已与用户确认）

| 决策 | 内容 |
|------|------|
| 目录四大区域 | `services`（服务实现）/ `traits`（接口契约）/ `plugins`（第三方插件）/ `sdk`（开发者组件） |
| 服务层 12 管理器 | 文档级 6：数据 / 历史 / 图层 / 选区 / 颜色 / 渲染；应用级 6：插件 / 软件 / 资源 / 快捷键 / UI / 任务 |
| 对象树 | Scene Graph（Figma / Krita 式），Group / Pixel / Vector / Text 四类节点，slotmap 稳定 ID |
| 选区 | 灰度蒙版，放 Document，与图层蒙版**同构互转**（PS 模型） |
| 动画 | **双轨**：逐帧手绘（Krita 式，走 PixelLayer.frames）+ 属性关键帧（AE 式，走 Timeline.tracks） |
| 调整层 | **不做内置节点**，改为插件效果链（Node.effects / EffectBinding），宿主只定义 Effect 契约 |
| 矢量路径 | 节点式（锚点 + 入/出控制点，可编辑） |
| 瓦片大小 | 固定 256×256 |

## 3. kaleido-core 实现现状（纯数据结构）

> core 已按"只含数据结构"原则清理：无服务逻辑、无插件框架、无 DI。服务契约在 traits，实现在 services。

### 3.1 模块清单

| 模块 | 实现的功能 |
|------|-----------|
| `types.rs` | Point / Size / ImageSize / Color（f32 RGBA）/ Transform2D（平移+旋转+缩放，动画友好）/ BlendMode（16 种）/ 稳定 ID（NodeId / DocumentId / ResourceId / EffectId） |
| `tile_core.rs` | Tile（256×256 固定缓冲，**Arc 写时复制 + dirty 脏标记**）、TileCoord |
| `tile.rs` | TiledImage：稀疏瓦片图（只分配实际绘制区域）、像素读写（checked/unchecked）、批量填充、裁剪、区域拷贝、格式转换、raw/RGBA 导出 |
| `pixel_layer.rs` | PixelLayer + FramePixels：**逐帧瓦片快照**（静态 1 帧，动画多帧，未改帧 Arc 共享） |
| `scene.rs` | Scene 对象树：节点增删/重挂/子树回收、Node（transform / opacity / visible / locked / blend / mask / effects） |
| `vector.rs` | VectorObject：节点式贝塞尔路径、FillStyle（纯色/无）、StrokeStyle |
| `text.rs` | TextObject：富文本分段 TextRun（字体/字号/颜色/粗斜体）、对齐、定宽文本框 |
| `mask.rs` | Mask（图层蒙版/矢量蒙版）+ SelectionMask（灰度蒙版，None=全选）——**同一套灰度结构互转** |
| `timeline.rs` | Timeline 双轨：frame_rate / duration + Track（节点属性关键帧）/ Keyframe / Easing / AnimValue |
| `effects.rs` | EffectBinding（effect ID + JSON 参数 + 作用域）/ EffectScope（SelfOnly 滤镜 / Subtree 调整层） |
| `color_profile.rs` | ColorSpace（sRGB / linear / CMYK / Lab）+ 位深 + ICC 引用 |
| `document.rs` | Document 顶层聚合：size / dpi / color_profile / scene / selection / history / timeline / resources / metadata |

### 3.2 性能手段（数据层面已落地）

1. **稀疏瓦片**：`TileMap`（TiledImage）只分配绘制过的区域，空白画布零像素内存
2. **Arc + 写时复制**：Tile 数据 Arc 共享，undo / 多视图 / 逐帧动画零拷贝共享，修改才 clone
3. **脏瓦片追踪**：Tile 带 `dirty` 标记（AtomicBool），写时置脏，增量渲染消费后清除
4. **稳定节点 ID**：递增 u64 句柄，删除节点不使其他引用失效
5. **SIMD 友好**：像素线性内存 + 位深对齐，`blend_simd` 可直接消费

### 3.3 测试

- `tile_tests.rs`：瓦片分配、稀疏性、非整倍尺寸、裁剪、拷贝、填充、往返转换、**脏瓦片追踪、灰度反转、COW 共享、跨瓦片 crop/copy**
- `model_tests.rs`（新增）：场景增删节点 / 非组节点拒绝挂子 / **reparent 环引用拒绝、子树移除、重排序、树完整性 validate** / 逐帧 COW 隔离 / **多帧动画共享、时间线关键帧采样与缓动、选区反转、文本 run 校验、矢量包围盒、Transform 变换、Document JSON roundtrip**

> `cargo check -p kaleido-core` + `cargo test -p kaleido-core` 均已验证通过（62 tests，clippy 0 warnings）。

## 4. 本次清理

- **移除 Cordis**：core 不再包含服务总线。`bus.rs` 已从编译中移除（lib.rs 无 `pub mod bus;`，Cargo.toml 去掉 `cordis-rs` 依赖）；文件留占位说明，可手动 `rm crates/kaleido-core/src/bus.rs`。
- **删除冗余方法**：`TiledImage::sub_view`（= crop 别名）、`to_packed_bytes`（= to_raw_vec 别名）、`offset()`（恒返 0 的 stub）。

## 5. 五种模式 ↔ 数据结构

| 模式 | 操作对象 |
|------|----------|
| 矢量 | VectorObject 的 PathNode / Fill / Stroke |
| 像素 | PixelLayer 的 Tile 读写 |
| 绘画 | PixelLayer + 画笔交互（工具插件化） |
| 排版 | TextObject 的 runs / frame |
| 动画 | Timeline（逐帧 frames + 属性 tracks） |

## 6. 待办与下一步

- [x] **编译验证**：`cargo check -p kaleido-core` + `cargo test -p kaleido-core`（62 tests, clippy 0 warnings）
- [x] **core 数据模型补全**：Scene 树操作（reparent 防环 / 重排序 / validate）、Tile 行拷贝优化与脏瓦片追踪、Timeline 关键帧采样、Mask/Selection 操作、文本 run 校验、文档 JSON 序列化
- [ ] **traits 契约层重构**：12 个服务的 trait 定义，对齐新数据模型
- [ ] 服务层接口定义（数据 / 历史 / 图层 / 选区 / 颜色 / 渲染 …）
- [ ] 桌面端接入新数据模型
- [ ] 冻结 WIT / ABI + 第一个 WASM 示例插件
