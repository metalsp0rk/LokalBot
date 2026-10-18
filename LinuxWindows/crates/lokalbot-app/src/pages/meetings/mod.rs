//! Meetings: the library list beside the meeting workspace
//! (`MeetingListView.swift`, `MeetingWorkspaceView.swift`,
//! `LiveMeetingDetailView.swift`).

mod detail;
mod dialogs;
mod list;
mod live;
pub mod model;
mod transcript;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Bounds, ClipboardItem, Context, Entity, FocusHandle,
    IntoElement, ListAlignment, ListState, ParentElement as _, Pixels, Render, SharedString,
    Styled as _, Subscription, Task, Window, anchored, canvas, deferred, div, point, px,
};
use lokalbot_core::{ActionStatus, Meeting};
use uuid::Uuid;

use self::model::MeetingDoc;
use crate::pages::Page;
use crate::state::{AppData, AppEvent, AskRequest, MeetingTab, RecordingState};
use crate::theme::metric;
use crate::ui::{self, Ico, segmented_tabs};

const TABS: [&str; 4] = ["Summary", "Transcript", "Notes", "Review"];

fn tab_index(tab: MeetingTab) -> usize {
    match tab {
        MeetingTab::Summary => 0,
        MeetingTab::Transcript => 1,
        MeetingTab::Notes => 2,
        MeetingTab::Review => 3,
    }
}

fn tab_at(index: usize) -> MeetingTab {
    match index {
        1 => MeetingTab::Transcript,
        2 => MeetingTab::Notes,
        3 => MeetingTab::Review,
        _ => MeetingTab::Summary,
    }
}

/// One row of the meeting list.
#[derive(Clone)]
pub(crate) enum Entry {
    Header(SharedString),
    Meeting { meeting: Meeting, live: bool },
}

/// Notes save state shown beside "Your notes".
#[derive(Clone, PartialEq)]
pub(crate) enum NotesStatus {
    Saved,
    Saving,
    Failed(SharedString),
}

pub(crate) struct NotesEditor {
    pub input: Entity<TextareaState>,
    pub meeting: Option<Uuid>,
    pub saved: String,
    pub status: NotesStatus,
    pub save_task: Option<Task<()>>,
    /// Set while the editor's text is replaced programmatically.
    pub loading: bool,
}

pub(crate) struct FindBar {
    pub input: Entity<InputState>,
    /// Opened with Find; on the Transcript tab the bar is always shown.
    pub open: bool,
    pub query: String,
    /// (row, byte range) of every occurrence.
    pub matches: Vec<(usize, std::ops::Range<usize>)>,
    pub current: usize,
}

pub struct MeetingsPage {
    data: Entity<AppData>,
    list_focus: FocusHandle,
    search: Entity<InputState>,
    query: String,
    entries: Vec<Entry>,
    entries_key: Option<(u64, String, Option<Uuid>, bool, usize)>,
    list_state: ListState,
    selected: Option<Uuid>,
    doc: Option<Rc<MeetingDoc>>,
    doc_loading: bool,
    load_generation: u64,
    loaded_revision: u64,
    tab: MeetingTab,
    /// Transcript moment to highlight and reveal (evidence, find).
    focus_at: Option<f64>,
    pending_reveal: bool,
    recap_expanded: bool,
    full_notes_expanded: bool,
    speakers_expanded: bool,
    transcript_state: ListState,
    find: FindBar,
    notes: NotesEditor,
    live_notes: NotesEditor,
    hover: Option<f32>,
    playhead: f32,
    speed: f32,
    page_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    tick: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl MeetingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let data = AppData::global(cx);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search meetings"));
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search Transcript"));
        let notes_input = cx.new(|cx| {
            TextareaState::new(window, cx).rows(18).placeholder(
                "Add context, reminders, or your own wording. Notes save automatically.",
            )
        });
        let live_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(12)
                .placeholder("Meeting notes")
        });

        let mut subscriptions = vec![
            cx.observe_in(&data, window, |this, _, window, cx| {
                this.on_data_changed(window, cx)
            }),
            cx.subscribe_in(&search, window, |this, input, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    this.query = input.read(cx).value().trim().to_string();
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &find_input,
                window,
                |this, input, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => {
                        this.find.query = input.read(cx).value().to_string();
                        this.update_find_matches(true, cx);
                    }
                    InputEvent::PressEnter { shift, .. } => {
                        this.move_find(if *shift { -1 } else { 1 }, cx)
                    }
                    _ => {}
                },
            ),
        ];
        subscriptions.push(cx.subscribe_in(
            &notes_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    this.schedule_notes_save(false, cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &live_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    this.schedule_notes_save(true, cx);
                }
            },
        ));

        let mut this = Self {
            data,
            list_focus: cx.focus_handle(),
            search,
            query: String::new(),
            entries: Vec::new(),
            entries_key: None,
            list_state: ListState::new(0, ListAlignment::Top, px(400.)),
            selected: None,
            doc: None,
            doc_loading: false,
            load_generation: 0,
            loaded_revision: 0,
            tab: MeetingTab::Summary,
            focus_at: None,
            pending_reveal: false,
            recap_expanded: false,
            full_notes_expanded: false,
            speakers_expanded: false,
            transcript_state: ListState::new(0, ListAlignment::Top, px(600.)),
            find: FindBar {
                input: find_input,
                open: false,
                query: String::new(),
                matches: Vec::new(),
                current: 0,
            },
            notes: NotesEditor {
                input: notes_input,
                meeting: None,
                saved: String::new(),
                status: NotesStatus::Saved,
                save_task: None,
                loading: false,
            },
            live_notes: NotesEditor {
                input: live_input,
                meeting: None,
                saved: String::new(),
                status: NotesStatus::Saved,
                save_task: None,
                loading: false,
            },
            hover: None,
            playhead: 0.,
            speed: 1.,
            page_bounds: Rc::new(Cell::new(None)),
            tick: None,
            _subscriptions: subscriptions,
        };
        this.on_data_changed(window, cx);
        this
    }

    // ------------------------------------------------------------ data flow

    /// Applies navigation requests and reloads after library changes.
    fn on_data_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.data.update(cx, |data, _| data.meeting_focus.take());
        if let Some(focus) = focus {
            self.tab = focus.tab;
            self.focus_at = focus.at;
            self.pending_reveal = focus.at.is_some();
            if self.selected != Some(focus.meeting_id) {
                self.selected = Some(focus.meeting_id);
                self.doc_loading = true;
                self.load_doc(window, cx);
            }
            if focus.tab == MeetingTab::Transcript && focus.at.is_some() {
                self.find.open = false;
            }
            cx.notify();
        }
        let revision = self.data.read(cx).revision;
        if revision != self.loaded_revision {
            self.loaded_revision = revision;
            self.ensure_selection(window, cx);
            self.load_doc(window, cx);
        }
    }

    /// Selects the newest meeting when nothing (or a deleted meeting) is selected.
    fn ensure_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let data = self.data.read(cx);
        if !data.meetings_loaded {
            return;
        }
        let live = data.recording.meeting().map(|m| m.id);
        let exists = self
            .selected
            .is_some_and(|id| data.meeting(id).is_some() || Some(id) == live);
        if !exists {
            self.selected = live.or_else(|| data.meetings.first().map(|m| m.id));
            self.doc = None;
            self.doc_loading = self.selected.is_some();
            self.load_doc(window, cx);
        }
    }

    fn selected_meeting(&self, cx: &App) -> Option<Meeting> {
        let id = self.selected?;
        let data = self.data.read(cx);
        data.recording
            .meeting()
            .filter(|m| m.id == id)
            .cloned()
            .or_else(|| data.meeting(id).cloned())
    }

    fn is_live_selected(&self, cx: &App) -> bool {
        let data = self.data.read(cx);
        matches!(&data.recording, RecordingState::Recording { meeting, .. } if Some(meeting.id) == self.selected)
    }

    pub(crate) fn select(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            return;
        }
        self.flush_notes(cx);
        self.selected = Some(id);
        self.focus_at = None;
        self.recap_expanded = false;
        self.full_notes_expanded = false;
        self.speakers_expanded = false;
        self.playhead = 0.;
        self.hover = None;
        self.doc_loading = true;
        self.load_doc(window, cx);
        cx.notify();
    }

    fn load_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(meeting) = self.selected_meeting(cx) else {
            self.doc = None;
            self.doc_loading = false;
            return;
        };
        let library = self.data.read(cx).library.clone();
        self.load_generation += 1;
        let generation = self.load_generation;
        cx.spawn_in(window, async move |this, cx| {
            let doc = cx
                .background_spawn(async move { model::load(&library, meeting) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.load_generation != generation {
                    return;
                }
                let switched = this.doc.as_ref().map(|d| d.meeting.id) != Some(doc.meeting.id);
                let doc = Rc::new(doc);
                if switched || this.transcript_state.item_count() != doc.rows.len() + 1 {
                    this.transcript_state.reset(doc.rows.len() + 1);
                }
                this.doc = Some(doc);
                this.doc_loading = false;
                if switched {
                    this.load_notes_editor(window, cx);
                }
                this.update_find_matches(false, cx);
                this.reveal_focus();
                cx.notify();
            });
        })
        .detach();
    }

    /// Scrolls the transcript to the focused moment once its rows exist.
    fn reveal_focus(&mut self) {
        if !self.pending_reveal || self.tab != MeetingTab::Transcript {
            return;
        }
        let Some(doc) = self.doc.as_ref() else { return };
        if let Some(row) = self.focus_at.and_then(|at| doc.row_at(at)) {
            self.transcript_state.scroll_to_reveal_item(row + 1);
            self.pending_reveal = false;
        }
    }

    pub(crate) fn set_tab(&mut self, tab: MeetingTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        if self.tab == MeetingTab::Transcript && tab != MeetingTab::Transcript && !self.find.open {
            self.find.matches.clear();
        }
        self.tab = tab;
        self.reveal_focus();
        cx.notify();
    }

    /// Opens the transcript at `seconds`, highlighting that passage.
    pub(crate) fn show_evidence(&mut self, seconds: f64, cx: &mut Context<Self>) {
        self.tab = MeetingTab::Transcript;
        self.focus_at = Some(seconds);
        self.pending_reveal = true;
        if let Some(doc) = self.doc.as_ref() {
            let duration = doc.meeting.duration_seconds();
            if duration > 0. {
                self.playhead = (seconds / duration).clamp(0., 1.) as f32;
            }
        }
        self.reveal_focus();
        cx.notify();
    }

    // ---------------------------------------------------------------- notes

    fn load_notes_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        let live = self.is_live_selected(cx);
        let editor = if live {
            &mut self.live_notes
        } else {
            &mut self.notes
        };
        if editor.meeting == Some(doc.meeting.id) {
            return;
        }
        editor.meeting = Some(doc.meeting.id);
        editor.saved = doc.notes.clone();
        editor.status = NotesStatus::Saved;
        editor.save_task = None;
        editor.loading = true;
        let text = doc.notes.clone();
        editor
            .input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        let editor = if live {
            &mut self.live_notes
        } else {
            &mut self.notes
        };
        editor.loading = false;
    }

    fn schedule_notes_save(&mut self, live: bool, cx: &mut Context<Self>) {
        let editor = if live {
            &mut self.live_notes
        } else {
            &mut self.notes
        };
        if editor.loading {
            return;
        }
        let text = editor.input.read(cx).value().to_string();
        if text == editor.saved {
            editor.status = NotesStatus::Saved;
            editor.save_task = None;
            cx.notify();
            return;
        }
        editor.status = NotesStatus::Saving;
        let delay = if live { 600 } else { 400 };
        editor.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = this.update(cx, |this, cx| this.save_notes(live, cx));
        }));
        cx.notify();
    }

    fn save_notes(&mut self, live: bool, cx: &mut Context<Self>) {
        let editor = if live {
            &mut self.live_notes
        } else {
            &mut self.notes
        };
        let Some(meeting_id) = editor.meeting else {
            return;
        };
        let text = editor.input.read(cx).value().to_string();
        let data = self.data.read(cx);
        let meeting = data
            .recording
            .meeting()
            .filter(|m| m.id == meeting_id)
            .cloned()
            .or_else(|| data.meeting(meeting_id).cloned());
        let Some(meeting) = meeting else { return };
        let library = data.library.clone();
        let editor = if live {
            &mut self.live_notes
        } else {
            &mut self.notes
        };
        editor.save_task = Some(cx.spawn(async move |this, cx| {
            let saved_text = text.clone();
            let result = cx
                .background_spawn(async move { library.save_notes(&meeting, &text) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let editor = if live {
                    &mut this.live_notes
                } else {
                    &mut this.notes
                };
                match result {
                    Ok(()) => {
                        editor.saved = saved_text;
                        editor.status = NotesStatus::Saved;
                    }
                    Err(err) => editor.status = NotesStatus::Failed(format!("{err:#}").into()),
                }
                cx.notify();
            });
        }));
    }

    /// Saves pending notes right away (before switching meetings).
    fn flush_notes(&mut self, cx: &mut Context<Self>) {
        for live in [false, true] {
            let editor = if live { &self.live_notes } else { &self.notes };
            if editor.status == NotesStatus::Saving {
                self.save_notes(live, cx);
            }
        }
    }

    // ----------------------------------------------------------------- find

    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.open = true;
        self.tab = MeetingTab::Transcript;
        self.find
            .input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.open = false;
        self.find.query.clear();
        self.find.matches.clear();
        self.find
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn update_find_matches(&mut self, reveal: bool, cx: &mut Context<Self>) {
        let needle = self.find.query.trim().to_lowercase();
        self.find.matches.clear();
        self.find.current = 0;
        if let (false, Some(doc)) = (needle.is_empty(), self.doc.as_ref()) {
            for (row_ix, row) in doc.rows.iter().enumerate() {
                let haystack = row.text.to_lowercase();
                // Lowercasing can change byte lengths for some scripts; only
                // highlight when the offsets still line up.
                if haystack.len() != row.text.len() {
                    if haystack.contains(&needle) {
                        self.find.matches.push((row_ix, 0..0));
                    }
                    continue;
                }
                let mut from = 0;
                while let Some(found) = haystack[from..].find(&needle) {
                    let start = from + found;
                    self.find
                        .matches
                        .push((row_ix, start..start + needle.len()));
                    from = start + needle.len().max(1);
                }
            }
        }
        if reveal && let Some((row, _)) = self.find.matches.first() {
            self.transcript_state.scroll_to_reveal_item(row + 1);
        }
        cx.notify();
    }

    pub(crate) fn move_find(&mut self, by: isize, cx: &mut Context<Self>) {
        let count = self.find.matches.len();
        if count == 0 {
            return;
        }
        self.find.current = (self.find.current as isize + by).rem_euclid(count as isize) as usize;
        let row = self.find.matches[self.find.current].0;
        self.transcript_state.scroll_to_reveal_item(row + 1);
        cx.notify();
    }

    // -------------------------------------------------------------- actions

    pub(crate) fn set_action_status(
        &mut self,
        action_id: String,
        status: ActionStatus,
        cx: &mut Context<Self>,
    ) {
        let Some(meeting_id) = self.doc.as_ref().map(|d| d.meeting.id) else {
            return;
        };
        self.data.update(cx, |data, cx| {
            data.set_action_status(meeting_id, action_id, status, cx)
        });
    }

    pub(crate) fn process(&mut self, transcribe: bool, summarize: bool, cx: &mut Context<Self>) {
        let Some(meeting_id) = self.selected else {
            return;
        };
        self.data.update(cx, |data, cx| {
            data.request(
                AppEvent::ProcessMeeting {
                    meeting_id,
                    transcribe,
                    summarize,
                },
                cx,
            )
        });
    }

    fn ask_meeting(&mut self, cx: &mut Context<Self>) {
        let Some(meeting_id) = self.selected else {
            return;
        };
        self.data.update(cx, |data, cx| {
            data.ask(
                AskRequest {
                    question: "Help me understand this meeting".into(),
                    meeting_ids: vec![meeting_id],
                    submit: false,
                },
                cx,
            )
        });
    }

    fn copy_text(&self, text: String, message: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let message = message.to_string();
        self.data
            .update(cx, |data, cx| data.show_message(message, cx));
    }

    fn export_markdown(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        let markdown = model::meeting_markdown(&doc);
        let name = format!(
            "{}.md",
            lokalbot_core::ids::slugify(&doc.meeting.display_title())
        );
        let directory = directories_home();
        let receiver = cx.prompt_for_new_path(&directory, Some(&name));
        let data = self.data.clone();
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let result = cx
                .background_spawn(async move { std::fs::write(&path, markdown) })
                .await;
            let _ = data.update(cx, |data, cx| match result {
                Ok(()) => data.show_message("Meeting exported as Markdown.", cx),
                Err(err) => data.show_error(format!("Meeting export failed: {err}"), cx),
            });
        })
        .detach();
    }

    // ------------------------------------------------------------- live tick

    /// Re-renders the live view five times a second for the timer and meters.
    fn sync_tick(&mut self, live_visible: bool, cx: &mut Context<Self>) {
        if live_visible && self.tick.is_none() {
            self.tick = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        } else if !live_visible {
            self.tick = None;
        }
    }

    // --------------------------------------------------------------- render

    fn toolbar_tabs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let page = self.page_bounds.get()?;
        let compact = page.size.width < px(900.);
        let segment = if compact { px(72.) } else { px(86.) };
        let width = segmented_tabs::width(TABS.len(), segment);
        let x = page.origin.x + page.size.width / 2. - width / 2.;
        let y = page.origin.y - metric::TOOLBAR_HEIGHT
            + (metric::TOOLBAR_HEIGHT - segmented_tabs::HEIGHT) / 2.;
        let on_select = cx.listener(|this, ix: &usize, _, cx| this.set_tab(tab_at(*ix), cx));
        let tabs = segmented_tabs::segmented_tabs(
            "meeting-tabs",
            &TABS,
            tab_index(self.tab),
            segment,
            move |ix, window, cx| on_select(&ix, window, cx),
            cx,
        );
        Some(deferred(anchored().position(point(x, y)).child(tabs)).into_any_element())
    }
}

fn directories_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

impl Page for MeetingsPage {
    fn title(&self, cx: &App) -> SharedString {
        let Some(meeting) = self.selected_meeting(cx) else {
            return "Meetings".into();
        };
        let title = meeting.display_title();
        // Keep the title clear of the centered tabs.
        let budget = self
            .page_bounds
            .get()
            .map(|b| ((f32::from(b.size.width) / 2. - 200.) / 9.).max(12.) as usize)
            .unwrap_or(40);
        if title.chars().count() > budget {
            let cut: String = title.chars().take(budget.saturating_sub(1)).collect();
            format!("{}…", cut.trim_end()).into()
        } else {
            title.into()
        }
    }

    fn toolbar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.selected.is_none() {
            return None;
        }
        if self.is_live_selected(cx) {
            let data = self.data.clone();
            let red = crate::theme::LbThemeExt::lb(&**cx).recording_text;
            return Some(
                Button::new("live-stop")
                    .outline()
                    .icon(Icon::new(Ico::CircleStop))
                    .label("Stop Recording")
                    .text_color(red)
                    .on_click(move |_, _, cx| data.update(cx, |data, cx| data.stop_recording(cx)))
                    .into_any_element(),
            );
        }
        let compact = self
            .page_bounds
            .get()
            .is_some_and(|b| b.size.width < px(900.));
        let has_summary = self.doc.as_ref().is_some_and(|d| d.has_summary());
        let has_transcript = self.doc.as_ref().is_some_and(|d| d.has_transcript());
        let page = cx.entity().downgrade();
        let export_page = page.clone();
        let more_page = page;
        let toolbar = h_flex()
            .gap_1()
            .items_center()
            .children(self.toolbar_tabs(cx))
            .child(if compact {
                Button::new("meeting-ask")
                    .ghost()
                    .icon(Icon::new(Ico::MessagesSquare))
                    .tooltip("Ask This Meeting")
                    .on_click(cx.listener(|this, _, _, cx| this.ask_meeting(cx)))
            } else {
                Button::new("meeting-ask")
                    .outline()
                    .label("Ask This Meeting")
                    .on_click(cx.listener(|this, _, _, cx| this.ask_meeting(cx)))
            })
            .child(
                Button::new("meeting-export")
                    .ghost()
                    .icon(Icon::new(Ico::Share))
                    .dropdown_caret(true)
                    .tooltip("Export meeting")
                    .dropdown_menu(move |menu, _, _| {
                        let p1 = export_page.clone();
                        let p2 = export_page.clone();
                        let p3 = export_page.clone();
                        let p4 = export_page.clone();
                        menu.item(PopupMenuItem::new("Copy Meeting as Markdown").on_click(
                            move |_, _, cx| {
                                let _ = p1.update(cx, |this, cx| {
                                    if let Some(doc) = this.doc.clone() {
                                        this.copy_text(
                                            model::meeting_markdown(&doc),
                                            "Meeting copied as Markdown.",
                                            cx,
                                        );
                                    }
                                });
                            },
                        ))
                        .item(PopupMenuItem::new("Export Meeting as Markdown…").on_click(
                            move |_, _, cx| {
                                let _ = p2.update(cx, |this, cx| this.export_markdown(cx));
                            },
                        ))
                        .item(
                            PopupMenuItem::new("Copy Summary")
                                .disabled(!has_summary)
                                .on_click(move |_, _, cx| {
                                    let _ = p3.update(cx, |this, cx| {
                                        if let Some(summary) =
                                            this.doc.as_ref().and_then(|d| d.summary.clone())
                                        {
                                            this.copy_text(summary, "Summary copied.", cx);
                                        }
                                    });
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Copy Transcript")
                                .disabled(!has_transcript)
                                .on_click(move |_, _, cx| {
                                    let _ = p4.update(cx, |this, cx| {
                                        if let Some(doc) = this.doc.clone() {
                                            this.copy_text(
                                                model::transcript_text(&doc),
                                                "Transcript copied.",
                                                cx,
                                            );
                                        }
                                    });
                                }),
                        )
                    }),
            )
            .child(
                Button::new("meeting-find")
                    .ghost()
                    .icon(Icon::new(Ico::Search))
                    .tooltip("Find on this meeting page (Ctrl+F)")
                    .on_click(cx.listener(|this, _, window, cx| this.open_find(window, cx))),
            )
            .child(
                Button::new("meeting-more")
                    .ghost()
                    .icon(Icon::new(Ico::Ellipsis))
                    .dropdown_caret(true)
                    .tooltip("More meeting actions")
                    .dropdown_menu(move |menu, _, _| {
                        let p = more_page.clone();
                        let item = |label: &'static str,
                                    f: fn(
                            &mut MeetingsPage,
                            &mut Window,
                            &mut Context<MeetingsPage>,
                        )| {
                            let p = p.clone();
                            PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                let _ = p.update(cx, |this, cx| f(this, window, cx));
                            })
                        };
                        menu.item(item("Transcribe & Summarize", |this, _, cx| {
                            this.process(true, true, cx)
                        }))
                        .item(item("Transcribe only", |this, _, cx| {
                            this.process(true, false, cx)
                        }))
                        .item(item("Summarize again", |this, _, cx| {
                            this.process(false, true, cx)
                        }))
                        .separator()
                        .item(item("Show in Folder", |this, _, cx| {
                            if let Some(doc) = this.doc.as_ref() {
                                cx.reveal_path(&doc.folder);
                            }
                        }))
                        .item(item("Copy Meeting Path", |this, _, cx| {
                            if let Some(path) =
                                this.doc.as_ref().map(|d| d.folder.display().to_string())
                            {
                                this.copy_text(path, "Meeting path copied.", cx);
                            }
                        }))
                        .separator()
                        .item(item("Delete…", |this, window, cx| {
                            if let Some(id) = this.selected {
                                dialogs::confirm_delete(this.data.clone(), vec![id], window, cx);
                            }
                        }))
                    }),
            );
        Some(toolbar.into_any_element())
    }
}

impl Render for MeetingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rebuild_entries(cx);
        let live = self.is_live_selected(cx);
        self.sync_tick(live, cx);
        if live {
            self.load_notes_editor(window, cx);
        }

        let bounds_cell = self.page_bounds.clone();
        let measure = canvas(
            move |bounds, window, _| {
                if bounds_cell.get() != Some(bounds) {
                    bounds_cell.set(Some(bounds));
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        h_flex()
            .size_full()
            .relative()
            .child(measure)
            .child(self.render_list_column(window, cx))
            .child(div().flex_1().min_w_0().h_full().child(if live {
                self.render_live(window, cx)
            } else {
                self.render_detail(window, cx)
            }))
    }
}

/// Speaker color by palette index.
pub(crate) fn speaker_color(index: usize, cx: &App) -> gpui_kit::Hsla {
    use crate::theme::LbThemeExt as _;
    cx.lb().speaker(index)
}

pub(crate) fn status_text(status: &NotesStatus) -> SharedString {
    match status {
        NotesStatus::Saved => "Saved on this PC".into(),
        NotesStatus::Saving => "Saving…".into(),
        NotesStatus::Failed(_) => "Not saved".into(),
    }
}
