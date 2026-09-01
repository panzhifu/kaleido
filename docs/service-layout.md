# 服务层目录规范(按服务分层 + 按功能拆文件)

> 目标:traits 契约与 services 实现**镜像同构**——每个服务一个目录,目录内按功能拆文件。

## 1. traits 契约层:按服务建目录

```
kaleido-traits/src/services/
  mod.rs                    ServiceError / ServiceResult + 13 个服务模块声明
  data/
    mod.rs                  DataService trait(文档生命周期/写路径/撤销栈/导出)
    format.rs               ★ 文件格式契约(ImageFormat / FileCodec / FormatCodec / FileCodecRegistry)
  history/  mod.rs          HistoryService trait
  layer/    mod.rs          LayerService trait
  selection/mod.rs          SelectionService trait
  color/    mod.rs          ColorService trait
  render/   mod.rs          RenderService trait
  plugin/   mod.rs          PluginService trait(从 services/plugin 上移,契约归位)
  app/      mod.rs          AppService trait
  resource/ mod.rs          ResourceService trait + ResourceKind / ResourceData
  shortcut/ mod.rs          ShortcutService trait
  ui/       mod.rs          UiService trait + MAX_NOTIFICATIONS
  task/     mod.rs          TaskService trait + TaskId / TaskStatus
  ai/       mod.rs          AIAgent 契约(从 ai_agent/ 归位)
```

## 2. services 实现层:服务目录内按功能拆文件

```
crates/kaleido-services/src/services/
  data/
    mod.rs                  DataServiceImpl 主体 + plugin + 生命周期(new/open/save/close)
    format.rs               ★ 格式解析(.kld JSON vs 位图 codec,扩展名推断)
    export.rs               ★ export_flattened 场景合成
    undo.rs                 ★ 撤销栈(双栈快照 + 标签 + 上限)
    async_io/               (已有:异步加载/保存)
    legacy/                 (已有:旧 image_store / file_codec)
  history/
    mod.rs                  HistoryServiceImpl 外观
    tile_history/           (已有)
    legacy_history_keeper/  (已有)
  layer/  mod.rs            LayerServiceImpl(+ legacy_*)
  selection/ mod.rs         (+ legacy_*)
  color/  mod.rs
  render/
    mod.rs                  RenderServiceImpl(合成)
    blend/  blend_simd/  canvas/  op_graph/(已有,按功能)
  plugin/
    mod.rs                  PluginService 实现 + 安装
    host.rs  wasm_host.rs   (已有:清单/管理器/wasmtime)
    tool_registry/  interactive_tool/(已有)
  app/    mod.rs  kaleido_app.rs  cordis_plugins.rs(已有拆分)
  resource/  shortcut/  ui/  task/  ai/    mod.rs(+ 已有子模块)
```

## 3. 执行规则

- **时机**:13 个子代理全部完成后统一执行(避免并行冲突)。
- **兼容**:lib.rs / traits lib.rs 的对外 re-export 路径保持不变(旧 `kaleido_services::X`、`kaleido_traits::X` 继续可用);内部 `use` 路径统一为 `crate::services::<svc>::<file>::…`。
- **迁移项**:`kaleido-traits::file_codec`(→ services/data/format)、`kaleido-traits::ai_agent`(→ services/ai)、`PluginService` 契约(services/plugin → traits/services/plugin)。
- **验证**:`cargo check --workspace` 0 error + `cargo test --workspace` 全绿。
