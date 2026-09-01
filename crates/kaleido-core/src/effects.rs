//! Effect chain — plugin-provided effects on nodes.
//!
//! Adjustment layers are **not** a built-in node type.  Instead, any node
//! may carry an effect chain ([`EffectBinding`]s).  The *implementation* of
//! each effect (brightness/contrast, curves, blur…) lives in a plugin
//! registered with the plugin manager; the host only defines the contract
//! and stores the bindings.  A `Subtree`-scoped binding reproduces the
//! Photoshop adjustment-layer semantics (applies to the node's composited
//! subtree), while `SelfOnly` is a filter on the node's own content.

use serde_json::Value as JsonValue;

use super::types::EffectId;

/// Effect scope — how far the effect reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EffectScope {
    /// Filter: only affects this node's own content.
    SelfOnly,
    /// Adjustment-layer semantics: affects this node and all descendants
    /// (applied to the subtree's composited result).
    Subtree,
}

/// A bound effect instance on a node's effect chain.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EffectBinding {
    /// ID of the plugin-registered effect implementation.
    pub effect: EffectId,
    /// Parameters as a JSON object (serializable, animation-drivable).
    pub params: JsonValue,
    pub scope: EffectScope,
    pub enabled: bool,
}

impl EffectBinding {
    /// Creates a binding for a plugin effect.
    #[inline]
    pub fn new(effect: EffectId, params: JsonValue, scope: EffectScope) -> Self {
        Self {
            effect,
            params,
            scope,
            enabled: true,
        }
    }

    /// Creates a binding with empty parameters.
    #[inline]
    pub fn simple(effect: EffectId, scope: EffectScope) -> Self {
        Self::new(effect, JsonValue::Object(Default::default()), scope)
    }

    /// Returns a copy with the given parameters.
    #[inline]
    pub fn with_params(mut self, params: JsonValue) -> Self {
        self.params = params;
        self
    }

    /// Returns a copy with the enabled flag set.
    #[inline]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}
