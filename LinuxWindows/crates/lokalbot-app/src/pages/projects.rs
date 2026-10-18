//! Projects page.

use gpui_kit::{
    App, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div,
};

use crate::pages::Page;
use crate::ui::{self, Ico};

pub struct ProjectsPage {}

impl ProjectsPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Page for ProjectsPage {
    fn title(&self, _cx: &App) -> SharedString {
        "Projects".into()
    }
}

impl Render for ProjectsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(ui::empty_state(
                Ico::Folder,
                "Projects",
                "This page is being built.",
                cx,
            ))
    }
}
