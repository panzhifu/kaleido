//! Dock actions — add, remove, toggle panels.

use gpui::actions;

actions!(
    kaleido_dock,
    [
        /// Add a panel to the dock area.
        AddPanel,
        /// Close the current panel.
        ClosePanel,
        /// Toggle panel visibility.
        TogglePanel,
        /// Toggle zoom (maximize) the current panel.
        ToggleZoom,
    ]
);
