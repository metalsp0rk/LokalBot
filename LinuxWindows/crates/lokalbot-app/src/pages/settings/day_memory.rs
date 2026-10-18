//! Day Memory: app and window activity and the day digest. Screen context
//! (screen text, screenshots, OCR) and coding agent sessions are not
//! implemented on Linux and Windows, so their rows are not shown; their
//! values stay in settings.json for the Mac app.

use gpui_kit::component::button::Button;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, ParentElement as _, Styled as _, div, px};

use super::{Section, SettingsPage};
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::settings_row;

/// The first characters of the digest instructions that are used.
pub const DIGEST_PROMPT_LIMIT: usize = 500;

fn hour_label(hour: u32) -> String {
    match hour % 24 {
        0 => "12 AM".into(),
        12 => "12 PM".into(),
        h if h < 12 => format!("{h} AM"),
        h => format!("{} PM", h - 12),
    }
}

impl SettingsPage {
    pub(super) fn day_memory_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let settings = self.data.read(cx).settings.clone();
        let data = self.data.clone();
        let lb = cx.lb().clone();

        let memory = Section::new("Day Memory")
            .row(
                "settings.trackingEnabled",
                self.toggle(
                    "settings.trackingEnabled",
                    "Track app & window activity",
                    Some("Records which app and window you're using for the Timeline and day digest."),
                    settings.tracking_enabled,
                    |s, v| s.tracking_enabled = v,
                    cx,
                ),
            )
            .row(
                "settings.activityManage",
                settings_row::content_row(
                    "settings.activityManage",
                    h_flex()
                        .gap(px(8.))
                        .flex_wrap()
                        .child(Button::new("settings-manage-retention").outline().small().label("Manage retention…").on_click(
                            cx.listener(|this, _, window, cx| this.reveal("settings.retentionDays", window, cx)),
                        ))
                        .child(Button::new("settings-manage-exclusions").outline().small().label("Manage exclusions…").on_click(
                            cx.listener(|this, _, window, cx| this.reveal("settings.excludedApps", window, cx)),
                        )),
                    false,
                    cx,
                ),
            )
            .row(
                "settings.activityDetails",
                settings_row::content_row(
                    "settings.activityDetails",
                    self.details(
                        "settings.activityDetails",
                        "What is recorded",
                        "Every few seconds LokalBot notes the app in front and its window title, and how long you stay. It never records what you type, the clipboard, or what is on screen. Excluded apps and sites are logged as “Private” with the time only, password managers are always excluded, and passwords or keys in window titles are redacted. Window titles are cleared after the retention period in Privacy & Data.",
                        cx,
                    ),
                    false,
                    cx,
                ),
            );

        let hour = settings.day_digest_hour.min(23);
        let hour_data = data.clone();
        let used = settings.day_digest_custom_prompt.trim().chars().count();
        let counter = if used > DIGEST_PROMPT_LIMIT {
            Some((
                format!(
                    "{used} of {DIGEST_PROMPT_LIMIT} characters. Only the first {DIGEST_PROMPT_LIMIT} are used; shorten the rest."
                ),
                lb.attention_text,
            ))
        } else if used > 0 {
            Some((
                format!("{used} of {DIGEST_PROMPT_LIMIT} characters"),
                lb.text_secondary,
            ))
        } else {
            None
        };
        let mut digest = Section::new("Day digest").row(
            "settings.dayDigestAutoEnabled",
            self.toggle(
                "settings.dayDigestAutoEnabled",
                "Generate the day digest automatically",
                Some("Writes the Timeline digest to your local journal at the chosen time, then finalizes yesterday after midnight so late activity is included."),
                settings.day_digest_auto_enabled,
                |s, v| s.day_digest_auto_enabled = v,
                cx,
            ),
        );
        if settings.day_digest_auto_enabled {
            digest = digest.row(
                "settings.dayDigestHour",
                settings_row::row(
                    "settings.dayDigestHour",
                    format!("Generate at {}", hour_label(hour)),
                    None,
                    settings_row::stepper(
                        "settings-digest-hour",
                        "",
                        hour > 0,
                        hour < 23,
                        move |step, _, cx| {
                            let next = (hour as i32 + step).clamp(0, 23) as u32;
                            hour_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.day_digest_hour = next, cx)
                            });
                        },
                        cx,
                    ),
                    self.is_focused("settings.dayDigestHour"),
                    cx,
                ),
            );
        }
        digest = digest.row(
            "settings.dayDigestCustomPrompt",
            settings_row::stacked_row(
                "settings.dayDigestCustomPrompt",
                "Digest instructions (optional)",
                Some(format!("Shapes both scheduled and manual digests. The first {DIGEST_PROMPT_LIMIT} characters are used.").into()),
                v_flex()
                    .gap(px(6.))
                    .child(Textarea::new(&self.digest_prompt).h(px(72.)).text_size(ty::BODY))
                    .when_some(counter, |this, (text, color)| {
                        this.child(div().text_size(ty::CAPTION).text_color(color).child(text))
                    }),
                self.is_focused("settings.dayDigestCustomPrompt"),
                cx,
            ),
        );

        vec![memory, digest]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_read_like_the_mac() {
        assert_eq!(hour_label(18), "6 PM");
        assert_eq!(hour_label(0), "12 AM");
        assert_eq!(hour_label(12), "12 PM");
        assert_eq!(hour_label(9), "9 AM");
    }
}
