//! Button presets in LokalBot's three styles: filled teal (primary),
//! bordered (secondary actions) and quiet icon buttons for toolbars.

use gpui_kit::ElementId;
use gpui_kit::SharedString;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _};

use super::Ico;

/// Filled teal button with white text ("Record Now", "Ask").
pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).primary().label(label)
}

/// Bordered button for secondary actions ("Review Actions…").
pub fn bordered(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).outline().label(label)
}

/// Bordered button with a leading icon.
pub fn bordered_icon(id: impl Into<ElementId>, icon: Ico, label: impl Into<SharedString>) -> Button {
    Button::new(id).outline().icon(Icon::new(icon)).label(label)
}

/// Icon-only toolbar button with a tooltip.
pub fn toolbar_icon(id: impl Into<ElementId>, icon: Ico, tooltip: impl Into<SharedString>) -> Button {
    Button::new(id).ghost().icon(Icon::new(icon)).tooltip(tooltip)
}

/// Small quiet button, used inside rows and cards.
pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).ghost().small().label(label)
}
