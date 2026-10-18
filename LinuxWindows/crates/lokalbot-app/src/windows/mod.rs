//! Windows besides the main one: Quick Recall, onboarding and the
//! full-size screen image.

pub mod onboarding;
pub mod quick_recall;
pub mod screen_image;

use gpui_kit::{AnyWindowHandle, App, Global, WeakEntity};

/// Handles of the secondary windows, so they are reused instead of stacked.
#[derive(Default)]
struct SecondaryWindows {
    quick_recall: Option<AnyWindowHandle>,
    quick_recall_view: Option<WeakEntity<quick_recall::QuickRecall>>,
    onboarding: Option<AnyWindowHandle>,
    screen_image: Option<AnyWindowHandle>,
}

impl Global for SecondaryWindows {}

fn secondary(cx: &mut App) -> &mut SecondaryWindows {
    cx.default_global::<SecondaryWindows>()
}

/// Brings a still-open window forward; `false` when it was closed.
fn activate(handle: Option<AnyWindowHandle>, cx: &mut App) -> bool {
    handle.is_some_and(|handle| {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    })
}

/// Brings the main window forward (the first window that is not a secondary one).
pub fn activate_main(cx: &mut App) {
    let skip: Vec<_> = {
        let windows = secondary(cx);
        [windows.quick_recall, windows.onboarding, windows.screen_image]
            .into_iter()
            .flatten()
            .map(|h| h.window_id())
            .collect()
    };
    for handle in cx.windows() {
        if skip.contains(&handle.window_id()) {
            continue;
        }
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }
}
