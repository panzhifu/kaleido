/// Panel contract — lets plugins render custom UI in the host's panel area.
///
/// A [`Panel`] is a UI surface that the host displays in its side panel
/// (properties, tool options, histogram, etc.). Plugins implement
/// `Panel` to show interactive controls or read-only results.
///
/// The panel is rendered through a callback-based approach: the host
/// provides a `PanelContext` that the panel uses to emit element
/// descriptions. This keeps the trait UI-framework-agnostic while still
/// allowing rich content.

use serde_json::Value;

// ---------------------------------------------------------------------------
// PanelSection — a labelled group of content inside a panel
// ---------------------------------------------------------------------------

/// A section within a panel fold-out.
#[derive(Debug, Clone, Default)]
pub struct PanelSection {
    /// Section heading (e.g. "Tool Options", "Histogram").
    pub title: String,
    /// Whether the section starts expanded.
    pub expanded: bool,
    /// Child element descriptions (see [`PanelElement`]).
    pub children: Vec<PanelElement>,
}

impl PanelSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            expanded: true,
            children: Vec::new(),
        }
    }

    pub fn with_element(mut self, el: PanelElement) -> Self {
        self.children.push(el);
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

// ---------------------------------------------------------------------------
// PanelElement — a single UI element inside a panel
// ---------------------------------------------------------------------------

/// A description of one UI element the host should render.
///
/// This is a simplified view-model: the host interprets each variant
/// and renders the appropriate widget. Plugins never touch GPUI
/// directly, so the same panel works on desktop and (eventually) in a
/// web-based host.
#[derive(Debug, Clone)]
pub enum PanelElement {
    /// A read-only text label.
    Label {
        text: String,
    },
    /// A heading (slightly larger / bolder text).
    Heading {
        text: String,
    },
    /// A horizontal divider.
    Divider,
    /// A numeric slider or spinner.
    NumberInput {
        label: String,
        value: f64,
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        /// Change callback identifier — the host calls
        /// `Panel::on_change(id, value)` when the user drags the slider.
        id: String,
    },
    /// A checkbox.
    Checkbox {
        label: String,
        checked: bool,
        id: String,
    },
    /// A dropdown / enum choice.
    Dropdown {
        label: String,
        options: Vec<String>,
        selected: usize,
        id: String,
    },
    /// A colour swatch picker.
    ColorPicker {
        label: String,
        /// RGBA hex string, e.g. "#FF0000FF".
        value: String,
        id: String,
    },
    /// A row of buttons (e.g. "Apply", "Reset").
    ButtonRow {
        buttons: Vec<PanelButton>,
    },
    /// A 2-D canvas for read-only previews (histogram, curve, etc.).
    ///
    /// The plugin draws into the pixel buffer and the host displays it.
    Canvas {
        width: u32,
        height: u32,
        /// RGBA8 pixel data (length = width * height * 4).
        pixels: Vec<u8>,
    },
    /// A progress bar.
    Progress {
        /// 0.0..=1.0, or `None` for indeterminate.
        value: Option<f64>,
        label: Option<String>,
    },
    /// A nested section (collapsible).
    Section(PanelSection),
}

/// A button inside a [`PanelElement::ButtonRow`].
#[derive(Debug, Clone)]
pub struct PanelButton {
    pub label: String,
    pub id: String,
    /// Whether this button is the primary / default action.
    pub primary: bool,
}

// ---------------------------------------------------------------------------
// PanelContext — host-provided rendering context
// ---------------------------------------------------------------------------

/// Host-provided context for a panel to build its UI description.
pub trait PanelContext {
    /// Adds a section to the panel.
    fn add_section(&mut self, section: PanelSection);

    /// Clears all content (called before each rebuild).
    fn clear(&mut self);
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// A plugin-supplied UI surface displayed in the host's panel area.
///
/// A panel is rebuilt whenever its tool is activated or when the plugin
/// calls `refresh()`. User interactions are reported back through
/// `on_change` and `on_button`.
///
/// # When to implement `Panel`
///
/// - Your tool has settings the user should tweak (brush size, opacity,
///   blending mode).
/// - Your tool shows read-only results (histogram, colour info, navigator).
/// - Your tool needs Apply / Cancel / Reset buttons.
///
/// Simple tools with no UI beyond the parameter schema do not need a
/// panel — the host auto-generates a form from the schema.
pub trait Panel: Send + Sync + 'static {
    /// Rebuilds the panel content.
    ///
    /// The host calls this when the panel needs refreshing. The previous
    /// content has already been cleared.
    fn render(&mut self, ctx: &mut dyn PanelContext);

    /// Called when the user changes a value in a `NumberInput`,
    /// `Checkbox`, `Dropdown`, or `ColorPicker`.
    ///
    /// `id` matches the `id` field of the element the user interacted with.
    /// `value` is the new JSON value (number, bool, or string).
    fn on_change(&mut self, _id: &str, _value: Value) {}

    /// Called when the user clicks a button in a `ButtonRow`.
    fn on_button(&mut self, _id: &str) {}

    /// Called when the panel becomes visible (its tool was activated).
    fn on_show(&mut self) {}

    /// Called when the panel is hidden (its tool was deactivated).
    fn on_hide(&mut self) {}

    /// Requests the host to rebuild this panel on the next frame.
    ///
    /// The default implementation is a no-op; panels that need to
    /// refresh in response to external changes (e.g. selection change)
    /// should override this or call into a shared refresh mechanism.
    fn refresh(&mut self) {}
}

impl std::fmt::Debug for dyn Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panel").finish_non_exhaustive()
    }
}
