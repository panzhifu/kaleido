//! Panel registry — manages plugin-supplied panels.
//!
//! The [`PanelRegistry`] contract lives in `kaleido-traits`; this module
//! provides the default implementation plus the Cordis plugin and resolver.

use std::sync::{Arc, Mutex, Weak};

use cordis::{Inject, PluginHandle};
use kaleido_traits::{Panel, PanelRegistry};

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
    pub panel: Weak<Mutex<dyn Panel>>,
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
    fn register(&self, panel: Weak<Mutex<dyn Panel>>) {
        let mut panels = self.panels.lock().unwrap();
        panels.push(PanelEntry { panel });
    }

    fn unregister(&self, index: usize) {
        let mut panels = self.panels.lock().unwrap();
        if index < panels.len() {
            panels.remove(index);
        }
    }

    fn panels(&self) -> Vec<Arc<Mutex<dyn Panel>>> {
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

// ---------------------------------------------------------------------------
// Cordis plugin
// ---------------------------------------------------------------------------

/// Plugin that installs the [`PanelRegistry`] service.
///
/// Plugins provide panels via `register`; the host reads live panels each
/// frame and renders them in its side panel area.
pub fn panel_registry_plugin() -> PluginHandle {
    cordis::plugin_sync::<(), _>("panel_registry", Inject::none(), |ctx, _config| {
        let registry: Arc<dyn PanelRegistry> = Arc::new(PanelRegistryImpl::new());
        ctx.provide("panel_registry", registry)?;
        Ok(cordis::PluginOutput::none())
    })
}
