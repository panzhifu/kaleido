//! AI Agent service implementation — template-based planner + executor.
//!
//! The agent plans image editing operations using simple keyword templates
//! and executes them against the current image through the ToolRegistry
//!

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use cordis::{Context, Service};
use kaleido_core::Image;
use kaleido_traits::ImageStore;
use kaleido_traits::{
    AIAgent, AgentError, AgentMode, AgentResult, AgentStats, AiActionExecutedEvent,
    KaleidoEmitter, Plan, PlanResult, ToolRegistry,
};

use crate::image_store_impl::ImageStoreImpl;

// ---------------------------------------------------------------------------
// Planning templates
// ---------------------------------------------------------------------------

/// A planning template: matches keywords and produces a plan.
struct PlanningTemplate {
    /// Keywords that trigger this template (any match).
    keywords: Vec<&'static str>,
    /// Template name (for logging).
    name: &'static str,
    /// The plan builder function.
    build: fn(&str, &dyn ToolRegistry) -> AgentResult<Plan>,
}

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

fn template_vintage(_goal: &str, registry: &dyn ToolRegistry) -> AgentResult<Plan> {
    let mut plan = Plan::new("复古胶片效果");
    if registry.get("brightness").is_some() {
        let params = serde_json::json!({"brightness": 5.0, "saturation": -20.0});
        plan = plan.with_action("brightness", params, "降低饱和度，轻微提亮");
    }
    if registry.get("color_temperature").is_some() {
        let params = serde_json::json!({"temperature": 15.0});
        plan = plan.with_action("color_temperature", params, "添加暖色调");
    }
    if registry.get("film_grain").is_some() {
        let params = serde_json::json!({"intensity": 0.15});
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
    if registry.get("brightness").is_some() {
        let params = serde_json::json!({"brightness": 15.0});
        plan = plan.with_action("brightness", params, "提亮整体画面");
    }
    if registry.get("saturation").is_some() {
        let params = serde_json::json!({"saturation": -10.0});
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
    if registry.get("saturation").is_some() {
        let params = serde_json::json!({"saturation": -100.0});
        plan = plan.with_action("saturation", params, "完全去除饱和度");
    } else if registry.get("brightness").is_some() {
        let params = serde_json::json!({"saturation": -100.0});
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
    if registry.get("brightness").is_some() {
        let params = serde_json::json!({"brightness": 20.0});
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
    if registry.get("brightness").is_some() {
        let params = serde_json::json!({"brightness": -20.0});
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
    if registry.get("invert").is_some() {
        let params = serde_json::json!({});
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
    if registry.get("blur").is_some() {
        let params = serde_json::json!({"radius": 2.0});
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
    if registry.get("sharpen").is_some() {
        let params = serde_json::json!({"amount": 0.5});
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
    if registry.get("brightness").is_some() {
        let params = serde_json::json!({"brightness": 8.0, "contrast": 5.0});
        plan = plan.with_action("brightness", params, "微调亮度和对比度");
    }
    if registry.get("saturation").is_some() {
        let params = serde_json::json!({"saturation": 10.0});
        plan = plan.with_action("saturation", params, "轻微增加饱和度");
    }
    if registry.get("sharpen").is_some() {
        let params = serde_json::json!({"amount": 0.2});
        plan = plan.with_action("sharpen", params, "轻微锐化增加细节");
    }
    if plan.is_empty() {
        return Err(AgentError::PlanningFailed {
            reason: "没有可用的工具来实现自动增强".to_string(),
        });
    }
    Ok(plan)
}

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
                let schema = tool.schema();
                let params = schema.apply_defaults(&serde_json::json!({}));
                return Ok(Plan::new(goal).with_action(tool.name(), params, &tool.description()));
            }
        }
    }
    let tool = &tools[0];
    let schema = tool.schema();
    if schema.params.is_empty() {
        Ok(Plan::new(goal).with_action(tool.name(), serde_json::json!({}), &tool.description()))
    } else {
        let params = schema.apply_defaults(&serde_json::json!({}));
        Ok(Plan::new(goal).with_action(tool.name(), params, &tool.description()))
    }
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

const MAX_PLAN_STEPS: usize = 20;

/// AI Agent service implementation.
pub struct AIAgentImpl {
    /// Tool registry — source of available tools.
    tool_registry: Arc<dyn ToolRegistry>,
    /// Image store — the target for operations.
    image_store: Arc<ImageStoreImpl>,
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
    pub fn new(
        tool_registry: Arc<dyn ToolRegistry>,
        image_store: Arc<ImageStoreImpl>,
        ctx: Context,
    ) -> Self {
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
    fn plan(&self, goal: &str, _context: Option<&serde_json::Value>) -> AgentResult<Plan> {
        self.stats.plans_created.fetch_add(1, Ordering::Relaxed);

        self.ctx.emit_ai_thinking(kaleido_traits::AiThinkingEvent {
            prompt: goal.to_string(),
        });

        let goal_lower = goal.to_lowercase();
        let templates = planning_templates();
        for template in &templates {
            for keyword in &template.keywords {
                if goal_lower.contains(&keyword.to_lowercase()) {
                    let plan = (template.build)(goal, &*self.tool_registry)?;
                    return Ok(plan);
                }
            }
        }

        let plan = template_fallback(goal, &*self.tool_registry)?;
        Ok(plan)
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
        let mut overall_success = true;

        for (step, action) in plan.actions.iter().enumerate() {
            let start = Instant::now();

            let tool = self.tool_registry.get(&action.tool_name).ok_or_else(|| {
                AgentError::ToolNotFound {
                    tool_name: action.tool_name.clone(),
                }
            })?;

            let tool_name = action.tool_name.clone();
            let params = action.params.clone();
            let params_json = serde_json::to_string(&params).unwrap_or_default();

            let apply_result =
                self.image_store
                    .apply_mutation(Box::new(move |image: &mut Image| {
                        tool.apply(image, &params)
                    }));

            let duration = start.elapsed();

            match apply_result {
                Ok(()) => {
                    self.ctx.emit_ai_action_executed(AiActionExecutedEvent {
                        tool: tool_name.clone(),
                        params: params_json,
                        duration_ms: duration.as_millis() as u64,
                    });

                    self.stats.actions_executed.fetch_add(1, Ordering::Relaxed);

                    results.push(kaleido_traits::ActionResult {
                        step,
                        tool_name: tool_name.clone(),
                        success: true,
                        error: None,
                        duration_ms: duration.as_millis() as u64,
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
                    overall_success = false;

                    let error_msg = format!("Tool '{}' failed: {}", tool_name, e);

                    self.ctx.emit_ai_action_executed(AiActionExecutedEvent {
                        tool: tool_name.clone(),
                        params: params_json,
                        duration_ms: duration.as_millis() as u64,
                    });

                    results.push(kaleido_traits::ActionResult {
                        step,
                        tool_name,
                        success: false,
                        error: Some(error_msg.clone()),
                        duration_ms: duration.as_millis() as u64,
                    });

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
            success: overall_success,
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
    use kaleido_core::{Pixel, PixelFormat};
    use kaleido_traits::ToolParams;

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
        fn apply(&self, image: &mut Image, _params: &ToolParams) -> kaleido_core::ImageResult<()> {
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
            use kaleido_traits::{ParamSchema, ParamType, ToolSchema};
            ToolSchema::new("brightness", "亮度", "Adjust brightness").with_param(
                ParamSchema::new("brightness", ParamType::Float)
                    .with_default(serde_json::json!(10.0))
                    .required(),
            )
        }
        fn apply(&self, image: &mut Image, params: &ToolParams) -> kaleido_core::ImageResult<()> {
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

    fn setup_test_env() -> (Arc<dyn ToolRegistry>, Arc<ImageStoreImpl>, Arc<AIAgentImpl>) {
        let ctx = Context::new();
        let registry: Arc<dyn ToolRegistry> =
            Arc::new(crate::tool_registry::ToolRegistryImpl::new());
        let codec: Arc<dyn kaleido_traits::FileCodec> =
            Arc::new(crate::file_codec_impl::FileCodecImpl::new());
        let image_store: Arc<ImageStoreImpl> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));

        let gray_tool: Arc<dyn kaleido_traits::Tool> = Arc::new(GrayFillTool);
        registry.register(Arc::downgrade(&gray_tool));

        let bright_tool: Arc<dyn kaleido_traits::Tool> = Arc::new(BrightnessTool);
        registry.register(Arc::downgrade(&bright_tool));

        let agent = Arc::new(AIAgentImpl::new(registry.clone(), image_store.clone(), ctx));

        (registry, image_store, agent)
    }

    #[test]
    fn test_plan_brightness_keyword() {
        let (_registry, _store, agent) = setup_test_env();
        let plan = agent.plan("把图片亮一点", None);
        assert!(plan.is_ok(), "Should create a plan for brightness");
        let plan = plan.unwrap();
        assert!(!plan.is_empty());
    }

    #[test]
    fn test_plan_fallback_to_tool_name() {
        let (_registry, _store, agent) = setup_test_env();
        let plan = agent.plan("gray_fill", None);
        assert!(plan.is_ok(), "Fallback should match tool name");
        let plan = plan.unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].tool_name, "gray_fill");
    }

    #[test]
    fn test_plan_empty_registry() {
        let ctx = Context::new();
        let registry: Arc<dyn ToolRegistry> =
            Arc::new(crate::tool_registry::ToolRegistryImpl::new());
        let codec: Arc<dyn kaleido_traits::FileCodec> =
            Arc::new(crate::file_codec_impl::FileCodecImpl::new());
        let image_store: Arc<ImageStoreImpl> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));
        let agent = AIAgentImpl::new(registry, image_store, ctx);
        let result = agent.plan("复古效果", None);
        assert!(result.is_err(), "Should fail with empty registry");
    }

    #[test]
    fn test_execute_plan_empty() {
        let (_registry, store, agent) = setup_test_env();
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(100, 100, 100)).unwrap();
        store.set_image(img).unwrap();
        let empty_plan = Plan::new("empty");
        let result = agent.execute_plan(&empty_plan);
        assert!(matches!(result, Err(AgentError::EmptyPlan)));
    }

    #[test]
    fn test_execute_plan_no_image() {
        let (_registry, _store, agent) = setup_test_env();
        let plan = Plan::new("test").with_action("gray_fill", serde_json::json!({}), "Fill gray");
        let result = agent.execute_plan(&plan);
        assert!(matches!(result, Err(AgentError::NoImageLoaded)));
    }

    #[test]
    fn test_execute_plan_success() {
        let (_registry, store, agent) = setup_test_env();
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(100, 100, 100)).unwrap();
        store.set_image(img).unwrap();
        let plan = Plan::new("test").with_action("gray_fill", serde_json::json!({}), "Fill with gray");
        let result = agent.execute_plan(&plan);
        assert!(result.is_ok(), "Plan execution should succeed");
        let plan_result = result.unwrap();
        assert!(plan_result.success);
        assert_eq!(plan_result.action_results.len(), 1);
    }

    #[test]
    fn test_run_plan_and_execute() {
        let (_registry, store, agent) = setup_test_env();
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(50, 50, 50)).unwrap();
        store.set_image(img).unwrap();
        let result = agent.run("亮一点", None);
        assert!(result.is_ok(), "run() should plan and execute");
        let plan_result = result.unwrap();
        assert!(plan_result.success);
    }

    #[test]
    fn test_stats_tracking() {
        let (_registry, store, agent) = setup_test_env();
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(50, 50, 50)).unwrap();
        store.set_image(img).unwrap();
        let _ = agent.run("gray_fill", None);
        let stats = agent.stats();
        assert!(stats.plans_created >= 1);
        assert!(stats.plans_executed >= 1);
        assert!(stats.actions_executed >= 1);
    }

    #[test]
    fn test_mode_is_template() {
        let (_registry, _store, agent) = setup_test_env();
        assert_eq!(agent.mode(), AgentMode::Template);
    }

    #[test]
    fn test_max_steps_exceeded() {
        let (_registry, store, agent) = setup_test_env();
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(50, 50, 50)).unwrap();
        store.set_image(img).unwrap();
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
