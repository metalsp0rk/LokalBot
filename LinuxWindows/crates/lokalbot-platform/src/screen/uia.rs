//! Windows: visible text and secure-field state through UI Automation.

use std::time::Duration;

use super::{FocusedWindow, TextRead, TextReader};

pub(super) struct UiaReader;

impl UiaReader {
    pub(super) fn new() -> Self {
        Self
    }
}

impl TextReader for UiaReader {
    fn read(&mut self, _window: &FocusedWindow, _include_text: bool, _deadline: Duration) -> TextRead {
        TextRead::Unavailable("UI Automation is not read yet".into())
    }

    fn name(&self) -> &'static str {
        "uia"
    }
}
