# 服务层 12 管理器重构(2026-08-31)

> 配套:`docs/refactor-overview.md`(总览)、`docs/architecture-data-model.md`(数据模型)。
> 目标:traits 定义 12 个管理器契约(对齐新 Document 数据模型),services 实现它们,全部作为 Cordis 服务。

## 1. 12 管理器一览

| # | 管理器 | service id | traits 契约 | services 实现 | 依赖 |
|---|--------|-----------|-------------|---------------|------|
| 1 | 数据 Data | `data_service` | `services::data::DataService` | `managers::data::DataServiceImpl` | file_codec |
| 2 | 历史 History | `history_service` | `services::history::HistoryService` | `managers::history::HistoryServiceImpl` | data_service |
| 3 | 图层 Layer | `layer_service` | `services::layer::LayerService` | `managers::layer::LayerServiceImpl` | data_service |
| 4 | 选区 Selection | `selection_service` | `services::selection::SelectionService` | `managers::selection::SelectionServiceImpl` | data_service |
| 5 | 颜色 Color | `color_service` | `services::color::ColorService` | `managers::color::ColorServiceImpl` | data_service |
| 6 | 渲染 Render | `render_service` | `services::render::RenderService` | `managers::render::RenderServiceImpl` | data_service |
| 7 | 插件 Plugin | `plugin_service` | `plugin_service::PluginService`(已在 services) | `plugin_service::PluginServiceImpl`(已完成) | 各注册表 |

> 注:原 `kaleido-plugin-host` crate 已合并进 `kaleido-services` 的 `plugin_service`
> 模块(`host.rs` = manifest/loader/manager/AIToolGenerator;`wasm_host.rs` = wasmtime
> 运行时),原 crate 已删除。WASM 类型路径:
> `crate::plugin_service::wasm_host::WasmPluginManager`、`crate::plugin_service::host::PluginManifest`。
| 8 | 软件 App | `app_service` | `services::app::AppService` | `managers::app::AppServiceImpl` | — |
| 9 | 资源 Resource | `resource_service` | `services::resource::ResourceService` | `managers::resource::ResourceServiceImpl` | — |
| 10 | 快捷键 Shortcut | `shortcut_service` | `services::shortcut::ShortcutService` | `managers::shortcut::ShortcutServiceImpl` | shortcut_registry(已有) |
| 11 | UI | `ui_service` | `services::ui::UiService` | `managers::ui::UiServiceImpl` | panel_registry(已有) |
| 12 | 任务 Task | `task_service` | `services::task::TaskService` | `managers::task::TaskServiceImpl` | — |

## 2. 核心设计:单一写路径 + COW 快照撤销

所有文档级变更**只允许**通过 `DataService::apply_mutation(label, f)` 进入:

```
apply_mutation(label, f)
  ├─ 读当前 Document(COW clone 为 before 快照,Arc 共享瓦片,零成本)
  ├─ 执行 f(&mut Document)(若失败:不动,不记录)
  ├─ Document::touch()
  ├─ before 快照 → history.undo 栈;清空 redo 栈
  └─ emit "document_changed"
```

- **撤销 = 快照恢复**:`HistoryService::undo()` 取 undo 栈顶快照,替换当前 Document(把当前压入 redo 栈)。快照是 `Arc<Document>` 级 COW,未修改的瓦片零拷贝共享,内存 ∝ 修改区域。
- 图层 / 选区 / 颜色等文档级服务**不直接持有文档锁**,全部通过注入的 `DataService` 操作(读 `document()`,写 `apply_mutation`)。
- `DataService::restore(snapshot)` 是内部恢复通道(History 专用),其他服务不得调用。

## 3. 模块布局

```
kaleido-traits/src/services/     ← 契约(已预写,见各文件)
  mod.rs      ServiceError / ServiceResult + 模块声明
  data.rs     DataService
  history.rs  HistoryService + DocumentCommand
  layer.rs    LayerService
  selection.rs SelectionService
  color.rs    ColorService
  render.rs   RenderService
  app.rs      AppService
  resource.rs ResourceService
  shortcut.rs ShortcutService
  ui.rs       UiService
  task.rs     TaskService
  (plugin 契约在 kaleido-services/src/plugin_service,不重复)

kaleido-services/src/managers/    ← 实现(子代理填充)
  mod.rs      模块声明 + re-export(已预写)
  data.rs history.rs layer.rs selection.rs color.rs render.rs
  app.rs resource.rs shortcut.rs ui.rs task.rs
  (plugin 实现已在 plugin_service/)
```

## 4. 编码约定

- 每个实现模块导出 `pub fn plugin() -> cordis::PluginHandle`,用 `cordis::service_sync::<T, (), _>(id, Inject::new([...]), |ctx, _| ...)`,参考 `plugin_service::plugin_service_plugin()`。
- 实现结构:`Arc<RwLock<State>>` 内部状态 + `Context` 事件 + 注入的依赖;实现 `cordis::Service`(NAME = service id)。
- 错误统一 `kaleido_traits::services::ServiceError` / `ServiceResult<T>`。
- 事件:复用现有常量(`image_loaded` 等)或新增 `document_*` 常量于 `kaleido_traits::events`;数据流事件用 `document_changed`(`DataService` 发出)。
- 测试:每个实现模块内 `#[cfg(test)] mod tests`,不依赖真实文件系统(临时目录用 `std::env::temp_dir`)。

## 5. 各服务契约要点(完整签名见 traits/services/*.rs)

| 服务 | 关键方法 |
|---|---|
| Data | `new_document(name,w,h)` `open(path)` `save()` `save_as(path)` `close()` `document() -> Option<Document>` `apply_mutation(label,f)` `restore(doc)` `has_document()` `path()` `size()` |
| History | `undo()` `redo()` `can_undo()` `can_redo()` `clear()` `undo_depth()` `redo_depth()` |
| Layer | `add_pixel_layer(name,w,h,fmt)` `add_group(name)` `remove(id)` `rename(id,name)` `reorder(child,to_idx)` `set_visible(id,b)` `set_opacity(id,f32)` `set_blend(id,BlendMode)` `children(id)` `node(id)` |
| Selection | `selection() -> Option<SelectionMask>` `set(Option<SelectionMask>)` `clear(w,h)` `invert(w,h)` `union/intersect/subtract(other,w,h)` |
| Color | `profile()` `set_profile(p)` `swatches() -> Vec<Color>` `add_swatch(Color)` `remove_swatch(i)` |
| Render | `render() -> TiledImage`(scene 像素层自底向上合成,blend+opacity;transform/mask/effects 标记 TODO) `render_node(id)` `export_flattened()` |
| App | `name()` `version()` `set_mode(m)` `current_mode()` `notify(msg)` |
| Resource | `register(kind,data) -> ResourceId` `get(id)` `remove(id)` `list(kind)` `ResourceKind::{Font,Swatch,Brush}` `ResourceData` |
| Shortcut | `register_global/mode/plugin(binding)` `unregister(action)` `resolve(key)` `key_for(action)`(委托已有 ShortcutRegistry) |
| UI | `notify(msg)` `set_status(text)` `status()` `register_panel(panel)` `panels()`(委托 PanelRegistry) |
| Task | `spawn(name, f: Box<dyn FnOnce() -> () + Send>) -> TaskId` `progress(id,pct)` `status(id) -> TaskStatus` `cancel(id)` `join(id)` `TaskStatus::{Pending,Running{f32},Done,Cancelled,Failed}` |

## 6. 集成

- `KaleidoApp::boot(AppConfig)`(crates/kaleido-services/src/app/mod.rs,已恢复编译)按依赖顺序安装:
  1. **旧服务**(旧桌面端/CLI 仍在用,保留暂不删除):`tool_registry` → `panel_registry` → `wasm_plugin_manager` → `file_codec` → `file_codec_registry` → `image_store` → `history_keeper` → `ai_agent` → `layer_store`;
  2. **12 管理器**:`data_service` → `history_service` → `layer_service` → `selection_service` → `color_service` → `render_service` → `plugin_service` → `app_service` → `resource_service` → `shortcut_service` → `ui_service` → `task_service`。
- 注意:`data_service` 由 `managers::data::plugin()` 提供(旧的 `data_service_plugin` 不参与 boot,二者 service id 相同,避免双注册)。
- 解析方式:`ctx.require::<ConcreteImpl>(id)` 后强转为 trait 对象(`Arc<dyn XxxService>`);`plugin_service` 通过 `plugin_service::resolve_plugin_service(&ctx)`(该 helper 已修正:原来用 `ctx.get::<Arc<dyn PluginService>>`,运行期 TypeId 不匹配必然失败,现改为 `require::<PluginServiceImpl>` + 强转)。
- `AppConfig` 新增 `mode: String`(默认 `"pixel"`),boot 时写入 app manager。
- `KaleidoApp` 暴露全部访问器:`data_service()` / `history_service()` / `layer_service()` / `selection_service()` / `color_service()` / `render_service()` / `plugin_service()` / `app_service()` / `resource_service()` / `shortcut_service()` / `ui_service()` / `task_service()`,旧访问器(`image_store()` 等)不变。
- 测试:`app::tests` 含 12 服务解析、跨管理器端到端(new_document → add_pixel_layer → selection → undo/redo → render 尺寸)、task spawn/join、mode 配置等;`managers::task::tests` 7 项全部通过。

## 7. 实现状态(2026-08-31 全部落地)

- **12 管理器全部实现并接入 KaleidoApp**(契约:`kaleido-traits/src/services/`,实现:`kaleido-services/src/managers/`,插件服务在 `plugin_service/`),各实现模块带单元测试。
- **去重清理**:
  1. `BlendMode` 三重定义统一为 `kaleido-core::types::BlendMode`(16 变体 + serde + Default);`kaleido-traits::layer::BlendMode` 改为 re-export;`blend::blend` 对 4 个 HSL 变体回退 Normal;`managers/render.rs`、`managers/data.rs` 的跨枚举转换器删除。
  2. `kaleido-plugin-host` crate 合并进 `kaleido-services/src/plugin_service/{host,wasm_host}.rs`,原 crate 删除(workspace 成员、依赖同步移除)。
  3. 死文件 `cordis_bus/` 删除(零引用、未声明)。
- **旧模型服务**(`image_store`/`history_keeper`/`layer_store`,基于旧 TiledImage 模型)保留:旧桌面端/CLI/ai_agent/interactive_tool 仍在使用,待桌面端迁移到新模型后可移除。
- 验证:`cargo check --workspace` 0 error;`cargo test --workspace` 全绿(core 62 + services 267 + traits 12 + sdk 3 + plugins 17)。
