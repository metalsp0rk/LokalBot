//! The meeting being recorded right now (`LiveMeetingDetailView.swift`,
//! `RecordingHealthStrip.swift`): recording pill, capture health with level
//! meters, the live transcript preview and a notes pad.

use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, Hsla, IntoElement, ParentElement as _, Styled as _, Window,
    div, px,
};

use super::{MeetingsPage, NotesStatus, status_text};
use crate::state::RecordingState;
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

impl MeetingsPage {
    pub(super) fn render_live(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lb = cx.lb().clone();
        let data = self.data.read(cx);
        let RecordingState::Recording {
            meeting,
            started,
            has_system_audio,
            mic_level,
            system_level,
        } = &data.recording
        else {
            return div().into_any_element();
        };
        let meeting = meeting.clone();
        let elapsed = started.elapsed().as_secs_f64();
        let (has_system, mic, system) = (*has_system_audio, *mic_level, *system_level);
        let phase = (elapsed * 4.).floor() as usize;

        let pill = ui::status_surface(lb.recording, cx)
            .rounded_full()
            .px(px(10.))
            .py(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(ui::status_dot(
                lb.recording.opacity(if phase % 2 == 0 { 1. } else { 0.55 }),
                px(7.),
            ))
            .child(
                div()
                    .text_size(ty::SECONDARY)
                    .text_color(lb.recording_text)
                    .child("Recording"),
            )
            .child(live_bars(mic.max(system), elapsed, lb.recording_text))
            .child(
                div()
                    .text_size(ty::CAPTION)
                    .text_color(lb.text_secondary)
                    .child(lokalbot_core::time::timer(elapsed)),
            );

        let chip = |icon: Ico, text: String, color: Hsla| {
            h_flex()
                .flex_none()
                .gap(px(5.))
                .items_center()
                .px(px(10.))
                .py(px(4.))
                .rounded_full()
                .bg(lb.chip_fill)
                .text_size(ty::SECONDARY)
                .text_color(color)
                .child(ui::icon(icon, px(13.), color))
                .child(text)
        };
        let header = v_flex()
            .gap(px(14.))
            .child(
                div()
                    .text_size(ty::PAGE_TITLE)
                    .font_weight(FontWeight::BOLD)
                    .text_color(lb.text)
                    .child(meeting.display_title()),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .items_center()
                    .child(pill)
                    .child(chip(
                        Ico::Calendar,
                        lokalbot_core::time::clock_time(meeting.started_at),
                        lb.supporting,
                    ))
                    .child(chip(Ico::Video, meeting.app_name.clone(), lb.supporting))
                    .child(if has_system {
                        chip(Ico::Volume2, "Mic + system".into(), lb.supporting)
                    } else {
                        chip(Ico::TriangleAlert, "Mic only".into(), lb.attention_text)
                    }),
            )
            .child(self.render_health(has_system, mic, system, cx));

        let status = self.live_notes.status.clone();
        // This version has no live transcription; notes are the live surface.
        let notes_column = v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .gap(px(6.))
            .child(
                div()
                    .text_size(ty::SECTION)
                    .font_weight(FontWeight::BOLD)
                    .text_color(lb.text)
                    .child("Meeting Notes"),
            )
            .child(
                ui::grouped_surface(cx)
                    .flex_1()
                    .min_h(px(160.))
                    .p(px(6.))
                    .child(
                        Textarea::new(&self.live_notes.input)
                            .appearance(false)
                            .h_full(),
                    ),
            )
            .child(
                h_flex()
                    .gap(px(12.))
                    .text_size(ty::SECONDARY)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(lb.text_tertiary)
                            .child("The full transcript and summary arrive after the meeting."),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(if matches!(status, NotesStatus::Failed(_)) {
                                lb.attention_text
                            } else {
                                lb.text_secondary
                            })
                            .child(match &status {
                                NotesStatus::Failed(message) => {
                                    format!("Not saved: {message}").into()
                                }
                                other => status_text(other),
                            }),
                    ),
            );

        v_flex()
            .size_full()
            .px(metric::DETAIL_PADDING)
            .pt(px(8.))
            .pb(px(20.))
            .gap(px(14.))
            .child(header)
            .child(notes_column)
            .into_any_element()
    }

    fn render_health(
        &self,
        has_system: bool,
        mic: f32,
        system: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let lb = cx.lb().clone();
        let mic_status = if mic > 0.01 {
            "Receiving audio"
        } else {
            "Waiting for audio"
        };
        let system_status = if !has_system {
            "Not attached"
        } else if system > 0.01 {
            "Receiving audio"
        } else {
            "Waiting for audio"
        };
        let line = |icon: Ico, label: String, level: Option<f32>| {
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(ui::icon(icon, px(14.), lb.text_secondary))
                .child(
                    div()
                        .w(px(220.))
                        .text_size(ty::SECONDARY)
                        .text_color(lb.text)
                        .child(label),
                )
                .when_some(level, |this, level| {
                    this.child(level_meter(level, lb.accent_text, lb.text_secondary))
                })
        };
        ui::grouped_surface(cx)
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(line(Ico::Mic, format!("Mic · {mic_status}"), Some(mic)))
            .child(line(
                Ico::Volume2,
                format!("System · {system_status}"),
                has_system.then_some(system),
            ))
    }
}

/// 20 segments, 5×10 each: teal up to the level, faint grey after.
fn level_meter(level: f32, on: Hsla, off: Hsla) -> impl IntoElement {
    let level = level.clamp(0., 1.);
    h_flex().gap(px(3.)).children((0..20).map(move |index| {
        let lit = (index as f32) / 20. < level;
        div()
            .w(px(5.))
            .h(px(10.))
            .rounded(px(2.))
            .bg(if lit { on } else { off.opacity(0.25) })
    }))
}

/// Five small bars that move with the input level (`LiveWaveform`).
pub(super) fn live_bars(level: f32, elapsed: f64, color: Hsla) -> impl IntoElement {
    let level = level.clamp(0.15, 1.);
    h_flex()
        .gap(px(1.5))
        .items_center()
        .h(px(10.))
        .children((0..5).map(move |index| {
            let wave = ((elapsed * 5. + index as f64 * 1.3).sin() * 0.5 + 0.5) as f32;
            let height = 3. + 7. * wave * level;
            div().w(px(2.5)).h(px(height)).rounded_full().bg(color)
        }))
}
