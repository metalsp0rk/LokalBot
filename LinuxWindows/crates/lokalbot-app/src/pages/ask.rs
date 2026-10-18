//! Ask page.

use gpui_kit::{App, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div};

use crate::pages::Page;
use crate::ui::{self, Ico};

pub struct AskPage {}

impl AskPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Page for AskPage {
    fn title(&self, _cx: &App) -> SharedString {
        "Ask".into()
    }
}

impl Render for AskPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().items_center().justify_center().child(ui::empty_state(
            Ico::TextSearch,
            "Ask",
            "This page is being built.",
            cx,
        ))
    }
}
