//! Sheets of the meeting workspace: delete confirmation, action correction
//! and naming a speaker.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div, px,
};
use lokalbot_core::{ActionState, ActionStatus, Library, Meeting, Transcript};
use uuid::Uuid;

use super::model::{MeetingAction, ReviewSpeaker};
use crate::state::AppData;
use crate::theme::{LbThemeExt as _, ty};

/// "Delete N meeting(s)?" with the macOS wording.
pub(crate) fn confirm_delete(
    data: Entity<AppData>,
    ids: Vec<Uuid>,
    window: &mut Window,
    cx: &mut App,
) {
    if ids.is_empty() {
        return;
    }
    let title: SharedString = if ids.len() == 1 {
        "Delete 1 meeting?".into()
    } else {
        format!("Delete {} meetings?", ids.len()).into()
    };
    window.open_dialog(cx, move |dialog, _, cx| {
        let lb = cx.lb().clone();
        let ids = ids.clone();
        let data = data.clone();
        dialog
            .w(px(440.))
            .title(
                div()
                    .text_size(ty::COLUMN_TITLE)
                    .font_weight(FontWeight::BOLD)
                    .child(title.clone()),
            )
            .child(
                div()
                    .text_size(ty::BODY)
                    .text_color(lb.text_secondary)
                    .child("This permanently deletes the audio, transcript and summary files."),
            )
            .footer(
                h_flex()
                    .w_full()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("delete-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new("delete-confirm")
                            .danger()
                            .label("Delete (removes recordings & transcripts)")
                            .on_click(move |_, window, cx| {
                                let ids = ids.clone();
                                data.update(cx, |data, cx| {
                                    for id in ids {
                                        data.delete_meeting(id, cx);
                                    }
                                });
                                window.close_dialog(cx);
                            }),
                    ),
            )
    });
}

// ------------------------------------------------------------ correction

/// Saves a correction beside the extracted action (`outcome-state.json`);
/// the original text, owner, due phrase and citations stay untouched.
///
/// `owner`/`due`: `None` keeps the current value, `Some(None)` clears it.
pub(crate) fn save_correction(
    library: &Library,
    meeting: &Meeting,
    action_id: &str,
    text: Option<String>,
    owner: Option<Option<String>>,
    due: Option<Option<String>>,
) -> anyhow::Result<()> {
    let mut state = library.outcome_state(meeting)?;
    let now = chrono::Utc::now();
    let entry = state
        .actions
        .entry(action_id.to_string())
        .or_insert_with(|| {
            let mut fresh = ActionState::new(ActionStatus::Open);
            fresh.user_edited = false;
            fresh
        });
    if let Some(text) = text {
        entry.text_correction = Some(text);
        entry.text_corrected_at = Some(now);
    }
    if let Some(owner) = owner {
        entry.owner_was_cleared = owner.is_none();
        entry.owner_override = owner;
        entry.owner_corrected_at = Some(now);
    }
    if let Some(due) = due {
        entry.due_was_cleared = due.is_none();
        entry.due_override = due;
        entry.due_corrected_at = Some(now);
    }
    entry.updated_at = now;
    entry.user_edited = true;
    library.save_outcome_state(meeting, &state)
}

pub(crate) struct CorrectionSheet {
    data: Entity<AppData>,
    meeting: Meeting,
    action: MeetingAction,
    suggestions: Vec<String>,
    text: Entity<InputState>,
    owner: Entity<InputState>,
    due: Entity<InputState>,
    error: Option<SharedString>,
}

impl CorrectionSheet {
    pub(crate) fn open(
        data: Entity<AppData>,
        meeting: Meeting,
        action: MeetingAction,
        speakers: &[ReviewSpeaker],
        window: &mut Window,
        cx: &mut App,
    ) {
        let suggestions: Vec<String> = {
            let mut seen = Vec::<String>::new();
            for speaker in speakers.iter().filter(|s| s.is_named && !s.is_user) {
                if !seen.iter().any(|n| n.eq_ignore_ascii_case(&speaker.name)) {
                    seen.push(speaker.name.clone());
                }
            }
            seen
        };
        let sheet = cx.new(|cx| {
            let text = cx.new(|cx| InputState::new(window, cx).default_value(action.text.clone()));
            let owner = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Owner")
                    .default_value(action.owner.clone().unwrap_or_default())
            });
            let due = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Due date as agreed")
                    .default_value(action.due.clone().unwrap_or_default())
            });
            Self {
                data,
                meeting,
                action,
                suggestions,
                text,
                owner,
                due,
                error: None,
            }
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(460.)).close_button(false).child(sheet.clone())
        });
    }

    fn set_owner(&mut self, owner: &str, window: &mut Window, cx: &mut Context<Self>) {
        let owner = owner.to_string();
        self.owner
            .update(cx, |input, cx| input.set_value(owner, window, cx));
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text.read(cx).value().trim().to_string();
        let owner = self.owner.read(cx).value().trim().to_string();
        let due = self.due.read(cx).value().trim().to_string();
        let text_change =
            (!text.is_empty() && text != self.action.original_text && text != self.action.text)
                .then_some(text);
        let owner_change = (owner != self.action.owner.clone().unwrap_or_default())
            .then(|| (!owner.is_empty() && owner != "Unassigned").then_some(owner));
        let due_change = (due != self.action.due.clone().unwrap_or_default())
            .then(|| (!due.is_empty()).then_some(due));
        if text_change.is_none() && owner_change.is_none() && due_change.is_none() {
            window.close_dialog(cx);
            return;
        }
        let library = self.data.read(cx).library.clone();
        let meeting = self.meeting.clone();
        let action_id = self.action.id.clone();
        let data = self.data.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    save_correction(
                        &library,
                        &meeting,
                        &action_id,
                        text_change,
                        owner_change,
                        due_change,
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    data.update(cx, |data, cx| data.mark_library_changed(cx));
                    window.close_dialog(cx);
                }
                Err(err) => {
                    this.error = Some(format!("The correction could not be saved. {err:#}").into());
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for CorrectionSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lb = cx.lb().clone();
        let owner_value = self.owner.read(cx).value().to_string();
        let owner_label: SharedString = if owner_value.trim().is_empty() {
            "Unassigned".into()
        } else if owner_value.trim().eq_ignore_ascii_case("me") {
            "You".into()
        } else {
            owner_value.into()
        };
        let suggestions = self.suggestions.clone();
        let sheet = cx.entity().downgrade();
        let label = |text: &'static str| {
            div()
                .text_size(ty::SECONDARY)
                .font_weight(FontWeight::MEDIUM)
                .text_color(lb.text_secondary)
                .child(text)
        };
        v_flex()
            .gap(px(14.))
            .child(
                div()
                    .text_size(ty::QUESTION)
                    .font_weight(FontWeight::BOLD)
                    .text_color(lb.text)
                    .child("Correct action details"),
            )
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(label("Corrected wording"))
                    .child(Input::new(&self.text)),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().flex_1().child(label("Owner")))
                    .child(
                        Button::new("correction-owner-menu")
                            .outline()
                            .small()
                            .label(owner_label)
                            .dropdown_caret(true)
                            .dropdown_menu(move |menu, _, _| {
                                let pick = |name: &str| {
                                    let sheet = sheet.clone();
                                    let value = name.to_string();
                                    move |_: &gpui_kit::ClickEvent,
                                          window: &mut Window,
                                          cx: &mut App| {
                                        let _ = sheet.update(cx, |this, cx| {
                                            this.set_owner(&value, window, cx)
                                        });
                                    }
                                };
                                let mut menu =
                                    menu.item(PopupMenuItem::new("You").on_click(pick("Me")));
                                for name in &suggestions {
                                    menu = menu.item(
                                        PopupMenuItem::new(name.clone()).on_click(pick(name)),
                                    );
                                }
                                menu.item(
                                    PopupMenuItem::new("Unresolved speaker")
                                        .on_click(pick("Unresolved speaker")),
                                )
                                .item(PopupMenuItem::new("Unassigned").on_click(pick("")))
                            }),
                    ),
            )
            .child(Input::new(&self.owner))
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(label("Due date as agreed"))
                    .child(Input::new(&self.due)),
            )
            .child(
                div()
                    .text_size(ty::SECONDARY)
                    .text_color(lb.text_secondary)
                    .child("This correction is stored separately from the extracted source."),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .text_size(ty::SECONDARY)
                        .text_color(lb.attention_text)
                        .child(error),
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("correction-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new("correction-save")
                            .primary()
                            .label("Save correction")
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

// --------------------------------------------------------- name speaker

pub(crate) struct NameSpeakerSheet {
    data: Entity<AppData>,
    meeting: Meeting,
    transcript: Transcript,
    speaker: ReviewSpeaker,
    name: Entity<InputState>,
    error: Option<SharedString>,
}

impl NameSpeakerSheet {
    pub(crate) fn open(
        data: Entity<AppData>,
        meeting: Meeting,
        transcript: Transcript,
        speaker: ReviewSpeaker,
        window: &mut Window,
        cx: &mut App,
    ) {
        let current = transcript
            .speaker_aliases
            .get(&speaker.key)
            .cloned()
            .unwrap_or_default();
        let sheet = cx.new(|cx| {
            let name = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Name this speaker")
                    .default_value(current)
            });
            Self {
                data,
                meeting,
                transcript,
                speaker,
                name,
                error: None,
            }
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(560.)).close_button(false).child(sheet.clone())
        });
    }

    fn save(&mut self, alias: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let mut transcript = self.transcript.clone();
        match alias.filter(|a| !a.trim().is_empty()) {
            Some(alias) => {
                transcript
                    .speaker_aliases
                    .insert(self.speaker.key.clone(), alias.trim().to_string());
            }
            None => {
                transcript.speaker_aliases.remove(&self.speaker.key);
            }
        }
        let library = self.data.read(cx).library.clone();
        let meeting = self.meeting.clone();
        let data = self.data.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { library.save_transcript(&meeting, &transcript) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    data.update(cx, |data, cx| data.mark_library_changed(cx));
                    window.close_dialog(cx);
                }
                Err(err) => {
                    this.error = Some(format!("The name could not be saved. {err:#}").into());
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for NameSpeakerSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lb = cx.lb().clone();
        v_flex()
            .gap(px(14.))
            .child(div().text_size(ty::QUESTION).font_weight(FontWeight::BOLD).text_color(lb.text).child("Name Speaker"))
            .child(match &self.speaker.sample {
                Some((start, text)) => v_flex()
                    .gap(px(4.))
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(lb.group_fill)
                    .border_1()
                    .border_color(lb.group_stroke)
                    .child(div().text_size(ty::BODY).text_color(lb.text).child(format!("“{text}”")))
                    .child(
                        div()
                            .text_size(ty::CAPTION)
                            .text_color(lb.text_secondary)
                            .child(lokalbot_core::time::stamp(*start)),
                    )
                    .into_any_element(),
                None => div()
                    .text_size(ty::SECONDARY)
                    .text_color(lb.text_secondary)
                    .child("No clear voice excerpt available. Check the transcript before confirming this speaker.")
                    .into_any_element(),
            })
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(div().text_size(ty::SECONDARY).text_color(lb.text_secondary).child("Name"))
                    .child(Input::new(&self.name)),
            )
            .child(
                h_flex()
                    .gap(px(6.))
                    .items_start()
                    .child(crate::ui::icon(crate::ui::Ico::ShieldCheck, px(14.), lb.accent_text))
                    .child(
                        div()
                            .flex_1()
                            .text_size(ty::CAPTION)
                            .text_color(lb.text_secondary)
                            .child("Names are saved with this meeting."),
                    ),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_size(ty::SECONDARY).text_color(lb.attention_text).child(error))
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("speaker-unidentified")
                            .ghost()
                            .label("Leave Unidentified")
                            .on_click(cx.listener(|this, _, window, cx| this.save(None, window, cx))),
                    )
                    .child(div().flex_1())
                    .child(Button::new("speaker-cancel").outline().label("Cancel").on_click(|_, window, cx| {
                        window.close_dialog(cx);
                    }))
                    .child(
                        Button::new("speaker-save").primary().label("Save").on_click(cx.listener(|this, _, window, cx| {
                            let name = this.name.read(cx).value().to_string();
                            this.save(Some(name), window, cx)
                        })),
                    ),
            )
    }
}
