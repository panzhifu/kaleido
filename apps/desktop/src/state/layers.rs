//! Layer state management.

use kaleido_services::layer::{Layer, LayerId, LayerStack};

#[derive(Debug, Clone)]
pub struct LayersState {
    pub stack: LayerStack,
    pub active_layer: Option<LayerId>,
}

impl Default for LayersState {
    fn default() -> Self {
        Self {
            stack: LayerStack::new(0, 0),
            active_layer: None,
        }
    }
}

impl LayersState {
    pub fn set_stack(&mut self, stack: LayerStack) {
        self.stack = stack;
        self.active_layer = self.stack.layer(0).map(|l| l.id);
    }
}
