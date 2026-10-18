//! Dictation: the shortcut, how it triggers, where the text goes, and a place
//! to try it (`DictationSettingsControls.swift`, `DictationView.swift`).

use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, FontWeight, ParentElement as _, SharedString, Styled as _, div, px};
use lokalbot_core::settings::{DictationOutputMode, DictationTriggerMode};

use super::{Section, SettingsPage};
use crate::state::AppEvent;
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{Ico, settings_row};

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
                    DictationTriggerMode::ALL.iter().map(|m| (*m, SharedString::from(m.label()))).collect(),
                    settings.dictation_trigger_mode,
                    |s, v| s.dictation_trigger_mode = v,
                    cx,
                ),
            )
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
            )
            .row(
                "settings.dictationPlaysStartSound",
                self.toggle(
                    "settings.dictationPlaysStartSound",
                    "Play a sound when the microphone is ready",
                    None,
                    settings.dictation_plays_start_sound,
                    |s, v| s.dictation_plays_start_sound = v,
                    cx,
                ),
            )
            .row(
                "settings.dictationRetainAudio",
                self.toggle(
                    "settings.dictationRetainAudio",
                    "Keep dictation audio files",
                    None,
                    settings.dictation_retain_audio,
                    |s, v| s.dictation_retain_audio = v,
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
