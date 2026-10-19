//! The meeting list column: header with Record, search, and day groups.

use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div,
    list, px, white,
};
use lokalbot_core::Meeting;

use super::{Entry, MeetingsPage, dialogs, model};
use crate::state::{AppData, ProcessingStage, day_label};
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

impl MeetingsPage {
    /// Rebuilds the grouped rows when the library, query or recording changed.
    pub(super) fn rebuild_entries(&mut self, cx: &mut Context<Self>) {
        let data = self.data.read(cx);
        let live = data.recording.meeting().cloned();
        let key = (
            data.revision,
            self.query.clone(),
            live.as_ref().map(|m| m.id),
            data.meetings_loaded,
            data.meetings.len(),
            self.content_generation,
        );
        if self.entries_key.as_ref() == Some(&key) {
            return;
        }
        let needle = self.query.to_lowercase();
        let content = &self.content_matches;
        let matches = |m: &Meeting| {
            needle.is_empty()
                || m.display_title().to_lowercase().contains(&needle)
                || m.app_name.to_lowercase().contains(&needle)
                || content.contains(&m.id)
        };
        let mut entries = Vec::new();
        let live_id = live.as_ref().map(|m| m.id);
        if let Some(live) = live.filter(|m| matches(m)) {
            entries.push(Entry::Header("Recording Now".into()));
            entries.push(Entry::Meeting {
                meeting: Box::new(live),
                live: true,
            });
        }
        let mut current: Option<String> = None;
        for meeting in data
            .meetings
            .iter()
            .filter(|m| Some(m.id) != live_id && matches(m))
        {
            let label = day_label(meeting.started_at);
            if current.as_deref() != Some(label.as_str()) {
                entries.push(Entry::Header(label.clone().into()));
                current = Some(label);
            }
            entries.push(Entry::Meeting {
                meeting: Box::new(meeting.clone()),
                live: false,
            });
        }
        let old = self.list_state.item_count();
        self.list_state.splice(0..old, entries.len());
        self.entries = entries;
        self.entries_key = Some(key);
    }

    fn meeting_ids(&self) -> Vec<uuid::Uuid> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Meeting { meeting, .. } => Some(meeting.id),
                Entry::Header(_) => None,
            })
            .collect()
    }

    fn move_selection(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.meeting_ids();
        if ids.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|id| ids.iter().position(|m| *m == id));
        let next = match current {
            Some(ix) => (ix as isize + by).clamp(0, ids.len() as isize - 1) as usize,
            None => 0,
        };
        let id = ids[next];
        self.select(id, window, cx);
        if let Some(entry_ix) = self
            .entries
            .iter()
            .position(|e| matches!(e, Entry::Meeting { meeting, .. } if meeting.id == id))
        {
            self.list_state.scroll_to_reveal_item(entry_ix);
        }
    }

    fn on_list_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "f"
            && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
        {
            self.open_find(window, cx);
            cx.stop_propagation();
            return;
        }
        match event.keystroke.key.as_str() {
            "space" => self.toggle_playback(cx),
            "left" => self.seek_by(-5., cx),
            "right" => self.seek_by(5., cx),
            "up" => self.move_selection(-1, window, cx),
            "down" => self.move_selection(1, window, cx),
            "delete" => {
                if let Some(id) = self.selected {
                    dialogs::confirm_delete(self.data.clone(), vec![id], window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_list_column(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let lb = cx.lb().clone();
        let data = self.data.read(cx);
        let count = data
            .meetings
            .iter()
            .filter(|m| !m.is_merged_source())
            .count();
        let recording = data.recording.is_active();
        let loaded = data.meetings_loaded;
        let library_empty = data.meetings.is_empty() && data.recording.meeting().is_none();
        let failed: Vec<uuid::Uuid> = data
            .processing
            .iter()
            .filter(|(_, stage)| matches!(stage, ProcessingStage::Failed(_)))
            .map(|(id, _)| *id)
            .collect();
        let data_entity = self.data.clone();

        let record_button = {
            let tint = lb.recording;
            let text = lb.recording_text;
            let data_entity = data_entity.clone();
            h_flex()
                .id("meetings-record")
                .flex_none()
                .h(px(28.))
                .px(px(10.))
                .gap(px(6.))
                .items_center()
                .rounded(px(7.))
                .bg(tint.opacity(0.10))
                .border_1()
                .border_color(tint.opacity(0.32))
                .hover(move |s| s.bg(tint.opacity(0.18)))
                .cursor_pointer()
                .child(if recording {
                    div()
                        .size(px(11.))
                        .rounded(px(2.))
                        .bg(tint)
                        .into_any_element()
                } else {
                    div()
                        .size(px(13.))
                        .rounded_full()
                        .bg(tint)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(5.)).rounded_full().bg(white()))
                        .into_any_element()
                })
                .child(
                    div()
                        .text_size(ty::SECONDARY)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(text)
                        .child(if recording {
                            "Stop Recording"
                        } else {
                            "Record"
                        }),
                )
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(if recording {
                        "Stop Recording"
                    } else {
                        "Record Now"
                    })
                    .build(window, cx)
                })
                .on_click(move |_, _, cx| {
                    data_entity.update(cx, |data, cx| data.toggle_recording(cx))
                })
        };

        let header = v_flex()
            .flex_none()
            .p(metric::PANEL_PADDING)
            .gap(px(10.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_start()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(ty::COLUMN_TITLE)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(lb.text)
                                    .child("Meetings"),
                            )
                            .child(
                                div()
                                    .text_size(ty::SECONDARY)
                                    .text_color(lb.text_secondary)
                                    .child(format!(
                                        "{count} {}",
                                        if count == 1 { "meeting" } else { "meetings" }
                                    )),
                            ),
                    )
                    .child(record_button),
            )
            .child(Input::new(&self.search).small().cleanable(true))
            .when(!failed.is_empty(), |this| {
                let data_entity = data_entity.clone();
                let failed = failed.clone();
                this.child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .text_size(ty::SECONDARY)
                        .child(ui::icon(Ico::TriangleAlert, px(14.), lb.attention_text))
                        .child(
                            div()
                                .flex_1()
                                .text_color(lb.attention_text)
                                .child(format!("{} failed", failed.len())),
                        )
                        .child(ui::link(
                            "meetings-retry-failed",
                            "Retry",
                            move |_, cx| {
                                data_entity.update(cx, |data, cx| {
                                    for id in &failed {
                                        data.request(
                                            crate::state::AppEvent::ProcessMeeting {
                                                meeting_id: *id,
                                                transcribe: true,
                                                summarize: true,
                                            },
                                            cx,
                                        );
                                    }
                                })
                            },
                            cx,
                        )),
                )
            });

        let body: AnyElement = if !loaded {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(Spinner::new().small())
                .child(
                    div()
                        .text_size(ty::BODY)
                        .text_color(lb.text_secondary)
                        .child("Loading your meeting library…"),
                )
                .into_any_element()
        } else if self.entries.is_empty() {
            let (title, message) = if library_empty && self.query.is_empty() {
                (
                    "No meetings yet",
                    "LokalBot detects meeting apps automatically, or start a recording now.",
                )
            } else {
                (
                    "No matching meetings",
                    "Try different words, a meeting title, or app.",
                )
            };
            v_flex()
                .flex_1()
                .justify_center()
                .child(ui::empty_state(Ico::AudioLines, title, message, cx).w_full())
                .into_any_element()
        } else {
            div()
                .id("meetings-list")
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    list(
                        self.list_state.clone(),
                        cx.processor(|this, ix: usize, window, cx| {
                            this.render_entry(ix, window, cx)
                        }),
                    )
                    .size_full(),
                )
                .vertical_scrollbar(&self.list_state)
                .into_any_element()
        };

        v_flex()
            .id("meetings-column")
            .w(metric::CONTENT_COLUMN_WIDTH)
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(lb.separator)
            .track_focus(&self.list_focus)
            .on_key_down(cx.listener(Self::on_list_key))
            .child(header)
            .child(body)
    }

    fn render_entry(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lb = cx.lb().clone();
        let Some(entry) = self.entries.get(ix).cloned() else {
            return div().into_any_element();
        };
        match entry {
            Entry::Header(label) => div()
                .px(px(18.))
                .pt(px(if ix == 0 { 4. } else { 12. }))
                .pb(px(5.))
                .text_size(ty::CAPTION)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(lb.text_secondary)
                .child(label)
                .into_any_element(),
            Entry::Meeting { meeting, live } => {
                let selected = self.selected == Some(meeting.id);
                let next_selected = matches!(self.entries.get(ix + 1), Some(Entry::Meeting { meeting: next, .. }) if Some(next.id) == self.selected);
                let next_is_meeting =
                    matches!(self.entries.get(ix + 1), Some(Entry::Meeting { .. }));
                let stage = self.data.read(cx).processing.get(&meeting.id).cloned();
                let live_level = match &self.data.read(cx).recording {
                    crate::state::RecordingState::Recording {
                        started,
                        mic_level,
                        system_level,
                        ..
                    } if live => Some((
                        mic_level.max(*system_level),
                        started.elapsed().as_secs_f64(),
                    )),
                    _ => None,
                };
                let id = meeting.id;
                let detail = model::row_detail(&meeting, live);
                let hover = lb.hover;
                let data = self.data.clone();
                let row = h_flex()
                    .id(("meeting-row", ix))
                    .w_full()
                    .px(px(8.))
                    .py(px(7.))
                    .gap(px(10.))
                    .items_center()
                    .rounded(metric::ROW_RADIUS)
                    .cursor_pointer()
                    .when(selected, |this| this.bg(lb.sidebar_selection))
                    .when(!selected, |this| this.hover(move |s| s.bg(hover)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                h_flex()
                                    .gap(px(6.))
                                    .items_center()
                                    .when(live, |this| {
                                        this.child(ui::status_dot(lb.recording, px(9.)))
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(ty::BODY)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(lb.text)
                                            .child(meeting.display_title()),
                                    )
                                    .when_some(live_level, |this, (level, elapsed)| {
                                        this.child(super::live::live_bars(
                                            level,
                                            elapsed,
                                            lb.recording,
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(ty::SECONDARY)
                                    .text_color(if selected { lb.text } else { lb.supporting })
                                    .child(detail),
                            ),
                    )
                    .when_some(stage.filter(|_| !live), |this, stage| {
                        this.child(stage_badge(&stage, id, data, cx))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.list_focus.focus(window, cx);
                        }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| this.select(id, window, cx)))
                    .context_menu({
                        let data = self.data.clone();
                        move |menu, _, _| {
                            let data = data.clone();
                            menu.item(PopupMenuItem::new("Delete meeting…").on_click(
                                move |_, window, cx| {
                                    dialogs::confirm_delete(data.clone(), vec![id], window, cx);
                                },
                            ))
                        }
                    });
                v_flex()
                    .w_full()
                    .px(px(10.))
                    .child(row)
                    .when(next_is_meeting && !selected && !next_selected, |this| {
                        this.child(div().mx(px(8.)).h(px(1.)).bg(lb.separator))
                    })
                    .when(!(next_is_meeting && !selected && !next_selected), |this| {
                        this.child(div().h(px(1.)))
                    })
                    .into_any_element()
            }
        }
    }
}

/// Processing status at the trailing edge of a row.
fn stage_badge(
    stage: &ProcessingStage,
    meeting_id: uuid::Uuid,
    data: gpui_kit::Entity<AppData>,
    cx: &mut Context<MeetingsPage>,
) -> AnyElement {
    let lb = cx.lb().clone();
    let retry = move |label: &'static str, id: &'static str| {
        let data = data.clone();
        div()
            .id(id)
            .text_size(ty::SECONDARY)
            .text_color(lb.accent_text)
            .cursor_pointer()
            .hover(|s| s.underline())
            .child(label)
            .on_click(move |_, _, cx| {
                data.update(cx, |data, cx| {
                    data.request(
                        crate::state::AppEvent::ProcessMeeting {
                            meeting_id,
                            transcribe: true,
                            summarize: true,
                        },
                        cx,
                    )
                })
            })
    };
    match stage {
        ProcessingStage::Failed(reason) => v_flex()
            .flex_none()
            .items_end()
            .gap(px(2.))
            .child(
                h_flex()
                    .gap(px(4.))
                    .items_center()
                    .text_size(ty::SECONDARY)
                    .text_color(lb.attention_text)
                    .child(ui::icon(Ico::TriangleAlert, px(13.), lb.attention_text))
                    .child("Failed"),
            )
            .child(retry("Retry", "row-retry"))
            .id(("row-failed", meeting_id.as_u128() as u64))
            .tooltip({
                let reason = reason.clone();
                move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(reason.clone()).build(window, cx)
                }
            })
            .into_any_element(),
        ProcessingStage::WaitingForModels => v_flex()
            .flex_none()
            .items_end()
            .gap(px(2.))
            .child(
                h_flex()
                    .gap(px(4.))
                    .items_center()
                    .text_size(ty::SECONDARY)
                    .text_color(lb.text_secondary)
                    .child(ui::icon(Ico::CircleArrowDown, px(13.), lb.text_secondary))
                    .child("Waiting for models"),
            )
            .child(retry("Download & process", "row-download"))
            .into_any_element(),
        other => h_flex()
            .flex_none()
            .gap(px(5.))
            .items_center()
            .text_size(ty::CAPTION)
            .text_color(lb.text_secondary)
            .child(Spinner::new().xsmall())
            .child(row_stage_label(other))
            .into_any_element(),
    }
}

/// `ProcessingPipeline.Stage.rowLabel`.
fn row_stage_label(stage: &ProcessingStage) -> String {
    match stage {
        ProcessingStage::Queued => "Queued".into(),
        ProcessingStage::PreparingModel(name) => {
            let lower = name.to_lowercase();
            if lower.contains("speaker") {
                "Preparing speaker model".into()
            } else if lower.contains("think") || lower.contains("summar") {
                "Preparing summary model".into()
            } else {
                "Preparing speech model".into()
            }
        }
        ProcessingStage::Transcribing => "Transcribing".into(),
        ProcessingStage::IdentifyingSpeakers => "Identifying speakers".into(),
        ProcessingStage::Summarizing => "Summarizing".into(),
        ProcessingStage::WaitingForModels => "Waiting for models".into(),
        ProcessingStage::Failed(_) => "Failed".into(),
    }
}
