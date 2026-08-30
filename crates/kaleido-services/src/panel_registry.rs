//! Panel registry — manages plugin-supplied panels.
//!
//! The [`PanelRegistry`] holds panels provided by plugins. The host
//! queries the registry to find the panel for the currently active tool
//! and renders it in the side panel area.

use std::sync::{Arc, Weak};

use kaleido_traits::Panel;

// ---------------------------------------------------------------------------
// PanelRegistry
// ---------------------------------------------------------------------------

/// Registry of panels currently provided by active plugins.
///
/// Implementations hold weak references so panels disappear automatically
/// when their providing plugin is disposed.
pub trait PanelRegistry: Send + Sync + 'static {
    /// Registers a panel. Held weakly — the plugin keeps the strong `Arc`
    /// alive for as long as its fiber is active.
    fn register(&self, panel: Weak<dyn Panel>);

    /// Removes the panel at the given index, if present.
    fn unregister(&self, index: usize);

    /// Returns all live panels (dead weak pointers are filtered out).
    fn panels(&self) -> Vec<Arc<dyn Panel>>;

    /// Returns the number of live panels.
    fn len(&self) -> usize;

    /// Returns `true` when no panels are registered.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Resolves the panel registry from a Cordis context.
pub fn resolve_panel_registry(
    ctx: &cordis::Context,
) -> cordis::Result<Arc<dyn PanelRegistry>> {
    let inner = ctx
        .get::<Arc<dyn PanelRegistry>>("panel_registry")?
        .ok_or_else(|| {
            cordis::CordisError::with_message(
                cordis::ErrorCode::MissingService,
                "panel_registry service is not available",
            )
        })?;
    Ok(inner.as_ref().clone())
}

// ---------------------------------------------------------------------------
// PanelEntry — internal bookkeeping
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PanelEntry {
    pub panel: Weak<dyn Panel>,
}

// ---------------------------------------------------------------------------
// PanelRegistryImpl
// ---------------------------------------------------------------------------

/// Default implementation of [`PanelRegistry`].
#[derive(Debug, Default)]
pub struct PanelRegistryImpl {
    panels: std::sync::Mutex<Vec<PanelEntry>>,
}

impl PanelRegistryImpl {
    /// Creates an empty panel registry.
    pub fn new() -> Self {
        Self {
            panels: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Cleans up dead weak pointers.
    fn sweep(&self) {
        let mut panels = self.panels.lock().unwrap();
        panels.retain(|entry| entry.panel.upgrade().is_some());
    }
}

impl PanelRegistry for PanelRegistryImpl {
    fn register(&self, panel: Weak<dyn Panel>) {
        let mut panels = self.panels.lock().unwrap();
        panels.push(PanelEntry { panel });
    }

    fn unregister(&self, index: usize) {
        let mut panels = self.panels.lock().unwrap();
        if index < panels.len() {
            panels.remove(index);
        }
    }

    fn panels(&self) -> Vec<Arc<dyn Panel>> {
        self.sweep();
        let panels = self.panels.lock().unwrap();
        panels
            .iter()
            .filter_map(|entry| entry.panel.upgrade())
            .collect()
    }

    fn len(&self) -> usize {
        self.sweep();
        let panels = self.panels.lock().unwrap();
        panels.len()
    }
}
