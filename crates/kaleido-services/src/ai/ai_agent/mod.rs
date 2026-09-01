//! AI Agent service implementation — template-based planner + executor.
//!
//! The agent turns a natural-language goal into an ordered list of tool
//! operations ([`AIAgent::plan`]) and then runs them against the current
//! image through the image store's single write path
//! ([`AIAgent::execute_plan`]):
//!
//! ```text
//! goal ──► keyword-template match ──► Plan ──► per-step apply_mutation ──► PlanResult
//!              │                                │
//!              └── fallback (tool-name match)   └── fail-fast on first error
//! ```
//!
//! # Planning templates
//!
//! The MVP planner lower-cases the goal and scans a fixed list of keyword
//! templates (vintage, brighten, grayscale, brightness up/down, invert,
//! blur, sharpen, auto-enhance). A template fires on the **first** keyword
//! hit in template order, so a goal like `"复古胶片感再提亮"` plans only the
//! vintage effect — multi-intent goals are a known MVP limitation (see
//! [Migration / future work](#migration--future-work)).
//!
//! Every template checks tool availability before adding an action and
//! merges the template params with the tool's declared schema defaults, so
//! tools that need extra parameters still receive them. A template that
//! cannot produce any action fails with [`AgentError::PlanningFailed`].
//!
//! When no template matches, planning falls back to matching goal words
//! against registered tool names/descriptions, and finally to the first
//! registered tool with its schema defaults.
//!
//! # Execution & error semantics
//!
//! - Empty plans → [`AgentError::EmptyPlan`].
//! - Plans longer than [`MAX_PLAN_STEPS`] → [`AgentError::MaxStepsExceeded`].
//! - No image loaded → [`AgentError::NoImageLoaded`].
//! - A missing tool → [`AgentError::ToolNotFound`] (defense in depth — the
//!   planner only produces plans referencing live tools).
//! - A failing tool application → [`AgentError::ExecutionFailed`]; execution
//!   **stops** at the first failing step (fail-fast). Per-step outcomes are
//!   observable through the `ai_action_executed` events; a partial
//!   [`PlanResult`] is not returned in MVP.
//!
//! # Events
//!
//! - `ai_thinking` — emitted at the start of [`AIAgent::plan`].
//! - `ai_action_executed` — emitted after every attempted tool application.
//!
//! # Stats
//!
//! [`AIAgent::stats`] exposes the [`AgentStats`] counters (plans created /
//! executed, actions executed / failed). `tools_generated` stays `0` in MVP:
//! tool generation ([`kaleido_traits::ToolGenerationRequest`]) is future work.
//!
//! # Migration / future work
//!
//! The executor depends on the legacy `image_store` service
//! ([`ImageStoreImpl`], old `TiledImage` model) — one of the three legacy
//! services kept for the old desktop / CLI hosts. Path to the new document
//! model ([`kaleido_traits::services::data::DataService`]):
//!
//! 1. Replace the `image_store: Arc<dyn ImageStore>` field with
//!    `Arc<dyn DataService>` (constructor signature change, coordinated with
//!    `cordis_plugins::ai_agent_plugin` and `kaleido_app::boot`).
//! 2. The pre-flight check becomes `data_service.has_document()` and each
//!    tool application becomes `data_service.apply_mutation(label, …)` on
//!    the document's active pixel layer instead of the single `TiledImage`.
//! 3. `plan()`'s `context` JSON (`current_image_size`, `available_tools` —
//!    currently accepted but ignored) can then be populated from the
//!    document model, and an LLM planner can replace the template matcher
//!    without touching the [`AIAgent`] trait.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use cordis::{Context, Service};
use kaleido_core::TiledImage;
use kaleido_traits::{
    AIAgent, AgentError, AgentMode, AgentResult, AgentStats, AiActionExecutedEvent,
    AiThinkingEvent, ImageStore, KaleidoEmitter, Plan, PlanResult, Tool, ToolParams, ToolRegistry,
};

use crate::services::data::legacy::image_store_impl::ImageStoreImpl;

// ---------------------------------------------------------------------------
// Planning templates
// ---------------------------------------------------------------------------

/// A planning template: matches keywords and produces a plan.
struct PlanningTemplate {
    /// Keywords that trigger this template (any match, case-insensitive).
    keywords: Vec<&'static str>,
    /// Template name (used for logging).
    name: &'static str,
    /// The plan builder function.
    build: fn(&str, &dyn ToolRegistry) -> AgentResult<Plan>,
}

/// Merges template params with the tool's declared schema defaults.
///
/// The templates hard-code sensible values for the built-in tools; applying
/// the schema defaults guarantees that tools declaring additional required
/// parameters still receive them (template values win on conflicts).
fn tool_params(tool: &dyn Tool, params: serde_json::Value) -> ToolParams {
    tool.schema().apply_defaults(&params)
}

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

fn template_vintage(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("复古胶片效果");
    if let Some(tool) = registry.get("brightness") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"brightness": 5.0, "saturation": -20.0}));
        plan = plan.with_action("brightness", params, "降低饱和度，轻微提亮");
    }
    if let Some(tool) = registry.get("color_temperature") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"temperature": 15.0}));
        plan = plan.with_action("color_temperature", params, "添加暖色调");
    }
    if let Some(tool) = registry.get("film_grain") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"intensity": 0.15}));
        plan = plan.with_action("film_grain", params, "添加胶片颗粒感");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的工具来实现复古效果".to_string(),
        });
    }
    Ok(plan)
}

fn template_brighten(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("美白提亮");
    if let Some(tool) = registry.get("brightness") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"brightness": 15.0}));
        plan = plan.with_action("brightness", params, "提亮整体画面");
    }
    if let Some(tool) = registry.get("saturation") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"saturation": -10.0}));
        plan = plan.with_action("saturation", params, "轻微降饱和，让肤色更自然");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的工具来实现美白效果".to_string(),
        });
    }
    Ok(plan)
}

fn template_grayscale(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("黑白效果");
    if let Some(tool) = registry.get("saturation") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"saturation": -100.0}));
        plan = plan.with_action("saturation", params, "完全去除饱和度");
    } else if let Some(tool) = registry.get("brightness") {
        // No saturation tool: drive the same visual effect through the
        // brightness tool's *own* parameter (not `saturation`).
        let params = tool_params(tool.as_ref(), serde_json::json!({"brightness": -100.0}));
        plan = plan.with_action("brightness", params, "通过亮度工具实现去饱和");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的工具来实现黑白效果".to_string(),
        });
    }
    Ok(plan)
}

fn template_brightness_up(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("增加亮度");
    if let Some(tool) = registry.get("brightness") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"brightness": 20.0}));
        plan = plan.with_action("brightness", params, "增加亮度");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的亮度工具".to_string(),
        });
    }
    Ok(plan)
}

fn template_brightness_down(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("降低亮度");
    if let Some(tool) = registry.get("brightness") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"brightness": -20.0}));
        plan = plan.with_action("brightness", params, "降低亮度");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的亮度工具".to_string(),
        });
    }
    Ok(plan)
}

fn template_invert(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("颜色反相");
    if let Some(tool) = registry.get("invert") {
        let params = tool_params(tool.as_ref(), serde_json::json!({}));
        plan = plan.with_action("invert", params, "反转所有颜色");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的反相工具".to_string(),
        });
    }
    Ok(plan)
}

fn template_blur(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("模糊效果");
    if let Some(tool) = registry.get("blur") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"radius": 2.0}));
        plan = plan.with_action("blur", params, "应用高斯模糊");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的模糊工具".to_string(),
        });
    }
    Ok(plan)
}

fn template_sharpen(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("锐化效果");
    if let Some(tool) = registry.get("sharpen") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"amount": 0.5}));
        plan = plan.with_action("sharpen", params, "应用锐化");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的锐化工具".to_string(),
        });
    }
    Ok(plan)
}

fn template_auto_enhance(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("自动增强");
    if let Some(tool) = registry.get("brightness") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"brightness": 8.0, "contrast": 5.0}));
        plan = plan.with_action("brightness", params, "微调亮度和对比度");
    }
    if let Some(tool) = registry.get("saturation") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"saturation": 10.0}));
        plan = plan.with_action("saturation", params, "轻微增加饱和度");
    }
    if let Some(tool) = registry.get("sharpen") {
        let params = tool_params(tool.as_ref(), serde_json::json!({"amount": 0.2}));
        plan = plan.with_action("sharpen", params, "轻微锐化增加细节");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的工具来实现自动增强".to_string(),
        });
    }
    Ok(plan)
}

/// Fallback planner: matches goal words against tool names/descriptions,
/// then falls back to the first registered tool (with schema defaults).
fn template_fallback(goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let tools = registry.tools();
    if tools.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "工具注册表为空，没有任何可用的编辑工具".to_string(),
        });
    }

    let goal_lower = goal.to_lowercase();
    for tool in &tools {
        let name_lower = tool.name().to_lowercase();
        let desc_lower = tool.description().to_lowercase();
        for word in goal_lower.split_whitespace() {
            if name_lower.contains(word) || desc_lower.contains(word) {
                let params = tool.schema().apply_defaults(&serde_json::json!({}));
                return Ok(Plan::new(goal).with_action(tool.name(), params, &tool.description()));
            }
        }
    }

    // No word matched any tool: use the first registered tool with defaults
    // (`apply_defaults` on a parameter-less schema yields an empty object).
    let tool = &tools[0];
    let params = tool.schema().apply_defaults(&serde_json::json!({}));
    Ok(Plan::new(goal).with_action(tool.name(), params, &tool.description()))
}

fn planning_templates() -> Vec<PlanningTemplate> {
    vec![
        PlanningTemplate {
            keywords: vec!["复古", "vintage", "胶片", "film", "怀旧", "nostalgia"],
            name: "vintage",
            build: template_vintage,
        },
        PlanningTemplate {
            keywords: vec!["美白", "brighten skin", "亮肤"],
            name: "brighten",
            build: template_brighten,
        },
        PlanningTemplate {
            keywords: vec![
                "黑白",
                "灰度",
                "black and white",
                "grayscale",
                "b&w",
                "单色",
                "monochrome",
            ],
            name: "grayscale",
            build: template_grayscale,
        },
        PlanningTemplate {
            keywords: vec!["亮一点", "增亮", "变亮", "更亮", "brighten"],
            name: "brightness_up",
            build: template_brightness_up,
        },
        PlanningTemplate {
            keywords: vec!["暗一点", "变暗", "更暗", "darken"],
            name: "brightness_down",
            build: template_brightness_down,
        },
        PlanningTemplate {
            keywords: vec!["反相", "反转", "invert", "负片"],
            name: "invert",
            build: template_invert,
        },
        PlanningTemplate {
            keywords: vec!["模糊", "blur", "柔化", "soften"],
            name: "blur",
            build: template_blur,
        },
        PlanningTemplate {
            keywords: vec!["锐化", "sharpen", "清晰"],
            name: "sharpen",
            build: template_sharpen,
        },
        PlanningTemplate {
            keywords: vec!["自动", "auto", "一键", "智能", "smart", "优化", "enhance"],
            name: "auto_enhance",
            build: template_auto_enhance,
        },
    ]
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

struct AgentStatsInternal {
    plans_created: AtomicU64,
    plans_executed: AtomicU64,
    actions_executed: AtomicU64,
    actions_failed: AtomicU64,
    tools_generated: AtomicU64,
}

// ---------------------------------------------------------------------------
// AIAgentImpl
// ---------------------------------------------------------------------------

/// Safety limit: the maximum number of actions a single plan may contain.
const MAX_PLAN_STEPS: usize = 20;

/// AI Agent service implementation.
pub struct AIAgentImpl {
    /// Tool registry — source of available tools.
    tool_registry: Arc<dyn ToolRegistry>,
    /// Image store — the single write path the agent executes tools through.
    ///
    /// Held as the trait object even though [`AIAgentImpl::new`] accepts the
    /// legacy [`ImageStoreImpl`]. This is the old-model dependency: the
    /// migration to the new document model swaps this field for
    /// `Arc<dyn DataService>` (see the module docs, "Migration / future
    /// work").
    image_store: Arc<dyn ImageStore>,
    /// Cordis context — for event emission.
    ctx: Context,
    /// Agent statistics.
    stats: AgentStatsInternal,
}

impl Service for AIAgentImpl {
    const NAME: &'static str = "ai_agent";
}

impl AIAgentImpl {
    /// Creates a new [`AIAgentImpl`] with the given dependencies.
    ///
    /// `image_store` is the legacy image store service; it is coerced to the
    /// [`ImageStore`] trait object internally (see the migration note in the
    /// module docs).
    pub fn new(
        tool_registry: Arc<dyn ToolRegistry>,
        image_store: Arc<ImageStoreImpl>,
        ctx: Context,
    ) -> Self {
        let image_store: Arc<dyn ImageStore> = image_store;
        Self {
            tool_registry,
            image_store,
            ctx,
            stats: AgentStatsInternal {
                plans_created: AtomicU64::new(0),
                plans_executed: AtomicU64::new(0),
                actions_executed: AtomicU64::new(0),
                actions_failed: AtomicU64::new(0),
                tools_generated: AtomicU64::new(0),
            },
        }
    }
}

impl AIAgent for AIAgentImpl {
    fn plan(&self, goal: &str, context: Option<&serde_json::Value>) -> AgentResult<Plan> {
        if goal.trim().is_empty() {
            return Err(AgentError::PlanningFailed {
                reason: "目标为空：请描述想要的效果".to_string(),
            });
        }

        self.stats.plans_created.fetch_add(1, Ordering::Relaxed);

        self.ctx.emit_ai_thinking(AiThinkingEvent {
            prompt: goal.to_string(),
        });

        // The `context` JSON (`current_image_size` / `available_tools`) is
        // reserved for LLM-driven planning; the template planner consults
        // the live registry directly, which is the MVP equivalent of the
        // auto-populated `available_tools`. See the module docs.
        if context.is_some() {
            tracing::debug!("plan(): context provided — ignored by template planner");
        }

        let goal_lower = goal.to_lowercase();
        let templates = planning_templates();
        for template in &templates {
            for keyword in &template.keywords {
                // All template keywords are pre-lower-cased; the goal is
                // lower-cased once above.
                if goal_lower.contains(keyword) {
                    tracing::debug!(
                        "plan(): template '{}' matched keyword '{keyword}'",
                        template.name
                    );
                    return (template.build)(goal, &*self.tool_registry);
                }
            }
        }

        tracing::debug!("plan(): no template matched — falling back to tool matching");
        template_fallback(goal, &*self.tool_registry)
    }

    fn execute_plan(&self, plan: &Plan) -> AgentResult<PlanResult> {
        self.stats.plans_executed.fetch_add(1, Ordering::Relaxed);

        if plan.is_empty() {
            return Err(AgentError::EmptyPlan);
        }
        if plan.len() > MAX_PLAN_STEPS {
            return Err(AgentError::MaxStepsExceeded {
                max: MAX_PLAN_STEPS,
            });
        }
        if !self.image_store.has_image() {
            return Err(AgentError::NoImageLoaded);
        }

        let mut results = Vec::with_capacity(plan.actions.len());

        for (step, action) in plan.actions.iter().enumerate() {
            let start = Instant::now();

            let tool = self.tool_registry.get(&action.tool_name).ok_or_else(|| {
                AgentError::ToolNotFound {
                    tool_name: action.tool_name.clone(),
                }
            })?;

            let tool_name = action.tool_name.clone();
            let params = action.params.clone();
            let params_json = params.to_string();

            // Apply the tool through the image store's single write path.
            // The closure captures `tool` and `params` (both `'static`), so
            // the boxed mutator can run inside the store's lock.
            let apply_result =
                self.image_store
                    .apply_mutation(Box::new(move |image: &mut TiledImage| {
                        tool.apply(image, &params)
                    }));

            let duration = start.elapsed();
            let duration_ms = duration.as_millis() as u64;

            match apply_result {
                Ok(()) => {
                    self.stats.actions_executed.fetch_add(1, Ordering::Relaxed);

                    self.ctx.emit_ai_action_executed(AiActionExecutedEvent {
                        tool: tool_name.clone(),
                        params: params_json,
                        duration_ms,
                    });

                    results.push(kaleido_traits::ActionResult {
                        step,
                        tool_name: tool_name.clone(),
                        success: true,
                        error: None,
                        duration_ms,
                    });

                    tracing::info!(
                        "AI agent step {}/{}: {} completed in {:?}",
                        step + 1,
                        plan.actions.len(),
                        tool_name,
                        duration
                    );
                }
                Err(e) => {
                    self.stats.actions_failed.fetch_add(1, Ordering::Relaxed);

                    let error_msg = format!("Tool '{}' failed: {}", tool_name, e);

                    self.ctx.emit_ai_action_executed(AiActionExecutedEvent {
                        tool: tool_name.clone(),
                        params: params_json,
                        duration_ms,
                    });

                    results.push(kaleido_traits::ActionResult {
                        step,
                        tool_name,
                        success: false,
                        error: Some(error_msg.clone()),
                        duration_ms,
                    });

                    // Fail-fast: execution stops at the first failing step.
                    // The partial `results` are not returned in MVP —
                    // per-step outcomes are observable through the
                    // `ai_action_executed` events (`PlanResult.success` is
                    // reserved for a future partial-result mode).
                    return Err(AgentError::ExecutionFailed {
                        step,
                        reason: error_msg,
                    });
                }
            }
        }

        Ok(PlanResult {
            plan: plan.clone(),
            action_results: results,
            success: true,
        })
    }

    fn mode(&self) -> AgentMode {
        AgentMode::Template
    }

    fn stats(&self) -> AgentStats {
        AgentStats {
            plans_created: self.stats.plans_created.load(Ordering::Relaxed),
            plans_executed: self.stats.plans_executed.load(Ordering::Relaxed),
            actions_executed: self.stats.actions_executed.load(Ordering::Relaxed),
            actions_failed: self.stats.actions_failed.load(Ordering::Relaxed),
            tools_generated: self.stats.tools_generated.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{ImageError, Pixel, PixelFormat};
    use kaleido_traits::{AI_ACTION_EXECUTED, AI_THINKING, ParamSchema, ParamType, ToolSchema};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct GrayFillTool;

    impl kaleido_traits::Tool for GrayFillTool {
        fn name(&self) -> &str {
            "gray_fill"
        }
        fn menu_path(&self) -> String {
            "测试/灰色填充".into()
        }
        fn description(&self) -> String {
            "Fill the image with gray color".into()
        }
        fn apply(&self, image: &mut TiledImage, _params: &ToolParams) -> kaleido_core::ImageResult<()> {
            image.fill(Pixel::rgb(128, 128, 128));
            Ok(())
        }
    }

    struct BrightnessTool;

    impl kaleido_traits::Tool for BrightnessTool {
        fn name(&self) -> &str {
            "brightness"
        }
        fn menu_path(&self) -> String {
            "调整/亮度".into()
        }
        fn description(&self) -> String {
            "Adjust image brightness".into()
        }
        fn schema(&self) -> kaleido_traits::ToolSchema {
            ToolSchema::new("brightness", "亮度", "Adjust brightness").with_param(
                ParamSchema::new("brightness", ParamType::Float)
                    .with_default(serde_json::json!(10.0))
                    .required(),
            )
        }
        fn apply(&self, image: &mut TiledImage, params: &ToolParams) -> kaleido_core::ImageResult<()> {
            let amount = params
                .get("brightness")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            image.fill(Pixel::rgb(
                (128.0 + amount) as u8,
                (128.0 + amount) as u8,
                (128.0 + amount) as u8,
            ));
            Ok(())
        }
    }

    /// A tool whose `apply` always fails — used to exercise the fail-fast
    /// execution path.
    struct FailingTool;

    impl kaleido_traits::Tool for FailingTool {
        fn name(&self) -> &str {
            "failing_tool"
        }
        fn menu_path(&self) -> String {
            "测试/失败工具".into()
        }
        fn description(&self) -> String {
            "Always fails".into()
        }
        fn apply(&self, _image: &mut TiledImage, _params: &ToolParams) -> kaleido_core::ImageResult<()> {
            Err(ImageError::OperationFailed {
                reason: "boom".into(),
            })
        }
    }

    /// Returns the registry, image store, agent, the strong tool
    /// references, and the context.
    ///
    /// The tool `Arc`s must be returned (and kept alive by the caller):
    /// `ToolRegistry` only holds `Weak<dyn Tool>`, so dropping them here
    /// would leave the registry effectively empty. The context is returned
    /// so tests can attach event listeners before driving the agent.
    fn setup_test_env() -> (
        Arc<dyn ToolRegistry>,
        Arc<ImageStoreImpl>,
        Arc<AIAgentImpl>,
        Vec<Arc<dyn kaleido_traits::Tool>>,
        Context,
    ) {
        let ctx = Context::new();
        let registry: Arc<dyn ToolRegistry> =
            Arc::new(crate::services::plugin::tool_registry::ToolRegistryImpl::new());
        let codec: Arc<dyn kaleido_traits::FileCodec> =
            Arc::new(crate::services::data::legacy::file_codec_impl::FileCodecImpl::new());
        let image_store: Arc<ImageStoreImpl> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));

        let gray_tool: Arc<dyn kaleido_traits::Tool> = Arc::new(GrayFillTool);
        registry.register(Arc::downgrade(&gray_tool));

        let bright_tool: Arc<dyn kaleido_traits::Tool> = Arc::new(BrightnessTool);
        registry.register(Arc::downgrade(&bright_tool));

        let agent = Arc::new(AIAgentImpl::new(registry.clone(), image_store.clone(), ctx.clone()));

        (registry, image_store, agent, vec![gray_tool, bright_tool], ctx)
    }

    /// Sets a small image in the store (helper for execution tests).
    fn set_image(store: &ImageStoreImpl, value: u8) {
        let img = TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(value, value, value))
            .unwrap();
        store.set_image(img).unwrap();
    }

    // ─── Planning: template hits ──────────────────────────────────────────

    #[test]
    fn test_plan_brightness_up_template() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        let plan = agent.plan("把图片亮一点", None);
        assert!(plan.is_ok(), "Should create a plan for brightness");
        let plan = plan.unwrap();
        assert!(!plan.is_empty());
        assert_eq!(plan.actions[0].tool_name, "brightness");
    }

    #[test]
    fn test_plan_brighten_template() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        let plan = agent.plan("美白皮肤", None).unwrap();
        assert_eq!(plan.actions.len(), 1, "only brightness is registered");
        assert_eq!(plan.actions[0].tool_name, "brightness");
    }

    #[test]
    fn test_plan_template_vintage_hit() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        let plan = agent.plan("给照片添加复古胶片效果", None).unwrap();
        assert_eq!(plan.actions.len(), 1, "only brightness is registered");
        assert_eq!(plan.actions[0].tool_name, "brightness");
    }

    #[test]
    fn test_plan_template_grayscale_falls_back_to_brightness() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        // No `saturation` tool is registered → the grayscale template falls
        // back to the brightness tool and must drive *its own* parameter.
        let plan = agent.plan("黑白", None).unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].tool_name, "brightness");
        assert_eq!(
            plan.actions[0]
                .params
                .get("brightness")
                .and_then(|v| v.as_f64()),
            Some(-100.0),
            "fallback must target the brightness parameter, not saturation"
        );
    }

    #[test]
    fn test_plan_template_with_missing_tools_fails() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        // "反相" hits the invert template, but no invert tool is registered.
        assert!(matches!(
            agent.plan("反相", None),
            Err(AgentError::PlanningFailed { .. })
        ));
    }

    // ─── Planning: fallback & validation ──────────────────────────────────

    #[test]
    fn test_plan_fallback_to_tool_name() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        let plan = agent.plan("gray_fill", None);
        assert!(plan.is_ok(), "Fallback should match tool name");
        let plan = plan.unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].tool_name, "gray_fill");
    }

    #[test]
    fn test_plan_unknown_goal_falls_back_to_tool_defaults() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        // No template keyword matches → the fallback plans the first
        // registered tool (gray_fill) with its schema defaults.
        let plan = agent.plan("把天空变成紫色", None).unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].tool_name, "gray_fill");
    }

    #[test]
    fn test_plan_empty_goal_fails() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        for goal in ["", "   ", "\n\t"] {
            assert!(
                matches!(agent.plan(goal, None), Err(AgentError::PlanningFailed { .. })),
                "goal {goal:?} should be rejected"
            );
        }
    }

    #[test]
    fn test_plan_empty_registry() {
        let ctx = Context::new();
        let registry: Arc<dyn ToolRegistry> =
            Arc::new(crate::services::plugin::tool_registry::ToolRegistryImpl::new());
        let codec: Arc<dyn kaleido_traits::FileCodec> =
            Arc::new(crate::services::data::legacy::file_codec_impl::FileCodecImpl::new());
        let image_store: Arc<ImageStoreImpl> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));
        let agent = AIAgentImpl::new(registry, image_store, ctx);
        let result = agent.plan("复古效果", None);
        assert!(result.is_err(), "Should fail with empty registry");
    }

    #[test]
    fn test_plan_unknown_goal_empty_registry_fails() {
        let ctx = Context::new();
        let registry: Arc<dyn ToolRegistry> =
            Arc::new(crate::services::plugin::tool_registry::ToolRegistryImpl::new());
        let codec: Arc<dyn kaleido_traits::FileCodec> =
            Arc::new(crate::services::data::legacy::file_codec_impl::FileCodecImpl::new());
        let image_store: Arc<ImageStoreImpl> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));
        let agent = AIAgentImpl::new(registry, image_store, ctx);
        assert!(matches!(
            agent.plan("把天空变成紫色", None),
            Err(AgentError::PlanningFailed { .. })
        ));
    }

    #[test]
    fn test_plan_accepts_context_json() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        // The `context` JSON is reserved (not consumed by the template
        // planner) but must be accepted gracefully.
        let context = serde_json::json!({
            "current_image_size": [800, 600],
            "available_tools": ["brightness"],
        });
        assert!(agent.plan("亮一点", Some(&context)).is_ok());
    }

    // ─── Events ───────────────────────────────────────────────────────────

    #[test]
    fn test_plan_emits_thinking_event() {
        let (_registry, _store, agent, _tools, ctx) = setup_test_env();
        let emitted = Arc::new(AtomicUsize::new(0));
        let count = emitted.clone();
        ctx.on(AI_THINKING, move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .unwrap();

        let _ = agent.plan("亮一点", None);
        assert_eq!(emitted.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_execute_plan_emits_action_events() {
        let (_registry, store, agent, _tools, ctx) = setup_test_env();
        set_image(&store, 100);

        let emitted = Arc::new(AtomicUsize::new(0));
        let count = emitted.clone();
        ctx.on(AI_ACTION_EXECUTED, move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .unwrap();

        let plan = Plan::new("test").with_action("gray_fill", serde_json::json!({}), "Fill gray");
        agent.execute_plan(&plan).unwrap();
        assert_eq!(emitted.load(Ordering::SeqCst), 1);
    }

    // ─── Execution ────────────────────────────────────────────────────────

    #[test]
    fn test_execute_plan_empty() {
        let (_registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 100);
        let empty_plan = Plan::new("empty");
        let result = agent.execute_plan(&empty_plan);
        assert!(matches!(result, Err(AgentError::EmptyPlan)));
    }

    #[test]
    fn test_execute_plan_no_image() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        let plan = Plan::new("test").with_action("gray_fill", serde_json::json!({}), "Fill gray");
        let result = agent.execute_plan(&plan);
        assert!(matches!(result, Err(AgentError::NoImageLoaded)));
    }

    #[test]
    fn test_execute_plan_success() {
        let (_registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 100);
        let plan = Plan::new("test").with_action("gray_fill", serde_json::json!({}), "Fill with gray");
        let result = agent.execute_plan(&plan);
        assert!(result.is_ok(), "Plan execution should succeed");
        let plan_result = result.unwrap();
        assert!(plan_result.success);
        assert_eq!(plan_result.action_results.len(), 1);
    }

    #[test]
    fn test_execute_plan_tool_not_found() {
        let (_registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 100);
        let plan = Plan::new("test").with_action("no_such_tool", serde_json::json!({}), "nope");
        assert!(matches!(
            agent.execute_plan(&plan),
            Err(AgentError::ToolNotFound { ref tool_name }) if tool_name == "no_such_tool"
        ));
    }

    #[test]
    fn test_execute_plan_failing_tool_stops_and_counts_failure() {
        let (registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 100);

        // Register a tool that always fails; keep the strong Arc alive so
        // the registry's weak pointer stays valid.
        let failing: Arc<dyn kaleido_traits::Tool> = Arc::new(FailingTool);
        registry.register(Arc::downgrade(&failing));

        let plan = Plan::new("test")
            .with_action("gray_fill", serde_json::json!({}), "ok step")
            .with_action("failing_tool", serde_json::json!({}), "fails");
        assert!(matches!(
            agent.execute_plan(&plan),
            Err(AgentError::ExecutionFailed { step: 1, .. })
        ));

        // The first step succeeded, the second failed.
        let stats = agent.stats();
        assert_eq!(stats.actions_executed, 1);
        assert_eq!(stats.actions_failed, 1);
    }

    #[test]
    fn test_run_plan_and_execute() {
        let (_registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 50);
        let result = agent.run("亮一点", None);
        assert!(result.is_ok(), "run() should plan and execute");
        let plan_result = result.unwrap();
        assert!(plan_result.success);
    }

    // ─── Stats & mode ─────────────────────────────────────────────────────

    #[test]
    fn test_stats_tracking() {
        let (_registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 50);
        let _ = agent.run("gray_fill", None);
        let stats = agent.stats();
        assert!(stats.plans_created >= 1);
        assert!(stats.plans_executed >= 1);
        assert!(stats.actions_executed >= 1);
    }

    #[test]
    fn test_mode_is_template() {
        let (_registry, _store, agent, _tools, _ctx) = setup_test_env();
        assert_eq!(agent.mode(), AgentMode::Template);
    }

    #[test]
    fn test_max_steps_exceeded() {
        let (_registry, store, agent, _tools, _ctx) = setup_test_env();
        set_image(&store, 50);
        let mut plan = Plan::new("too many steps");
        for _ in 0..25 {
            plan = plan.with_action("gray_fill", serde_json::json!({}), "fill");
        }
        let result = agent.execute_plan(&plan);
        assert!(matches!(
            result,
            Err(AgentError::MaxStepsExceeded { max: 20 })
        ));
    }
}
