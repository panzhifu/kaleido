//! AI Agent service contract.
//!
//! The [`AIAgent`] trait defines the interface for an AI-powered image
//! editing agent. The agent can:
//!
//! - **Plan** a sequence of tool operations from a high-level description
//! - **Execute** the plan against the current image
//! - **Generate** new tools when existing ones are insufficient
//!
//! # Architecture
//!
//! The agent operates in a loop:
//!
//! ```text
//! User prompt → Plan → Execute step → Reflect → Next step? → Done
//! ```
//!
//! Each step emits events (`ai_thinking`, `ai_action_executed`) so the UI
//! can show progress.
//!
//! # MVP Implementation
//!
//! The initial implementation uses a simple template-based planner that
//! maps keywords to predefined tool sequences. This provides a solid
//! foundation for future LLM integration — the trait interface stays
//! the same, only the planning strategy changes.

use serde::{Deserialize, Serialize};

use crate::ToolParams;

// ---------------------------------------------------------------------------
// Plan types
// ---------------------------------------------------------------------------

/// A planned operation: a tool to call with specific parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedAction {
    /// Name of the tool to execute (must exist in the ToolRegistry).
    pub tool_name: String,
    /// Parameters to pass to the tool.
    pub params: ToolParams,
    /// Human-readable explanation of what this step does.
    pub description: String,
}

/// A complete plan: a sequence of actions to achieve a goal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// The original user prompt.
    pub goal: String,
    /// Ordered list of actions to execute.
    pub actions: Vec<PlannedAction>,
}

impl Plan {
    /// Creates a new plan with the given goal.
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            actions: Vec::new(),
        }
    }

    /// Adds an action to the plan.
    pub fn with_action(mut self, tool_name: &str, params: ToolParams, description: &str) -> Self {
        self.actions.push(PlannedAction {
            tool_name: tool_name.to_string(),
            params,
            description: description.to_string(),
        });
        self
    }

    /// Returns the number of actions in this plan.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Returns true if the plan has no actions.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Execution result
// ---------------------------------------------------------------------------

/// Result of executing a single action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Index of the action in the plan.
    pub step: usize,
    /// Name of the tool that was executed.
    pub tool_name: String,
    /// Whether the execution succeeded.
    pub success: bool,
    /// Error message if execution failed.
    pub error: Option<String>,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
}

/// Result of executing a complete plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanResult {
    /// The plan that was executed.
    pub plan: Plan,
    /// Results for each action.
    pub action_results: Vec<ActionResult>,
    /// Whether the entire plan succeeded.
    #[allow(dead_code)]
    pub success: bool,
}

// ---------------------------------------------------------------------------
// AIAgent trait
// ---------------------------------------------------------------------------

/// AI-powered image editing agent.
///
/// The agent can plan and execute multi-step image editing operations.
/// In MVP mode, planning is template-based; in future versions, it can
/// be replaced with LLM-powered planning without changing this interface.
///
/// # Events Emitted
///
/// - `ai_thinking` — when the agent starts processing a request
/// - `ai_action_executed` — after each tool execution
/// - `tool_upgraded` — when a new tool is generated
pub trait AIAgent: Send + Sync + 'static {
    /// Plans a sequence of operations to achieve the given goal.
    ///
    /// The `context` JSON can provide additional information:
    /// - `current_image_size`: (width, height) of the current image
    /// - `available_tools`: list of tool names (auto-populated if not provided)
    ///
    /// Returns a [`Plan`] with the sequence of actions, or an error if
    /// the goal cannot be understood or no suitable tools are available.
    ///
    /// # Events
    ///
    /// Emits `ai_thinking` with the prompt.
    fn plan(&self, goal: &str, context: Option<&serde_json::Value>) -> AgentResult<Plan>;

    /// Executes a plan against the current image.
    ///
    /// Each action in the plan is executed sequentially. If an action fails,
    /// execution stops and the partial result is returned (unless
    /// `stop_on_failure` is false).
    ///
    /// # Events
    ///
    /// Emits `ai_action_executed` after each step.
    fn execute_plan(&self, plan: &Plan) -> AgentResult<PlanResult>;

    /// Convenience method: plan and execute in one call.
    ///
    /// Equivalent to calling `plan` followed by `execute_plan`.
    fn run(&self, goal: &str, context: Option<&serde_json::Value>) -> AgentResult<PlanResult> {
        let plan = self.plan(goal, context)?;
        self.execute_plan(&plan)
    }

    /// Returns the agent's current mode.
    fn mode(&self) -> AgentMode;

    /// Returns planning statistics (for debugging/monitoring).
    fn stats(&self) -> AgentStats;
}

/// Agent operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    /// Template-based planning (MVP).
    Template,
    /// LLM-powered planning (future).
    Llm,
    /// Hybrid: LLM planning with template fallback.
    Hybrid,
}

/// Agent statistics for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStats {
    /// Total number of plans created.
    pub plans_created: u64,
    /// Total number of plans executed.
    pub plans_executed: u64,
    /// Total number of actions executed.
    pub actions_executed: u64,
    /// Total number of failed actions.
    pub actions_failed: u64,
    /// Total number of tools generated.
    pub tools_generated: u64,
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during agent operation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AgentError {
    /// The goal could not be understood or planned.
    #[error("Planning failed: {reason}")]
    PlanningFailed { reason: String },

    /// A tool referenced in the plan was not found.
    #[error("Tool not found: {tool_name}")]
    ToolNotFound { tool_name: String },

    /// Tool execution failed.
    #[error("Execution failed at step {step}: {reason}")]
    ExecutionFailed { step: usize, reason: String },

    /// No image is loaded in the store.
    #[error("No image loaded in the store")]
    NoImageLoaded,

    /// The plan is empty (no actions to execute).
    #[error("Plan is empty — no actions to execute")]
    EmptyPlan,

    /// Maximum steps exceeded (safety limit).
    #[error("Plan exceeds maximum step count: {max}")]
    MaxStepsExceeded { max: usize },

    /// Internal error.
    #[error("Internal error: {reason}")]
    Internal { reason: String },
}

/// Result type for agent operations.
pub type AgentResult<T> = std::result::Result<T, AgentError>;

// ---------------------------------------------------------------------------
// Tool generation types
// ---------------------------------------------------------------------------

/// Description for generating a new tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGenerationRequest {
    /// Name for the new tool.
    pub name: String,
    /// What the tool should do.
    pub description: String,
    /// Parameter definitions.
    pub params: Vec<ToolParamDef>,
    /// How to implement the tool (template name or custom logic).
    pub implementation: ToolImplSpec,
}

/// Parameter definition for tool generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParamDef {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub param_type: String,
    /// Default value (JSON).
    pub default_value: serde_json::Value,
    /// Whether the parameter is required.
    pub required: bool,
}

/// How a generated tool should be implemented.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolImplSpec {
    /// Compose from existing tools (chain their effects).
    Compose {
        /// Ordered list of (tool_name, param_overrides) to compose.
        steps: Vec<(String, serde_json::Value)>,
    },
    /// Apply a simple pixel operation (brightness, contrast, etc).
    PixelOp {
        /// Operation name.
        op: String,
        /// Operation parameter name.
        param: String,
    },
}

/// Result of tool generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGenerationResult {
    /// Name of the generated tool.
    pub name: String,
    /// Whether the tool was successfully registered.
    pub registered: bool,
}
