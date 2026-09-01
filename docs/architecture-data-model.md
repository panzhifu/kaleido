# Kaleido 数据模型设计（v1 草案）

> 定位：重构后的顶层数据模型。宿主唯一的数据结构事实来源，所有服务与插件都建立在这套模型之上。
> 状态：2026-08-31 讨论定稿草案，细节仍可迭代。

## 1. 设计原则

| 原则 | 说明 |
|------|------|
| **统一对象树** | 五种模式（矢量 / 像素 / 绘画 / 排版 / 动画）共享同一棵节点树，模式只是编辑视角，不换数据结构 |
| **性能优先** | 稀疏瓦片、Arc 共享 + 写时复制、稳定节点 ID、缓存位、SIMD 友好布局 |
| **借鉴** | Figma（对象树）、Krita（节点 + 逐帧动画）、Photoshop（瓦片 / 选区 = alpha 通道）、After Effects（关键帧轨道）、Illustrator（贝塞尔节点路径） |

## 2. 顶层：Document

```rust
struct Document {
    id: DocumentId,
    name: String,
    size: ImageSize,                // 画布宽高
    dpi: f32,
    color_profile: ColorProfile,    // 色彩配置（ICC）
    scene: Scene,                   // ★ 核心：对象树
    selection: Option<SelectionMask>, // 当前选区（灰度蒙版）
    history: HistoryState,          // undo/redo 栈
    timeline: Timeline,             // 动画时间轴（双轨）
    resources: ResourceRefs,        // 引用字体 / 色卡 / 笔刷
    metadata: DocumentMeta,         // 创建时间、作者、修改记录
}
```

## 3. 对象树 Scene

节点用 slotmap 式稳定 ID 组织（删除节点不使其他引用失效，避免树指针悬挂）。

```rust
struct Scene {
    root: NodeId,
    nodes: NodeMap,                 // NodeId -> Node
}

struct Node {
    id: NodeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,          // 组才非空
    name: String,
    transform: Transform2D,         // 位置 / 旋转 / 缩放（矢量、排版、动画都依赖）
    opacity: f32,
    visible: bool,
    locked: bool,
    blend_mode: BlendMode,
    content: NodeContent,           // ★ 类型分派（五种模式落点）
    mask: Option<Mask>,             // 蒙版（灰度 / 矢量）
    effects: Vec<EffectBinding>,    // ★ 效果链（插件提供，含调整层语义）
}

enum NodeContent {
    Group,                          // 组
    Pixel(PixelLayer),              // 像素层 —— 像素 / 绘画模式
    Vector(VectorObject),           // 矢量对象 —— 矢量模式
    Text(TextObject),               // 文本对象 —— 排版模式
}
```

## 3.5 效果链 EffectBinding（插件提供）

调整层**不内置为节点类型**，而是通过「效果链」落地——效果实现全部由插件提供（符合宿主只做基础设施的定位）：

```rust
struct EffectBinding {
    effect: EffectId,               // 插件注册的效果 ID
    params: EffectParams,           // 参数（可序列化、可动画）
    scope: EffectScope,             // SelfOnly / Subtree
    enabled: bool,
}

enum EffectScope {
    SelfOnly,                       // 滤镜：只影响本节点内容
    Subtree,                        // 调整层语义：影响本节点及所有后代合成结果
}
```

## 4. 节点类型

### 4.1 Group 组
容纳子节点的容器，自身无像素。合成时组 = 子树先合成再与外部混合。

### 4.2 PixelLayer 像素层（性能核心）

```rust
struct PixelLayer {
    frames: Vec<FramePixels>,       // 逐帧瓦片快照；静态文档 len == 1
    tile_size: u32,                 // 固定 256×256
    format: PixelFormat,            // RGB8 / RGBA8 / RGBA16F / RGB32F
}

struct FramePixels {
    tiles: Arc<TileMap>,            // 稀疏瓦片图，Arc 共享，写时复制
}

struct TileMap {
    tiles: HashMap<TileCoord, Arc<Tile>>, // 只分配实际绘制过的区域
}

struct Tile {
    data: Arc<[u8]>,                // 像素数据，共享所有权
    dirty: bool,                    // 脏标记（配合增量渲染）
}
```

性能要点：
- **稀疏分配**：空白画布零像素内存，画到哪才分配哪
- **Arc + COW**：undo 快照、多视图、逐帧动画共享未修改瓦片
- **逐帧支持**：`frames` 数组让手绘逐帧动画（每帧一组瓦片）与静态文档共用同一结构

### 4.3 VectorObject 矢量对象

```rust
struct VectorObject {
    paths: Vec<VectorPath>,
    fill: FillStyle,                // 纯色 / 渐变
    stroke: StrokeStyle,            // 描边
    raster_cache: Option<CacheKey>, // 栅格化缓存（避免重复重绘）
}

struct VectorPath {
    nodes: Vec<PathNode>,           // 节点式（锚点 + 控制点，可编辑）
    closed: bool,
}

struct PathNode {
    anchor: Point,                  // 锚点
    control_in: Option<Point>,      // 入控制点
    control_out: Option<Point>,     // 出控制点
    smooth: bool,                   // 平滑 / 尖角
}
```

节点式路径（借鉴 Illustrator / Inkscape）保证矢量模式的核心诉求：**形状可编辑**。

### 4.4 TextObject 文本对象（排版模式）

```rust
struct TextObject {
    runs: Vec<TextRun>,             // 富文本分段：字体 / 字号 / 颜色
    font: ResourceId,               // 引用资源管理的字体
    size: f32,
    align: TextAlign,
    frame: Option<TextFrame>,       // 定宽文本框（换行）或自由文本
}
```

### 4.5 效果链（调整层 = 插件效果）

亮度/对比度、色相/饱和度、曲线、模糊等**全部做成插件效果**，挂在任意节点的 `effects` 链上（见 3.5）：

- **插件注册**：效果插件通过 SDK 注册「效果处理器」（输入：合成结果像素 → 输出：处理后像素），宿主只定义 Effect 接口契约
- **渲染时机**：节点合成 → 按序执行效果链 → 输出；`Subtree` 作用域的效果在子树合成后应用（即 PS 调整层语义）
- **可动画**：效果参数可被 Timeline 关键帧驱动（如亮度随时间变化）
- **v1 计划**：宿主先定义 Effect 契约 + 内置 1-2 个示例效果（如亮度/对比度）验证链路，其余由社区插件补充

## 5. 动画 Timeline（双轨）

```rust
struct Timeline {
    frame_rate: u32,                // 24 / 30 / 60
    duration: u32,                  // 总帧数
    tracks: Vec<Track>,             // 轨道 1：属性关键帧（AE 式）
    // 轨道 2：逐帧手绘动画 —— 由各 PixelLayer.frames 承载
}

struct Track {
    node: NodeId,                   // 绑定哪个节点
    prop: AnimatableProp,           // Transform / Opacity / FillColor…
    keyframes: Vec<Keyframe>,
}

struct Keyframe {
    frame: u32,
    value: AnimValue,               // 枚举：位置 / 缩放 / 透明度 / 颜色…
    easing: Easing,                 // 缓动：线性 / 贝塞尔 / 保持
}
```

两条轨道各司其职：
| 轨道 | 模型 | 借鉴 | 适用 |
|------|------|------|------|
| 逐帧手绘 | 每帧一组像素瓦片（PixelLayer.frames） | Krita | 手绘动画（一帧帧画成动画） |
| 属性关键帧 | Track + Keyframe 插值 | After Effects / Spine | 矢量 / 排版 / 位移动画 |

## 6. 蒙版与选区（同一套灰度蒙版，互转）

```rust
struct Mask {
    kind: MaskKind,                 // 图层蒙版 / 矢量蒙版
    data: MaskData,
}

enum MaskData {
    Grayscale(Option<MaskTiles>),   // 灰度蒙版
    Vector(VectorObject),           // 矢量蒙版
}

struct SelectionMask {
    tiles: Option<MaskTiles>,       // 灰度蒙版；None = 全选
}
```

选区与图层蒙版**共用同一数据结构**（借鉴 PS：选区本质是 alpha 通道），支持互转，只实现一套蒙版逻辑。

## 7. 历史 HistoryState

```rust
struct HistoryState {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    limit: usize,                   // 深度上限，溢出合并 / 丢弃
}
// HistoryEntry：命令描述（对哪些节点 / 瓦片做了什么），配合脏瓦片快照
```

命令模式 + 脏瓦片快照（TileSnapshotCommand），只记录被改的瓦片，不整图快照。

## 8. 资源引用 ResourceRefs

```rust
struct ResourceRefs {
    fonts: Vec<ResourceId>,
    swatches: Vec<ResourceId>,      // 色卡
    brushes: Vec<ResourceId>,       // 笔刷
    // 只引用资源管理，不持有数据
}
```

## 9. 色彩 ColorProfile

- 色彩空间：sRGB / linear / CMYK / Lab
- 位深：8bit / 16bit / 32bit 浮点
- 关联 ICC profile；文档级配置，随文档保存

## 10. 性能策略汇总

1. 稀疏瓦片（256×256）：只分配实际绘制区域
2. Arc 共享 + 写时复制：undo / 多视图 / 逐帧动画零拷贝共享
3. slotmap 稳定节点 ID：删除不使引用失效
4. 缓存位：矢量栅格缓存、文本布局缓存、OpGraph 节点合成缓存
5. SIMD 友好：像素线性内存 + 位深对齐，blend_simd 直接消费

## 11. 五种模式 ↔ 数据结构映射

| 模式 | 操作对象 |
|------|----------|
| 矢量 | VectorObject 的 PathNode / Fill / Stroke |
| 像素 | PixelLayer 的 Tile 读写 |
| 绘画 | PixelLayer + 画笔交互（工具插件化） |
| 排版 | TextObject 的 runs / frame |
| 动画 | Timeline（逐帧 frames + 属性 tracks） |

## 12. 待定项

- [x] 调整层 → 改为插件效果链（Node.effects / EffectBinding），宿主只定义 Effect 契约，v1 内置 1-2 个示例效果
- [ ] 文本引擎细节：竖排 / RTL / 行距字距
- [ ] 逐帧动画的内存策略（帧上限、未修改帧共享细节）
- [ ] 文档工程格式 `.kld` 的序列化方案
