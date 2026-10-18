//! Writing: the writing profile Compose dictation uses
//! (`SettingsView.cotypingSection`, `lokalbot_ai::dictation::Profile`).
//! Autocomplete is macOS-only for now, so its controls are not shown here;
//! its values stay in settings.json for the Mac app.

use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{Context, FontWeight, ParentElement as _, Styled as _, div, px};

use super::{Section, SettingsPage};
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{self, Ico, settings_row};

impl SettingsPage {
    pub(super) fn writing_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let lb = cx.lb().clone();

        let note = h_flex()
            .w_full()
            .gap(px(12.))
            .items_start()
            .child(ui::icon_tile(Ico::TextCursorInput, lb.accent_text, px(40.)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(ty::BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(lb.text)
                            .child("Autocomplete is on macOS only for now"),
                    )
                    .child(
                        div()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child("On this PC, dictation can write for you. With Compose, it tidies what you say, or drafts the reply you ask for, using the profile below."),
                    )
                    .child(
                        h_flex().pt(px(4.)).child(
                            Button::new("settings-open-dictation")
                                .outline()
                                .small()
                                .icon(Icon::new(Ico::Mic))
                                .label("Dictation Settings")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.reveal("settings.dictationIntent", window, cx)
                                })),
                        ),
                    ),
            );
        let availability = Section::new(None).row(
            "settings.autocompleteAvailability",
            settings_row::content_row(
                "settings.autocompleteAvailability",
                note,
                self.is_focused("settings.autocompleteAvailability"),
                cx,
            ),
        );

        let profile = Section::new("Writing profile")
            .row(
                "settings.cotypingUserName",
                settings_row::row(
                    "settings.cotypingUserName",
                    "Your name",
                    Some("Also keeps you out of People.".into()),
                    div()
                        .w(px(280.))
                        .child(Input::new(&self.cotyping_name).small()),
                    self.is_focused("settings.cotypingUserName"),
                    cx,
                ),
            )
            .row(
                "settings.cotypingStyleNote",
                settings_row::row(
                    "settings.cotypingStyleNote",
                    "Writing style",
                    None,
                    div()
                        .w(px(280.))
                        .child(Input::new(&self.cotyping_style).small()),
                    self.is_focused("settings.cotypingStyleNote"),
                    cx,
                ),
            )
            .row(
                "settings.writingProfileHelp",
                settings_row::content_row(
                    "settings.writingProfileHelp",
                    settings_row::help(
                        "Compose dictation writes as you, in this style. Both are optional and stay on this PC unless your Think model runs on a server.",
                        cx,
                    ),
                    false,
                    cx,
                ),
            );

        vec![availability, profile]
    }
}
