//! Dictation: the shortcut, how it triggers, Transcribe or Compose, where the
//! text goes, and a place to try it (`DictationSettingsControls.swift`,
//! `DictationView.swift`). The start sound and kept audio files are not
//! implemented on Linux and Windows, so their rows are not shown.

use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, FontWeight, ParentElement as _, SharedString, Styled as _, div, px};
use lokalbot_core::Settings;
use lokalbot_core::settings::{DictationIntent, DictationOutputMode, DictationTriggerMode};

use super::{Section, SettingsPage};
use crate::state::AppEvent;
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{Ico, settings_row};

const INTENTS: [DictationIntent; 2] = [DictationIntent::Transcribe, DictationIntent::Compose];

fn intent_label(intent: DictationIntent) -> &'static str {
    match intent {
        DictationIntent::Transcribe => "Transcribe",
        DictationIntent::Compose => "Compose",
    }
}

fn intent_detail(intent: DictationIntent) -> &'static str {
    match intent {
        DictationIntent::Transcribe => "Keeps the recognized speech. No rewriting.",
        DictationIntent::Compose => {
            "Your Think model tidies what you say. Start with “reply…”, “write…” or “draft…” to have it write the text for you."
        }
    }
}

/// "Name, Style" or "Not set" (`DictationView.writingProfileSummary`).
pub fn writing_profile_summary(settings: &Settings) -> &'static str {
    match (
        settings.cotyping_user_name.trim().is_empty(),
        settings.cotyping_style_note.trim().is_empty(),
    ) {
        (false, false) => "Name, Style",
        (false, true) => "Name",
        (true, false) => "Style",
        (true, true) => "Not set",
    }
}

impl SettingsPage {
    pub(super) fn dictation_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let (settings, microphones, last) = {
            let data = self.data.read(cx);
            (
                data.settings.clone(),
                data.audio_inputs.clone(),
                data.last_dictation.clone(),
            )
        };
        let lb = cx.lb().clone();

        let mut options: Vec<(SharedString, SharedString)> =
            vec![("".into(), "System default".into())];
        options.extend(microphones.iter().map(|m| (m.clone(), m.clone())));
        if !settings.dictation_microphone_id.is_empty()
            && !options
                .iter()
                .any(|(value, _)| value.as_ref() == settings.dictation_microphone_id)
        {
            let id: SharedString = settings.dictation_microphone_id.clone().into();
            options.push((id.clone(), id));
        }
        let current = options
            .iter()
            .find(|(value, _)| value.as_ref() == settings.dictation_microphone_id)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| "System default".into());
        let items: Vec<(SharedString, bool)> = options
            .iter()
            .map(|(value, label)| {
                (
                    label.clone(),
                    value.as_ref() == settings.dictation_microphone_id,
                )
            })
            .collect();
        let values: Vec<String> = options.iter().map(|(value, _)| value.to_string()).collect();
        let microphone_data = self.data.clone();
        let intent_data = self.data.clone();
        let intent_index = INTENTS
            .iter()
            .position(|i| *i == settings.dictation_intent)
            .unwrap_or(0);

        let dictation = Section::new("Dictation")
            .row(
                "settings.dictationEnabled",
                self.toggle(
                    "settings.dictationEnabled",
                    "Enable dictation shortcut",
                    None,
                    settings.dictation_enabled,
                    |s, v| s.dictation_enabled = v,
                    cx,
                ),
            )
            .row(
                "settings.dictationShortcut",
                settings_row::row(
                    "settings.dictationShortcut",
                    "Shortcut",
                    None,
                    self.dictation_shortcut.clone(),
                    self.is_focused("settings.dictationShortcut"),
                    cx,
                ),
            )
            .row(
                "settings.dictationTriggerMode",
                self.choice(
                    "settings.dictationTriggerMode",
                    "Trigger",
                    None,
                    DictationTriggerMode::ALL
                        .iter()
                        .map(|m| (*m, SharedString::from(m.label())))
                        .collect(),
                    settings.dictation_trigger_mode,
                    |s, v| s.dictation_trigger_mode = v,
                    cx,
                ),
            )
            .row(
                "settings.dictationIntent",
                settings_row::row(
                    "settings.dictationIntent",
                    "Intent",
                    Some(intent_detail(settings.dictation_intent).into()),
                    settings_row::segmented(
                        "settings-dictation-intent",
                        &INTENTS.map(intent_label),
                        intent_index,
                        move |index, _, cx| {
                            let intent = INTENTS[index];
                            intent_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.dictation_intent = intent, cx)
                            });
                        },
                        cx,
                    ),
                    self.is_focused("settings.dictationIntent"),
                    cx,
                ),
            );
        let dictation = if settings.dictation_intent == DictationIntent::Compose {
            let compose_help = if crate::pages::ask::composer_inference(cx).remote {
                "Compose uses your name and writing style. What you dictate is sent to your remote Think model for the final wording."
            } else {
                "Compose uses your name and writing style, with your Think model on this PC."
            };
            dictation.row(
                "settings.dictationWritingProfile",
                settings_row::row(
                    "settings.dictationWritingProfile",
                    "Writing profile",
                    Some(compose_help.into()),
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(settings_row::value(writing_profile_summary(&settings), cx))
                        .child(
                            Button::new("settings-dictation-profile")
                                .outline()
                                .small()
                                .label("Edit…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.reveal("settings.cotypingUserName", window, cx)
                                })),
                        ),
                    self.is_focused("settings.dictationWritingProfile"),
                    cx,
                ),
            )
        } else {
            dictation
        };
        let dictation = dictation
            .row(
                "settings.dictationOutputMode",
                self.choice(
                    "settings.dictationOutputMode",
                    "After a shortcut recording",
                    None,
                    DictationOutputMode::ALL.iter().map(|m| (*m, SharedString::from(m.label()))).collect(),
                    settings.dictation_output_mode,
                    |s, v| s.dictation_output_mode = v,
                    cx,
                ),
            )
            .row(
                "settings.dictationMicrophoneID",
                settings_row::row(
                    "settings.dictationMicrophoneID",
                    "Microphone",
                    Some("With a Bluetooth headset, choose the built-in microphone to keep the headset's audio quality while you dictate.".into()),
                    settings_row::picker("settings-dictation-microphone", current, items, move |index, _, cx| {
                        if let Some(value) = values.get(index).cloned() {
                            microphone_data.update(cx, |data, cx| data.update_settings(|s| s.dictation_microphone_id = value, cx));
                        }
                    }),
                    self.is_focused("settings.dictationMicrophoneID"),
                    cx,
                ),
            )
            .row(
                "settings.dictationShowOverlay",
                self.toggle(
                    "settings.dictationShowOverlay",
                    "Show floating dictation status",
                    None,
                    settings.dictation_show_overlay,
                    |s, v| s.dictation_show_overlay = v,
                    cx,
                ),
            );

        let status: SharedString = if settings.dictation_enabled {
            let verb = match settings.dictation_trigger_mode {
                DictationTriggerMode::PushToTalk => "hold",
                DictationTriggerMode::Toggle => "press",
                DictationTriggerMode::TapOrHold => "tap or hold",
            };
            format!("Ready — {verb} {} to dictate.", settings.dictation_shortcut).into()
        } else {
            "The dictation shortcut is off. Try it here, or turn on the shortcut above.".into()
        };
        let data = self.data.clone();
        let try_it = v_flex()
            .w_full()
            .gap(px(10.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(ty::BODY)
                            .text_color(lb.text)
                            .child(status),
                    )
                    .child(
                        Button::new("settings-try-dictation")
                            .outline()
                            .small()
                            .icon(Icon::new(Ico::Mic))
                            .label("Try here")
                            .on_click(move |_, _, cx| {
                                data.update(cx, |data, cx| {
                                    data.request(AppEvent::ToggleDictation, cx)
                                })
                            }),
                    ),
            )
            .when_some(last, |this, text| {
                this.child(
                    v_flex()
                        .gap(px(4.))
                        .p(px(10.))
                        .rounded(px(8.))
                        .bg(lb.chip_fill)
                        .child(
                            div()
                                .text_size(ty::CAPTION)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(lb.text_secondary)
                                .child("Last result"),
                        )
                        .child(div().text_size(ty::BODY).text_color(lb.text).child(text)),
                )
            });
        let try_section = Section::new("Try dictation").row(
            "settings.dictationTry",
            settings_row::content_row("settings.dictationTry", try_it, false, cx),
        );

        vec![dictation, try_section]
    }
}
