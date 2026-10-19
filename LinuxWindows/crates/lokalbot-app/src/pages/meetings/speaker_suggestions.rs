//! Suggested names in Name Speaker (`MeetingSpeakerRenameSheet`): the
//! meeting's calendar guests and name hints. Choosing a guest keeps its
//! opaque identity with the name (`speakerCalendarIdentityIDs`), so same-named
//! guests stay apart; attendance alone never names a voice.

use std::collections::HashSet;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use lokalbot_core::calendar::{SpeakerSuggestion, SuggestionSource};
use lokalbot_core::{Meeting, Transcript};

use super::dialogs::NameSpeakerSheet;
use crate::i18n::h_flex;
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{self, Ico};

/// Longer lists get a search field.
const SEARCH_THRESHOLD: usize = 3;

pub(crate) struct SpeakerSuggestions {
    pub choices: Vec<SpeakerSuggestion>,
    /// The calendar guest chosen for this voice.
    pub selected: Option<String>,
    /// Guests already given to another voice in this meeting.
    assigned_elsewhere: HashSet<String>,
    pub query: Option<Entity<InputState>>,
}

impl SpeakerSuggestions {
    pub fn new(
        meeting: &Meeting,
        transcript: &Transcript,
        speaker_key: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let guests = lokalbot_core::calendar::resolved_meeting_identities(meeting);
        let hints = meeting.participant_name_hints.clone().unwrap_or_default();
        let choices = SpeakerSuggestion::choices(&guests, &hints);
        let selected = transcript
            .speaker_calendar_identity_ids
            .get(speaker_key)
            .cloned();
        let assigned_elsewhere = transcript
            .speaker_calendar_identity_ids
            .iter()
            .filter(|(key, _)| key.as_str() != speaker_key)
            .map(|(_, id)| id.clone())
            .collect();
        let query = (choices.len() > SEARCH_THRESHOLD).then(|| {
            cx.new(|cx| {
                InputState::new(window, cx).placeholder(crate::i18n::placeholder(
                    "Search suggested names",
                    window,
                    cx,
                ))
            })
        });
        Self {
            choices,
            selected,
            assigned_elsewhere,
            query,
        }
    }

    /// The calendar identity to save with `name`: the chosen guest's, while
    /// a guest is chosen.
    pub fn identity_for_save(&self) -> Option<String> {
        self.selected.clone()
    }

    pub fn render(&self, name: &str, cx: &mut Context<NameSpeakerSheet>) -> Option<AnyElement> {
        if self.choices.is_empty() {
            return None;
        }
        let lb = cx.lb().clone();
        let query = self
            .query
            .as_ref()
            .map(|q| q.read(cx).value().to_string())
            .unwrap_or_default();
        let visible: Vec<(usize, &SpeakerSuggestion)> = self
            .choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| query.trim().is_empty() || choice.matches(&query))
            .collect();
        let mut list = v_flex().gap(px(6.));
        for (index, choice) in &visible {
            list = list.child(self.row(*index, choice, name, cx));
        }
        Some(
            v_flex()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(ty::BODY)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(lb.text)
                        .child(lokalbot_i18n::tr("Suggested Names")),
                )
                .child(
                    div()
                        .text_size(ty::SECONDARY)
                        .text_color(lb.text_secondary)
                        .child(crate::ui::text::paragraph(lokalbot_i18n::tr("Listen, choose a name, then save. These sources suggest names; attendance does not identify a voice."))),
                )
                .when_some(self.query.clone(), |this, query| this.child(Input::new(&query)))
                .child(
                    div()
                        .id("speaker-suggestions")
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .child(list),
                )
                .when(visible.is_empty(), |this| {
                    this.child(
                        div()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(lokalbot_i18n::tr("No suggested names match this search.")),
                    )
                })
                .when(self.choices.iter().any(|c| c.calendar.is_some()), |this| {
                    this.child(
                        div()
                            .text_size(ty::CAPTION)
                            .text_color(lb.text_secondary)
                            .child(crate::ui::text::paragraph(lokalbot_i18n::tr("Email addresses stay in this meeting's local metadata and help distinguish guests."))),
                    )
                })
                .into_any_element(),
        )
    }

    fn row(
        &self,
        index: usize,
        choice: &SpeakerSuggestion,
        name: &str,
        cx: &mut Context<NameSpeakerSheet>,
    ) -> AnyElement {
        let lb = cx.lb().clone();
        let selected = match &self.selected {
            Some(id) => choice
                .calendar
                .as_ref()
                .is_some_and(|guest| &guest.id == id),
            None => {
                choice.calendar.is_none()
                    && choice.name.as_deref().map(SpeakerSuggestion::name_key)
                        == Some(SpeakerSuggestion::name_key(name))
            }
        };
        let elsewhere = choice
            .calendar
            .as_ref()
            .is_some_and(|guest| self.assigned_elsewhere.contains(&guest.id));
        let title: SharedString = match &choice.name {
            Some(name) => name.clone().into(),
            None => lokalbot_i18n::tr("Enter a name for this guest").into(),
        };
        let badges = choice.sources().into_iter().map(|source| {
            let label = match source {
                SuggestionSource::Calendar => lokalbot_i18n::tr("Calendar"),
                SuggestionSource::NameFromEmail => lokalbot_i18n::tr("Name from email"),
                SuggestionSource::SuggestedName => lokalbot_i18n::tr("Suggested name"),
            };
            div()
                .px(px(6.))
                .py(px(1.))
                .rounded(px(5.))
                .bg(lb.accent_fill.opacity(0.14))
                .text_size(ty::CAPTION)
                .text_color(lb.accent_text)
                .child(label)
        });
        div()
            .id(SharedString::from(format!("speaker-suggestion-{index}")))
            .cursor_pointer()
            .p(px(10.))
            .rounded(px(9.))
            .border_1()
            .border_color(if selected {
                lb.accent_text.opacity(0.6)
            } else {
                lb.group_stroke
            })
            .bg(if selected {
                lb.accent_fill.opacity(0.10)
            } else {
                lb.group_fill
            })
            .child(
                v_flex()
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(ui::icon(
                                if selected {
                                    Ico::CircleCheckBig
                                } else {
                                    Ico::User
                                },
                                px(15.),
                                if selected {
                                    lb.accent_text
                                } else {
                                    lb.text_secondary
                                },
                            ))
                            .child(div().text_size(ty::BODY).text_color(lb.text).child(title)),
                    )
                    .child(h_flex().gap(px(6.)).flex_wrap().children(badges))
                    .when_some(
                        choice
                            .calendar
                            .as_ref()
                            .and_then(|g| g.email_address.clone()),
                        |this, email| {
                            this.child(
                                div()
                                    .text_size(ty::SECONDARY)
                                    .text_color(lb.text_secondary)
                                    .child(email),
                            )
                        },
                    )
                    .when(elsewhere, |this| {
                        this.child(
                            div()
                                .text_size(ty::SECONDARY)
                                .text_color(lb.text_secondary)
                                .child(lokalbot_i18n::tr("Also assigned to another voice")),
                        )
                    }),
            )
            .on_click(
                cx.listener(move |sheet, _, window, cx| sheet.choose_suggestion(index, window, cx)),
            )
            .into_any_element()
    }
}
