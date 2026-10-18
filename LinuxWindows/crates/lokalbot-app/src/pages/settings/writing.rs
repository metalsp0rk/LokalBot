//! Writing: autocomplete, its practice editor, the writing profile and
//! exclusions (`AutocompleteExperienceView.swift`, `SettingsView.cotypingSection`).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{Context, FontWeight, ParentElement as _, SharedString, Styled as _, div, px};
use lokalbot_core::settings::AcceptGranularity;

use super::{Section, SettingsPage, models};
use crate::state::SettingsCategory;
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{self, Ico, settings_row};

pub fn granularity_label(granularity: AcceptGranularity) -> &'static str {
    match granularity {
        AcceptGranularity::Word => "One word",
        AcceptGranularity::Phrase => "One phrase",
    }
}

/// `CotypingRehearsalConversation.sample`.
const SAMPLE: [(&str, &str); 2] = [
    (
        "Sarah",
        "The migration timeline we scoped yesterday still works for my team.",
    ),
    (
        "Sarah",
        "Daniel can start the database cutover on Thursday if you confirm today.",
    ),
];

impl SettingsPage {
    pub(super) fn writing_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let (settings, model_ready) = {
            let data = self.data.read(cx);
            (data.settings.clone(), data.models.autocomplete_ready)
        };
        let lb = cx.lb().clone();
        let data = self.data.clone();
        let model_name = models::model_name(&settings.cotyping_built_in_model_id);

        // Summary row: what autocomplete is doing, its readiness and the switch.
        let summary_detail = if settings.cotyping_enabled {
            format!("{model_name} · suggestions appear as you type in other apps.")
        } else if model_ready {
            "The model is ready. Turn on autocomplete to start.".into()
        } else {
            "Download the Autocomplete model to try it.".into()
        };
        let enable_data = data.clone();
        let summary = h_flex()
            .w_full()
            .gap(px(12.))
            .items_center()
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
                            .child(if settings.cotyping_enabled {
                                "Autocomplete on"
                            } else {
                                "Autocomplete off"
                            }),
                    )
                    .child(
                        div()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(summary_detail),
                    )
                    .child(h_flex().gap(px(6.)).pt(px(2.)).child(status_pill(
                        if model_ready {
                            "Model ready"
                        } else {
                            "Model needed"
                        },
                        model_ready,
                        cx,
                    ))),
            )
            .child(settings_row::switch(
                "settings.cotypingEnabled",
                settings.cotyping_enabled,
                move |on, _, cx| {
                    enable_data.update(cx, |data, cx| {
                        data.update_settings(|s| s.cotyping_enabled = on, cx)
                    })
                },
            ));
        let home = Section::new(None).row(
            "settings.cotypingEnabled",
            settings_row::content_row(
                "settings.cotypingEnabled",
                summary,
                self.is_focused("settings.cotypingEnabled"),
                cx,
            ),
        );

        // Practice editor. Suggestions come from the autocomplete runtime once it is connected.
        let conversation = v_flex()
            .gap(px(6.))
            .child(
                div()
                    .text_size(ty::SECONDARY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(lb.text_secondary)
                    .child("Sample conversation"),
            )
            .children(SAMPLE.iter().map(|(sender, text)| {
                h_flex()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .bg(lb.text.opacity(0.05))
                    .text_size(ty::SECONDARY)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(lb.text)
                            .child(*sender),
                    )
                    .child(div().text_color(lb.text).child(*text))
            }));
        let hint = format!(
            "Tab accepts the next {} · Esc dismisses",
            if settings.cotyping_accept_granularity == AcceptGranularity::Word {
                "word"
            } else {
                "phrase"
            }
        );
        let practice = v_flex()
            .w_full()
            .gap(px(10.))
            .child(conversation)
            .child(Textarea::new(&self.practice).h(px(76.)).text_size(ty::BODY))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(hint),
                    )
                    .child(
                        Button::new("settings-insert-suggestion")
                            .primary()
                            .small()
                            .label("Insert suggestion")
                            .disabled(true),
                    ),
            )
            .child(settings_row::help(
                if settings.cotyping_enabled {
                    "Suggestions appear here as grey text once the Autocomplete model is running."
                } else {
                    "Turn on autocomplete to see suggestions while you type."
                },
                cx,
            ));
        let try_it = Section::new("Try the real autocomplete").row(
            "settings.autocompletePreview",
            settings_row::content_row("settings.autocompletePreview", practice, false, cx),
        );

        let words = settings.cotyping_max_words.clamp(2, 50);
        let words_data = data.clone();
        let change_data = data.clone();
        let autocomplete = Section::new("Autocomplete")
            .row(
                "settings.cotypingModel",
                settings_row::row(
                    "settings.cotypingModel",
                    "Autocomplete model",
                    None,
                    h_flex()
                        .gap(px(10.))
                        .items_center()
                        .child(
                            div()
                                .text_size(ty::BODY)
                                .text_color(lb.text_secondary)
                                .child(model_name),
                        )
                        .child(
                            Button::new("settings-cotyping-model")
                                .outline()
                                .small()
                                .label("Change…")
                                .on_click(move |_, _, cx| {
                                    change_data.update(cx, |data, cx| {
                                        data.open_settings(SettingsCategory::Models, cx)
                                    });
                                }),
                        ),
                    false,
                    cx,
                ),
            )
            .row(
                "settings.cotypingMaxWords",
                settings_row::row(
                    "settings.cotypingMaxWords",
                    format!("Suggestion length: up to {words} words"),
                    None,
                    settings_row::stepper(
                        "settings-cotyping-words",
                        "",
                        words > 2,
                        words < 50,
                        move |step, _, cx| {
                            let next = (words as i32 + step).clamp(2, 50) as u32;
                            words_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.cotyping_max_words = next, cx)
                            });
                        },
                        cx,
                    ),
                    self.is_focused("settings.cotypingMaxWords"),
                    cx,
                ),
            )
            .row(
                "settings.cotypingMultiLine",
                self.toggle(
                    "settings.cotypingMultiLine",
                    "Allow multi-line suggestions",
                    None,
                    settings.cotyping_multi_line,
                    |s, v| s.cotyping_multi_line = v,
                    cx,
                ),
            )
            .row(
                "settings.cotypingDebounceMs",
                settings_row::row(
                    "settings.cotypingDebounceMs",
                    "Pause before suggesting",
                    None,
                    h_flex()
                        .gap(px(10.))
                        .items_center()
                        .child(div().w(px(220.)).child(Slider::new(&self.debounce_slider)))
                        .child(
                            div()
                                .min_w(px(56.))
                                .text_size(ty::BODY)
                                .text_color(lb.text_secondary)
                                .flex()
                                .justify_end()
                                .child(format!("{} ms", settings.cotyping_debounce_ms)),
                        ),
                    self.is_focused("settings.cotypingDebounceMs"),
                    cx,
                ),
            )
            .row(
                "settings.cotypingAcceptGranularity",
                self.choice(
                    "settings.cotypingAcceptGranularity",
                    "Each accept takes",
                    None,
                    vec![
                        (
                            AcceptGranularity::Word,
                            granularity_label(AcceptGranularity::Word).into(),
                        ),
                        (
                            AcceptGranularity::Phrase,
                            granularity_label(AcceptGranularity::Phrase).into(),
                        ),
                    ],
                    settings.cotyping_accept_granularity,
                    |s, v| s.cotyping_accept_granularity = v,
                    cx,
                ),
            );

        let profile = Section::new("Writing profile")
            .row(
                "settings.cotypingUserName",
                settings_row::row(
                    "settings.cotypingUserName",
                    "Your name",
                    None,
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
            );

        let exclusions = Section::new("Exclusions").row(
            "settings.cotypingExcludedApps",
            settings_row::content_row(
                "settings.cotypingExcludedApps",
                self.cotyping_excluded_apps.clone(),
                self.is_focused("settings.cotypingExcludedApps"),
                cx,
            ),
        );

        vec![home, try_it, autocomplete, profile, exclusions]
    }
}

/// "Model ready" in green or "Model needed" in amber (`MemoryHealthStatus`).
pub fn status_pill(label: &'static str, good: bool, cx: &gpui_kit::App) -> gpui_kit::Div {
    let lb = cx.lb();
    let tint = if good {
        lb.success_text
    } else {
        lb.attention_text
    };
    h_flex()
        .gap(px(5.))
        .items_center()
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .bg(tint.opacity(0.10))
        .text_size(ty::CAPTION)
        .font_weight(FontWeight::MEDIUM)
        .text_color(tint)
        .child(ui::status_dot(tint, px(6.)))
        .child(SharedString::from(label))
}
