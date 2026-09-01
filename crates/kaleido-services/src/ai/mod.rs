//! The **AI assistant** service — the 13th service (outside the 12-manager
//! document), already reorganized into `services/`.
//!
//! The AI agent plans multi-step image editing operations from a
//! natural-language goal and executes them against the current image:
//!
//! - **Contract**: [`kaleido_traits::ai_agent`] — `AIAgent`, `Plan`,
//!   `PlanResult`, `AgentStats`, `AgentError`, …
//! - **Implementation**: [`ai_agent`] — [`AIAgentImpl`], a template-based
//!   planner (MVP) plus a fail-fast executor running tools through the
//!   legacy `image_store` service.
//! - **Wiring**: installed as the `ai_agent` Cordis service by
//!   [`crate::services::app::cordis_plugins::ai_agent_plugin`]; depends on
//!   `image_store` + `tool_registry` (see `kaleido_app::boot`).
//!
//! # Status / migration
//!
//! The executor depends on the legacy [`ImageStoreImpl`] (old `TiledImage`
//! model), one of the legacy services kept for the old desktop / CLI hosts.
//! The migration path to the new document model
//! ([`kaleido_traits::services::data::DataService`]) is documented in the
//! [`ai_agent`] module header.

pub mod ai_agent;
pub use ai_agent::AIAgentImpl;
