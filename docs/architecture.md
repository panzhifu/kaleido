# 图像编辑软件架构调研与 Kaleido 开发计划

## 目录

1. [调研对象](#1-调研对象)
2. [核心架构对比](#2-核心架构对比)
3. [各维度方案分析](#3-各维度方案分析)
4. [Kaleido 开发计划](#4-kaleido-开发计划)

---

## 1. 调研对象

| 软件 | 定位 | 技术栈 | 核心优势 |
|---|---|---|---|
| **Adobe Photoshop** | 专业图像处理 | C++ / Objective-C | 行业标准、GPU 加速、非破坏性编辑 |
| **GIMP** | 开源图像编辑 | C / GEGL | 图结构处理、插件生态 |
| **Affinity Photo** | 专业图像编辑（现代） | C++ / Rust | 性能极佳、Metal/OpenCL、非破坏性实时滤镜 |
| **Krita** | 数字绘画 | C++ / Qt | OpenGL 画布、笔刷引擎、动画制作 |
| **Photopea** | 在线图像编辑 | JavaScript / WebGL / WASM | 零安装、跨平台、WebGL 渲染 |
| **Pixelmator Pro** | Mac 图像编辑 | Swift / Metal | Metal 全管线、ML 功能、Apple 生态 |
| **Paint.NET** | 轻量图像编辑 | C# / Direct2D | 简洁、Direct2D 硬件加速 |
| **Acorn** | Mac 轻量编辑 | Objective-C / Metal | Metal 渲染、简洁 |

---

## 2. 核心架构对比

### 2.1 数据存储模型

| 软件 | 存储模型 | Tile 大小 | 撤销实现 | 大图处理 |
|---|---|---|---|---|
| **Photoshop** | Tile-based（每层独立） | 256×256（可配） | 脏 tile 快照 + 历史状态 | 内存映射文件 + LRU |
| **GIMP** | Tile Manager（全局共享） | 64×64 | 脏 tile 旧版本 | LRU 淘汰 + swap |
| **Affinity Photo** | 自适应 tile（大 tile + 子 tile） | 512×512（内部） | 操作日志 + 增量快照 | 流式加载 |
| **Krita** | Tile-based（每层独立） | 64×64 | 快照 + 撤销栈 | 内存映射 |
| **Photopea** | ArrayBuffer（连续内存） | 无 tile | 全量快照 | 浏览器内存限制 |
| **Pixelmator Pro** | Metal Texture（GPU 端） | GPU tile | 操作日志 | 统一内存架构 |
| **Paint.NET** | 连续 Bitmap | 无 tile | 全量快照 | 系统内存限制 |
| **Acorn** | Metal Texture | GPU tile | 操作日志 | 统一内存 |

### 2.2 渲染管线

| 软件 | 渲染后端 | GPU 角色 | 渐进渲染 | 图层混合 |
|---|---|---|---|---|
| **Photoshop** | GPU（Metal/OpenGL） | 全管线：2D 变换 + 滤镜 | ✅ 低分辨率预览 | GPU shader |
| **GIMP** | CPU（C 循环） | 无（GIMP 3 部分 OpenCL） | ❌ | CPU 逐像素 |
| **Affinity Photo** | GPU（Metal/OpenCL） | 实时滤镜 + 显示 | ✅ 实时 | GPU compute |
| **Krita** | OpenGL 2.4+ | 画布 + 笔刷 | ✅ 低分辨率预览 | OpenGL shader |
| **Photopea** | WebGL 2.0 | 2D 变换 + 合成 | ✅ | WebGL shader |
| **Pixelmator Pro** | Metal 2/3 | 全管线 | ✅ | Metal compute |
| **Paint.NET** | Direct2D | 2D 加速 | ❌ | Direct2D 效果 |
| **Acorn** | Metal | 显示 + 基础滤镜 | ✅ | Metal shader |

### 2.3 非破坏性编辑模型

| 软件 | 模型 | 调整图层 | 智能对象/滤镜 | 实现方式 |
|---|---|---|---|---|
| **Photoshop** | Smart Object + Adjustment Layer | ✅ | ✅ Smart Filter | 节点图 + 延迟渲染 |
| **GIMP** | 传统破坏性（GIMP 3 非破坏性） | ✅ (3.x) | ✅ (3.x) | GEGL 图 |
| **Affinity Photo** | Live Filter Layer | ✅ | ✅ Live Filter | 操作图 + 实时计算 |
| **Krita** | Filter Mask / Filter Layer | ✅ | ❌ | 节点图 |
| **Photopea** | Adjustment Layer | ✅ | ❌ | 模拟 PS 行为 |
| **Pixelmator Pro** | ML Adjustment | ✅ | ✅ ML Repair | 操作日志 |
| **Paint.NET** | 无 | ❌ | ❌ | — |
| **Acorn** | 无 | ❌ | ❌ | — |

### 2.4 处理引擎架构

| 软件 | 处理模型 | 并行策略 | 操作合并 |
|---|---|---|---|
| **Photoshop** | 内部节点图 | 多核 + GPU | 自动（引擎内部） |
| **GIMP** | GEGL 图 | OpenCL / 线程池 | 自动（GEGL） |
| **Affinity Photo** | 内部操作图 | 多核 + GPU | 自动（引擎） |
| **Krita** | GEGL + 内部 | 多核 + OpenGL | 部分 |
| **Photopea** | 无图结构 | WASM 多线程 | ❌ |
| **Pixelmator Pro** | Metal 计算图 | GPU 全并行 | 自动 |
| **Paint.NET** | 无 | 单核 | ❌ |
| **Acorn** | 无 | GPU 并行 | ❌ |

### 2.5 插件/扩展系统

| 软件 | 插件技术 | 生态 | 脚本支持 |
|---|---|---|---|
| **Photoshop** | C++ SDK / UXP (JS) | 极其丰富 | JavaScript / AppleScript |
| **GIMP** | C / Script-Fu / Python-Fu | 丰富 | Scheme / Python / C |
| **Affinity Photo** | 无官方插件 | — | 宏录制 |
| **Krita** | Python / C++ | 中等 | Python |
| **Photopea** | 无（Web 应用） | — | — |
| **Pixelmator Pro** | 无 | — | 无 |
| **Paint.NET** | C# 插件 | 中等 | — |
| **Acorn** | 无 | — | — |

### 2.6 色彩管理

| 软件 | 色彩引擎 | ICC 支持 | 色域 | 位深度 |
|---|---|---|---|---|
| **Photoshop** | Adobe CMM | ✅ 全支持 | Lab/CMYK/RGB | 8/16/32 bit |
| **GIMP** | babl + LCMS | ✅ | RGB/Indexed | 8/16/32 bit |
| **Affinity Photo** | 自研 | ✅ | RGB/CMYK/Lab | 8/16/32 bit |
| **Krita** | LCMS | ✅ | RGBA/CMYK | 8/16/32 bit |
| **Photopea** | 无 | ❌ | sRGB only | 8 bit |
| **Pixelmator Pro** | Core Image | ✅ | Display P3 | 8/16 bit |
| **Paint.NET** | 无 | ❌ | sRGB | 8 bit |
| **Acorn** | Core Image | ✅ | Display P3 | 8/16 bit |

---

## 3. 各维度方案分析

### 3.1 数据存储：Tile 大小选择

| 方案 | 优点 | 缺点 | 适用场景 |
|---|---|---|---|
| **64×64**（GIMP） | 精细粒度，内存利用率高 | Tile 数量多，管理开销大 | 内存受限、小图 |
| **128×128**（Kaleido 已选） | 平衡粒度与管理开销 | 大图仍有较多 tile | 通用场景 |
| **256×256**（PS） | Tile 数量少，管理简单 | 内存浪费（边缘 tile） | 大图、内存充足 |
| **512×512**（Affinity） | 超大图友好 | 小图浪费严重 | 专业大图处理 |
| **无 Tile**（Photopea） | 实现简单 | 大图 OOM、无法并行 | 小图、Web 环境 |

**Kaleido 选择：128×128（已确定）**

原因：
- 现代 CPU L1 cache 32KB，L2 256KB，L3 数 MB
- 128×128 RGBA8 = 64KB，正好占满 L2，L1 放得下活跃行
- 在 64 和 256 之间取平衡
- 256×256 RGBA8 = 256KB，超过 L2 cache，效率下降
- 与 tile 数量（管理开销）之间取折中

### 3.2 渲染后端选择

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **CPU 全管线**（GIMP） | 兼容性好、无驱动问题 | 慢、大滤镜卡顿 | GIMP、旧版 PS |
| **GPU 全管线**（Pixelmator） | 极快、实时反馈 | 驱动兼容、GPU 差异 | Pixelmator Pro、Acorn |
| **CPU 处理 + GPU 显示**（Krita） | 平衡、兼容性好 | 滤镜仍需 CPU | Krita、PS（混合） |
| **WebGL/WebGPU**（Photopea） | 跨平台、零安装 | 性能受限于浏览器 | Photopea |
| **Metal**（Apple） | Apple 生态最优 | 仅限 Apple | Pixelmator、Acorn |
| **Direct2D/Direct3D**（Windows） | Windows 原生 | 仅限 Windows | Paint.NET |

**Kaleido 选择：CPU 处理 + GPU 显示（GPUI）**

原因：
- Kaleido 已基于 GPUI，其 GPU 后端已实现 Metal/OpenGL/Vulkan 抽象
- 像素处理需要精确控制（色彩精度、SIMD），CPU 更可控
- GPU 用于 2D 变换（缩放/平移/旋转）和图层合成，这些天然适合 GPU
- 避免 GPU compute 的精度问题（浮点 vs 整数、驱动差异）
- 保留未来接入 GPU compute 的能力（通过 Metal compute shader）

### 3.3 非破坏性编辑模型

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **操作图（节点图）**（GEGL） | 最灵活、自动优化 | 实现复杂 | GIMP 3、Affinity |
| **调整图层栈**（PS） | 用户理解直观 | 复杂交互受限 | Photoshop |
| **Live Filter**（Affinity） | 实时预览、直观 | 内存占用高 | Affinity Photo |
| **操作日志**（Pixelmator） | 简单、内存友好 | 无法修改历史参数 | Pixelmator、Acorn |
| **破坏性**（传统） | 最简单 | 不可逆 | Paint.NET、早期软件 |

**Kaleido 选择：操作图（Op Graph）+ 调整图层（上层封装）**

原因：
- Op Graph 作为底层：灵活、支持自动合并、ROI 传播
- 调整图层作为上层 UI 概念：映射到 Op Graph 节点
- 调整图层 = Op Graph 中的一个特殊节点（不修改输入，只声明参数）
- 用户看到"调整图层面板"，底层是 Op Graph
- 这比 PS 的"调整图层 + 像素图层"二元模型更统一

### 3.4 GPU 像素处理

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **全 GPU**（Pixelmator） | 极快 | 精度问题、驱动差异 | Pixelmator Pro |
| **GPU 可选**（Affinity） | 灵活 | 需要两套实现 | Affinity Photo |
| **全 CPU**（GIMP） | 精确、兼容 | 慢 | GIMP |
| **CPU + GPU 混合**（PS） | 平衡 | 复杂 | Photoshop |

**Kaleido 选择：全 CPU（Phase 1-3），GPU 可选扩展（远期）**

原因：
- 像素级操作需要精确控制（色彩空间、精度、边缘处理）
- SIMD（SSE/AVX/NEON）已能提供 4-8 倍加速，满足大多数场景
- GPU compute 的精度问题（FP16 vs FP32）在色彩敏感场景不可接受
- 保留接口：Op trait 的 compute_roi 未来可实现 GPU 版本
- 当滤镜复杂度超过阈值（如 4K 图像的复杂卷积），再考虑 GPU

### 3.5 并行策略

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **Tile 级并行**（rayon） | 简单、无数据竞争 | Tile 间有依赖时复杂 | Kaleido（已选） |
| **行级并行** | 细粒度 | 同步开销大 | 简单场景 |
| **SIMD 向量化** | 单核性能极致 | 算法需适配 SIMD | 所有现代软件 |
| **GPU 全并行** | 海量并行 | 数据传输开销 | PS、Affinity |
| **任务图并行** | 自动依赖分析 | 实现复杂 | Affinity |

**Kaleido 选择：Tile 级并行（rayon）+ SIMD 向量化**

原因：
- Tile 级并行：每个 tile 独立计算，无数据竞争
- rayon 的 par_iter 自动处理线程池、负载均衡
- 与 Op Graph 配合：不同 tile 的子图可并行执行
- SIMD 在 pixel_convert 中已实现（RGBA↔Gray）
- 两者结合：tile 间并行（多核）+ tile 内并行（SIMD）= 最大化性能

### 3.6 撤销/历史管理

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **脏 tile 快照**（GIMP） | 内存友好 | 实现复杂 | GIMP |
| **操作日志**（Pixelmator） | 极简 | 撤销需重新计算 | Pixelmator、Acorn |
| **全量快照**（Photopea） | 简单 | 内存爆炸 | Photopea、Paint.NET |
| **增量快照 + 压缩**（PS） | 平衡 | 实现复杂 | Photoshop |
| **命令模式 + 脏 tile**（Kaleido 已有） | 灵活 | 需扩展 | Kaleido |

**Kaleido 选择：命令模式 + 脏 tile 快照（扩展现有 HistoryKeeper）**

原因：
- 现有 HistoryKeeper 已基于命令模式，扩展成本低
- 脏 tile 快照：只保存被修改 tile 的旧版本
- 结合 TiledImage：每个 tile 维护版本链
- 撤销时只恢复脏 tile，不触碰未修改 tile
- 内存占用与修改区域成正比，而非全图大小

### 3.7 色彩管理

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **全 ICC 支持**（PS） | 专业、精确 | 复杂 | Photoshop、GIMP |
| **sRGB 为主**（Photopea） | 简单 | 不专业 | Photopea、Paint.NET |
| **系统色彩管理**（Acorn） | 简单、系统集成 | 平台依赖 | Acorn、Pixelmator |
| **babl 库**（GIMP） | 高效、灵活 | 额外依赖 | GIMP |

**Kaleido 选择：sRGB 为主 + ICC 接口预留**

原因：
- Phase 1-3 不需要完整色彩管理（Web/sRGB 覆盖 95% 场景）
- 在 PixelFormat 中预留色彩空间字段
- 未来接入 little-rs-CMS 或绑定 LCMS2
- pixel_convert 服务预留色彩空间转换接口
- 避免过早引入复杂性

### 3.8 AI 功能集成

| 方案 | 优点 | 缺点 | 采用者 |
|---|---|---|---|
| **本地 ML 模型**（Pixelmator） | 隐私、离线 | 模型大小、推理速度 | Pixelmator Pro |
| **云端 API** | 无需本地资源 | 网络依赖、隐私 | Photopea |
| **混合**（PS） | 灵活 | 复杂 | Photoshop (Firefly) |
| **LLM 规划 + 工具调用** | 灵活、可解释 | LLM 延迟、成本 | Kaleido（已选方向） |

**Kaleido 选择：LLM 规划 + Op Graph 工具调用**

原因：
- AI Agent 构建 Op Graph，不直接操作像素
- 每个 Op 对应一个图像操作（brightness、blur、crop 等）
- LLM 理解自然语言 → 规划操作序列 → 构建 Op Graph → 执行
- 优势：可解释（能看到操作链）、可中断、可编辑
- 劣势：LLM 延迟（但 Op Graph 可流式显示进度）
- 接口预留：AIToolGenerator 作为 Cordis 服务

---

## 4. Kaleido 开发计划

### Phase 1: 性能地基 ✅ 已完成

| 服务 | 状态 | 验证 |
|---|---|---|
| `TiledImage` + `Tile` | ✅ | 51 测试通过 |
| SIMD pixel_convert（6 条路径） | ✅ | 8 新增测试通过 |
| `read_pixel` / `write_pixel` pub(crate) | ✅ | — |

### Phase 2: 执行引擎 ✅ 已完成

| 服务 | 状态 | 验证 |
|---|---|---|
| `OpGraph`（DAG + 拓扑排序） | ✅ | 3 测试 |
| `Op` trait（ROI 驱动） | ✅ | — |
| `GraphExecutor`（tile 并行） | ✅ | 2 测试 |
| `FusedOp`（point-op 融合） | ✅ | 1 测试 |
| `Rect`（ROI 操作） | ✅ | 2 测试 |

### Phase 3: 渲染与 I/O（P1）— 下一步

#### 3.1 GPU Canvas Service

**目标**：桌面端丝滑缩放/平移/旋转

**方案对比**：

| 方案 | 优点 | 缺点 | 选择 |
|---|---|---|---|
| A) GPUI 原生 canvas | 已集成、跨平台 | GPUI API 不稳定 | ✅ 选这个 |
| B) 自研 GPU 层 | 完全控制 | 工作量巨大 | ❌ |
| C) 第三方库（wgpu） | 成熟 | 与 GPUI 重复 | ❌ |

**选择 A 的原因**：
- Kaleido 已基于 GPUI，其内部已有 GPU 抽象（Metal/OpenGL/Vulkan）
- GPUI 的 `Scene` 和 `Primitive` 系统可直接用于图像显示
- 自研 GPU 层工作量远超项目范围

**接口设计**：

```rust
pub struct CanvasService {
    scene: gpui::Scene,
    texture_cache: HashMap<TileCoord, gpui::Texture>,
    viewport: Viewport,
}

pub struct Viewport {
    zoom: f32,
    offset: Vec2,
    rotation: f32,
}

impl CanvasService {
    /// 设置视口（GPU 直接变换，不重采样原图）
    pub fn set_viewport(&mut self, zoom: f32, offset: Vec2);

    /// 渲染可见区域
    pub fn render_visible(&mut self, image: &TiledImage, visible: Rect);

    /// 渐进预览
    pub fn render_progressive(&mut self, image: &TiledImage, quality: RenderQuality);

    /// 坐标转换：屏幕 ↔ 图像
    pub fn screen_to_image(&self, screen: Vec2) -> Vec2;
    pub fn image_to_screen(&self, image: Vec2) -> Vec2;
}
```

**关键实现**：
- 每个 tile 作为独立 texture 上传到 GPU
- 缩放/平移/旋转通过 transform matrix 实现（零像素计算）
- 只上传可见 tiles 到 GPU
- 纹理缓存避免重复上传

#### 3.2 异步文件加载器

**目标**：大图不卡 UI，渐进显示

**方案对比**：

| 方案 | 优点 | 缺点 | 选择 |
|---|---|---|---|
| A) tokio::spawn | 标准异步 | 需要 tokio runtime | ✅ 选这个 |
| B) 线程池 + channel | 简单 | 手动管理线程 | ❌ |
| C) rayon::spawn | 已有 rayon | 不适合 I/O 密集 | ❌ |

**选择 A 的原因**：
- I/O 密集操作（文件读取）适合 async/await
- tokio 是 Rust 异步标准
- 与 GPUI 的 async 集成良好

**接口设计**：

```rust
pub struct AsyncImageLoader;

impl AsyncImageLoader {
    /// 加载缩略图（快速预览）
    pub async fn load_preview(path: &Path, max_size: u32) -> ImageResult<TiledImage>;

    /// 后台加载全分辨率
    pub async fn load_full(
        path: &Path,
        priority: LoadPriority,
    ) -> ImageResult<TiledImage>;

    /// 取消加载
    pub fn cancel(&self, request_id: LoadRequestId);
}

pub enum LoadPriority {
    /// 可见区域优先
    VisibleFirst(Rect),
    /// 从中心向外
    CenterOut,
    /// 顺序加载
    Sequential,
}
```

#### 3.3 后台保存器

**目标**：保存不阻塞编辑

**方案对比**：

| 方案 | 优点 | 缺点 | 选择 |
|---|---|---|---|
| A) tokio::spawn_blocking | 标准 | 需要 tokio | ✅ 选这个 |
| B) 专用线程 | 简单 | 资源浪费 | ❌ |
| C) 同步保存 | 最简单 | 阻塞 UI | ❌ |

**接口设计**：

```rust
pub struct BackgroundSaver {
    sender: Sender<SaveRequest>,
}

struct SaveRequest {
    image: TiledImage,
    path: PathBuf,
    format: ImageFormat,
    response: oneshot::Sender<ImageResult<()>>,
}

impl BackgroundSaver {
    pub async fn save(
        &self,
        image: TiledImage,
        path: PathBuf,
        format: ImageFormat,
    ) -> ImageResult<()>;
}
```

### Phase 4: 撤销扩展（P1）

**目标**：脏 tile 快照替代全量快照

**现有问题**：HistoryKeeper 基于 `SnapshotCommand`（全量前后快照），大图内存爆炸

**方案对比**：

| 方案 | 优点 | 缺点 | 选择 |
|---|---|---|---|
| A) 脏 tile 版本链 | 内存友好 | 实现复杂 | ✅ 选这个 |
| B) 操作日志重放 | 极简 | 撤销慢 | ❌ |
| C) 全量压缩 | 实现简单 | 仍耗内存 | ❌ |

**选择 A 的原因**：
- 与 TiledImage 天然配合
- 内存占用 ∝ 修改区域（而非全图）
- 撤销速度 O(脏 tile 数)（常数时间）

**接口设计**：

```rust
pub struct TileVersion {
    data: Arc<Vec<u8>>,
    timestamp: Instant,
}

pub struct TileHistory {
    versions: VecDeque<TileVersion>,  // 固定容量（如 20 步）
    max_versions: usize,
}

impl TileHistory {
    pub fn push(&mut self, data: Arc<Vec<u8>>);
    pub fn undo(&mut self) -> Option<Arc<Vec<u8>>>;
    pub fn redo(&mut self) -> Option<Arc<Vec<u8>>>;
}

pub struct TiledHistoryKeeper {
    tile_histories: HashMap<TileCoord, TileHistory>,
    current_step: usize,
}
```

### Phase 5: 图层系统（P2）

**目标**：基础图层管理（不含高级混合模式）

**方案对比**：

| 方案 | 优点 | 缺点 | 选择 |
|---|---|---|---|
| A) 每层 = TiledImage + 元数据 | 简单、复用基础层 | 无高级混合 | ✅ 选这个 |
| B) 统一 Op Graph | 最灵活 | 实现复杂 | 远期 |
| C) PS 式 Layer 对象 | 功能全 | 过于复杂 | ❌ |

**选择 A 的原因**：
- 每层是独立的 TiledImage，复用所有基础层服务
- 图层合成 = 自底向上逐层混合（CPU SIMD 或 GPU）
- 调整图层 = Op Graph 节点（不修改原图）
- 为 Phase 6 的高级功能打底

**接口设计**：

```rust
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub content: LayerContent,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    pub visible: bool,
    pub locked: bool,
    pub mask: Option<TiledImage>,
}

pub enum LayerContent {
    Pixels(TiledImage),
    Adjustment(Box<dyn Op>),  // 调整图层 = Op 节点
}

pub struct LayerStack {
    layers: Vec<Layer>,
    width: u32,
    height: u32,
    composited_dirty: bool,
}

impl LayerStack {
    pub fn add_layer(&mut self, layer: Layer) -> LayerId;
    pub fn remove_layer(&mut self, id: LayerId);
    pub fn reorder(&mut self, id: LayerId, new_index: usize);
    pub fn composite(&self) -> ImageResult<TiledImage>;
}
```

### Phase 6: 高级功能（P3）

#### 6.1 高级混合模式

| 模式 | 实现方式 |
|---|---|
| Normal / Multiply / Screen / Overlay | SIMD 逐像素 |
| Color Dodge / Color Burn | SIMD |
| Hue / Saturation / Color / Luminosity | 色彩空间转换 + SIMD |

#### 6.2 蒙版系统

- 图层蒙版 = 灰度 TiledImage
- 矢量蒙版 = 路径 + 栅格化
- 剪贴蒙版 = 图层组蒙版

#### 6.3 调整图层（非破坏性）

- 曲线 / 色阶 / 色相饱和度 = Op Graph 节点
- 实时预览 = Op Graph 增量计算

### Phase 7: AI 集成（P3）

**目标**：LLM 规划 + Op Graph 工具调用

**接口预留**（已实现）：

```rust
pub trait AIAgent: Send + Sync + 'static {
    fn plan(&self, goal: &str, context: Option<&serde_json::Value>) -> AgentResult<Plan>;
    fn execute_plan(&self, plan: &Plan) -> AgentResult<PlanResult>;
    fn run(&self, goal: &str, context: Option<&serde_json::Value>) -> AgentResult<PlanResult>;
    fn mode(&self) -> AgentMode;
    fn stats(&self) -> AgentStats;
}

pub struct AIToolGenerator;

impl AIToolGenerator {
    pub fn create_tool(
        description: &serde_json::Value,
        apply_fn: impl Fn(&mut Image, &ToolParams) -> ImageResult<()> + Send + Sync + 'static,
    ) -> Result<DynamicTool>;
}
```

**Phase 7 不做的事**：
- 不训练自有模型（调用外部 LLM API）
- 不实现本地推理（远期）
- 不做图像生成（仅编辑）

---

## 5. 开发优先级总结

| 阶段 | 优先级 | 工作量 | 依赖 | 交付物 |
|---|---|---|---|---|
| Phase 1 | P0 | 已完成 | — | TiledImage + SIMD |
| Phase 2 | P0 | 已完成 | Phase 1 | OpGraph + Executor |
| Phase 3.1 | P1 | 中等 | Phase 2 | GPU Canvas |
| Phase 3.2 | P1 | 中等 | Phase 1 | Async I/O |
| Phase 4 | P1 | 中等 | Phase 1 | 脏 tile 撤销 |
| Phase 5 | P2 | 大 | Phase 2, 4 | 图层系统 |
| Phase 6 | P3 | 大 | Phase 5 | 混合模式 + 蒙版 |
| Phase 7 | P3 | 大 | Phase 2, 5 | AI Agent |

---

## 6. 关键决策记录

### 决策 1：Tile 大小 = 128×128

| | 64×64 | 128×128 | 256×256 | 512×512 |
|---|---|---|---|---|
| L1 友好 | ✅ | ✅ | ❌ | ❌ |
| L2 友好 | ✅ | ✅ | ✅ | ❌ |
| 管理开销 | 高 | 中 | 低 | 低 |
| 内存浪费 | 少 | 中 | 多 | 多 |
| 并行粒度 | 细 | 中 | 粗 | 粗 |

**选择 128×128**：平衡 cache 效率与管理开销。

### 决策 2：GPU 仅用于显示

| | 全 GPU | CPU+GPU 混合 | 全 CPU |
|---|---|---|---|
| 滤镜速度 | 极快 | 快 | 慢 |
| 精度 | 有风险 | 精确 | 精确 |
| 兼容性 | 差 | 好 | 最好 |
| 实现难度 | 高 | 中 | 低 |

**选择混合**：CPU 处理像素（精确）+ GPU 显示（丝滑）。

### 决策 3：Op Graph 作为底层

| | 调整图层栈 | Op Graph | 操作日志 |
|---|---|---|---|
| 灵活性 | 低 | 高 | 中 |
| 自动合并 | ❌ | ✅ | ❌ |
| ROI 传播 | ❌ | ✅ | ❌ |
| 用户理解 | 直观 | 需 UI 包装 | 直观 |
| 实现难度 | 低 | 高 | 低 |

**选择 Op Graph**：底层图结构 + 上层调整图层 UI。

### 决策 4：撤销 = 脏 tile 快照

| | 全量快照 | 操作日志 | 脏 tile 快照 |
|---|---|---|---|
| 内存 | 爆炸 | 极少 | 中等 |
| 撤销速度 | 即时 | 慢 | 即时 |
| 实现难度 | 低 | 低 | 中 |
| 大图友好 | ❌ | ✅ | ✅ |

**选择脏 tile**：与 TiledImage 天然配合，内存可控。

---

## 7. 不做的事（明确排除）

1. **不做 GPU 像素处理**（Phase 1-6）：CPU + SIMD 足够，GPU 精度风险
2. **不做全 ICC 色彩管理**（Phase 1-5）：sRGB 覆盖 95%，接口预留
3. **不做专有二进制格式**：全部走 FileCodecRegistry 插件
4. **不做图像生成**：仅编辑，不做 AI 生成（Stable Diffusion 等）
5. **不做动画制作**：Krita 的强项，不在 Kaleido 范围内
6. **不做 CMYK/印刷支持**：专业印刷是 PS 的领域
7. **不做笔刷引擎**：Krita 的核心，不在范围内

---

## 8. 与竞品的差异化定位

| 特性 | PS | GIMP | Affinity | Krita | Kaleido |
|---|---|---|---|---|---|
| 核心架构 | Tile | GEGL | 自适应 | Tile | **Tile + Op Graph** |
| AI 集成 | Firefly | 无 | 无 | 无 | **LLM 规划** |
| 插件生态 | 极丰富 | 丰富 | 无 | 中 | **WASM + 原生** |
| 非破坏性 | ✅ | 3.x | ✅ | ✅ | **✅ (Op Graph)** |
| 开源 | ❌ | ✅ | ❌ | ✅ | **✅** |
| 语言 | C++ | C | C++ | C++ | **Rust** |

**Kaleido 的独特定位**：
- Rust 内存安全 + 现代并发（rayon + tokai）
- Op Graph 统一处理引擎（自动融合 + ROI 传播）
- LLM 驱动的自然语言编辑（未来）
- WASM 插件沙箱（安全 + 跨平台）
