//! Linux: visible text and secure-field state through AT-SPI.

use std::time::Duration;

use super::{FocusedWindow, TextRead, TextReader};

pub(super) struct AtspiReader;

impl AtspiReader {
    pub(super) fn new() -> Self {
        Self
    }
}

impl TextReader for AtspiReader {
    fn read(&mut self, _window: &FocusedWindow, _include_text: bool, _deadline: Duration) -> TextRead {
        TextRead::Unavailable("AT-SPI is not read yet".into())
    }

    fn name(&self) -> &'static str {
        "atspi"
    }
}
