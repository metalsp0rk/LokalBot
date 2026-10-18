//! Timeline: one day at a time, with its digest, open actions, the day's
//! overview and work sessions, and a context panel for the selected session
//! or meeting (`CaptureView.swift`, `CaptureDetailView.swift`).

mod context;

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use chrono::{Days, Local, NaiveDate};
use gpui_kit::component::button::Button;
use gpui_kit::component::calendar::{Calendar, CalendarEvent, CalendarState, Date, Matcher};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Disableable as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, canvas, div,
    px,
};
use lokalbot_core::{ActionThread, DayActivity, DayDigest, Meeting, ScreenMoment, WorkSession};
use uuid::Uuid;

use self::context::MeetingPreview;
use crate::pages::Page;
use crate::pages::today::digest::{self, DigestMode};
use crate::pages::today::threads;
use crate::state::{AppData, AppEvent};
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

/// Below this page width, selections open in a drawer over a single column.
const DRAWER_BREAKPOINT: f32 = 820.;
const DAY_MIN_WIDTH: f32 = 440.;

/// One day's data, loaded off the main thread.
pub(crate) struct DayModel {
    pub activity: DayActivity,
    pub digest: Option<DayDigest>,
    pub attention: Vec<ActionThread>,
    pub meetings: Vec<Meeting>,
    pub moments: Vec<ScreenMoment>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Selection {
    Session(String),
    Meeting(Uuid),
    /// "Browse Raw Capture": every activity block and screen moment.
    RawCapture,
}

pub struct TimelinePage {
    data: Entity<AppData>,
    day: NaiveDate,
    model: Option<Rc<DayModel>>,
    load_generation: u64,
    loaded_key: Option<(u64, NaiveDate)>,
    selection: Option<Selection>,
    drawer_open: bool,
    meeting_preview: Option<MeetingPreview>,
    expanded_tasks: HashSet<usize>,
    expanded_titles: HashSet<String>,
    session_details_open: bool,
    calendar: Entity<CalendarState>,
    calendar_open: bool,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _subscriptions: Vec<Subscription>,
}

impl TimelinePage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let data = AppData::global(cx);
        let today = Local::now().date_naive();
        let calendar = cx.new(|cx| {
            let mut state = CalendarState::new(window, cx).disabled_matcher(Matcher::custom(
                move |date: &NaiveDate| *date > Local::now().date_naive(),
            ));
            state.set_date(today, window, cx);
            state
        });
        let subscriptions = vec![
            cx.observe(&data, |this, _, cx| {
                this.on_data_changed(cx);
                cx.notify();
            }),
            cx.subscribe(&calendar, |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(Some(day))) = event else {
                    return;
                };
                this.calendar_open = false;
                this.set_day(*day, cx);
            }),
        ];
        let mut this = Self {
            data,
            day: today,
            model: None,
            load_generation: 0,
            loaded_key: None,
            selection: None,
            drawer_open: false,
            meeting_preview: None,
            expanded_tasks: HashSet::new(),
            expanded_titles: HashSet::new(),
            session_details_open: false,
            calendar,
            calendar_open: false,
            bounds: Rc::new(Cell::new(None)),
            _subscriptions: subscriptions,
        };
        this.on_data_changed(cx);
        this
    }

    fn on_data_changed(&mut self, cx: &mut Context<Self>) {
        let focus = self.data.update(cx, |data, _| data.timeline_focus.take());
        if let Some(focus) = focus {
            self.set_day(focus.day, cx);
            if let Some(session) = focus.session_id {
                self.select(Some(Selection::Session(session)), cx);
            }
        }
        let data = self.data.read(cx);
        if !data.meetings_loaded {
            return;
        }
        let key = (data.revision, self.day);
        if self.loaded_key != Some(key) {
            self.loaded_key = Some(key);
            self.reload(cx);
        }
    }

    fn set_day(&mut self, day: NaiveDate, cx: &mut Context<Self>) {
        let day = day.min(Local::now().date_naive());
        if day == self.day {
            return;
        }
        self.day = day;
        self.selection = None;
        self.meeting_preview = None;
        self.drawer_open = false;
        self.expanded_tasks.clear();
        self.loaded_key = None;
        self.reload(cx);
        cx.notify();
    }

    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        let library = self.data.read(cx).library.clone();
        let day = self.day;
        self.load_generation += 1;
        let generation = self.load_generation;
        cx.spawn(async move |this, cx| {
            let model = cx
                .background_spawn(async move {
                    let (start, end) = lokalbot_core::time::local_day_bounds(day);
                    DayModel {
                        activity: library.day_activity(day).unwrap_or_else(|err| {
                            log::warn!("day activity for {day}: {err:#}");
                            DayActivity::default()
                        }),
                        digest: library.day_digest(day).unwrap_or_default(),
                        attention: library.needs_attention(Some(day), 3).unwrap_or_default(),
                        meetings: library
                            .list_meetings()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|m| lokalbot_core::time::local_day(m.started_at) == day)
                            .collect(),
                        moments: library.screen_moments(start, end).unwrap_or_default(),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.load_generation == generation {
                    this.model = Some(Rc::new(model));
                    this.refresh_meeting_preview(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn select(&mut self, selection: Option<Selection>, cx: &mut Context<Self>) {
        let toggled_off = selection.is_some() && selection == self.selection;
        self.selection = if toggled_off { None } else { selection };
        self.drawer_open = self.selection.is_some();
        self.expanded_titles.clear();
        self.session_details_open = false;
        self.meeting_preview = None;
        self.refresh_meeting_preview(cx);
        cx.notify();
    }

    fn refresh_meeting_preview(&mut self, cx: &mut Context<Self>) {
        let Some(Selection::Meeting(id)) = self.selection.clone() else {
            return;
        };
        let Some(meeting) = self
            .model
            .as_ref()
            .and_then(|m| m.meetings.iter().find(|m| m.id == id).cloned())
        else {
            return;
        };
        let library = self.data.read(cx).library.clone();
        cx.spawn(async move |this, cx| {
            let preview = cx
                .background_spawn(async move { MeetingPreview::load(&library, meeting) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selection == Some(Selection::Meeting(preview.meeting.id)) {
                    this.meeting_preview = Some(preview);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn page_width(&self) -> Pixels {
        self.bounds.get().map(|b| b.size.width).unwrap_or(px(1056.))
    }

    fn uses_drawer(&self) -> bool {
        self.page_width() < px(DRAWER_BREAKPOINT)
    }

    fn rail_width(&self) -> Pixels {
        let width = f32::from(self.page_width());
        (width - DAY_MIN_WIDTH).clamp(260., 360.).min(640.).into()
    }

    // --------------------------------------------------------------- header

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let lb = cx.lb().clone();
        let today = Local::now().date_naive();
        let is_today = self.day >= today;
        let compact = self.page_width() < px(1000.);
        let day_label = self.day.format("%a, %b %-d").to_string();
        let calendar = self.calendar.clone();
        let page = cx.entity().downgrade();
        let date_button = Popover::new("timeline-calendar")
            .open(self.calendar_open)
            .on_open_change(move |open, _, cx| {
                let _ = page.update(cx, |this, cx| {
                    this.calendar_open = *open;
                    cx.notify();
                });
            })
            .trigger(ui::controls::bordered_icon(
                "timeline-day",
                Ico::Calendar,
                day_label,
            ))
            .content(move |_, _, _| div().p(px(4.)).child(Calendar::new(&calendar)));
        let controls = h_flex()
            .gap(px(7.))
            .items_center()
            .child(
                Button::new("timeline-previous")
                    .outline()
                    .icon(Icon::new(Ico::ChevronLeft))
                    .tooltip("Previous day")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let day = this.day - Days::new(1);
                        this.set_day(day, cx)
                    })),
            )
            .child(date_button)
            .child(
                ui::controls::bordered("timeline-today", "Today")
                    .disabled(is_today)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.set_day(Local::now().date_naive(), cx)),
                    ),
            )
            .child(
                Button::new("timeline-next")
                    .outline()
                    .icon(Icon::new(Ico::ChevronRight))
                    .tooltip("Next day")
                    .disabled(is_today)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let day = this.day + Days::new(1);
                        this.set_day(day, cx)
                    })),
            );
        let digest_controls = digest::digest_controls(
            self.day,
            self.model.as_ref().and_then(|m| m.digest.as_ref()),
            true,
            &self.data,
            cx,
        );
        let header = if compact {
            v_flex()
                .gap(px(8.))
                .child(controls)
                .child(digest_controls)
                .into_any_element()
        } else {
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(controls)
                .child(div().flex_1())
                .child(digest_controls)
                .into_any_element()
        };
        v_flex()
            .flex_none()
            .child(div().px(px(16.)).py(px(12.)).child(header))
            .child(div().h(px(1.)).bg(lb.separator))
            .into_any_element()
    }

    // ------------------------------------------------------------------ day

    fn render_day_column(
        &self,
        model: &DayModel,
        include_rail: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lb = cx.lb().clone();
        let page = cx.entity().downgrade();
        let toggle: digest::TaskToggle = Rc::new(move |ix, _, cx| {
            let _ = page.update(cx, |this, cx| {
                if !this.expanded_tasks.remove(&ix) {
                    this.expanded_tasks.insert(ix);
                }
                cx.notify();
            });
        });
        let digest_panel = ui::panel(cx).flex().flex_col().gap(px(10.)).child(
            div()
                .text_size(ty::SECTION)
                .font_weight(FontWeight::BOLD)
                .text_color(lb.text)
                .child("Day Digest"),
        );
        let digest_panel = digest_panel.child(digest::digest_card(
            model.digest.as_ref(),
            DigestMode::Timeline,
            px(600.),
            &self.expanded_tasks,
            toggle,
            cx,
        ));
        let attention = (!model.attention.is_empty()).then(|| {
            let count = model.attention.len();
            let rows = model.attention.iter().enumerate().map(|(ix, thread)| {
                v_flex()
                    .child(threads::thread_row(
                        format!("timeline-thread-{ix}").into(),
                        thread,
                        false,
                        &self.data,
                        cx,
                    ))
                    .when(ix + 1 < count, |this| {
                        this.child(div().h(px(1.)).bg(lb.separator))
                    })
            });
            let data = self.data.clone();
            ui::section(
                "Needs Attention",
                v_flex().children(rows).child(h_flex().pt(px(8.)).child(
                    ui::controls::bordered("timeline-review-actions", "Review Actions…").on_click(
                        move |_, _, cx| data.update(cx, |data, cx| data.open_actions(cx)),
                    ),
                )),
                cx,
            )
        });
        div()
            .id("timeline-day")
            .size_full()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .p(metric::DETAIL_PADDING)
                    .gap(metric::SECTION_SPACING)
                    .child(digest_panel)
                    .children(attention)
                    .when(include_rail, |this| {
                        this.child(self.render_rail_content(model, cx))
                    }),
            )
            .into_any_element()
    }

    // ----------------------------------------------------------------- rail

    fn render_rail_content(&self, model: &DayModel, cx: &mut Context<Self>) -> AnyElement {
        let lb = cx.lb().clone();
        let tracking = self.data.read(cx).settings.tracking_enabled;
        let sessions: Vec<&WorkSession> = model.activity.sessions.iter().collect();
        let nothing = model.activity.blocks.is_empty()
            && model.meetings.is_empty()
            && model.moments.is_empty();
        let sessions_block: AnyElement = if nothing {
            ui::empty_state(
                Ico::Clock,
                "No activity recorded",
                if tracking {
                    "Blocks appear as you use your PC (sampled every 5 s, idle-aware)."
                } else {
                    "Day tracking is off — enable it in Settings."
                },
                cx,
            )
            .into_any_element()
        } else {
            let rows: Vec<AnyElement> = sessions
                .iter()
                .enumerate()
                .map(|(ix, s)| self.render_session_row(ix, s, cx))
                .collect();
            v_flex()
                .gap(px(10.))
                .child(div().text_size(ty::SECTION).font_weight(FontWeight::BOLD).text_color(lb.text).child("Work Sessions"))
                .when(rows.is_empty(), |this| {
                    this.child(ui::empty_state(
                        Ico::Layers,
                        "No meaningful sessions",
                        "Only system activity or raw context was captured. You can still inspect it below.",
                        cx,
                    ))
                })
                .child(v_flex().gap(px(8.)).children(rows))
                .child(self.render_raw_capture_row(model, cx))
                .into_any_element()
        };
        v_flex()
            .gap(metric::SECTION_SPACING)
            .when(!nothing, |this| {
                this.child(digest::day_overview(
                    "Day Overview",
                    &model.activity,
                    true,
                    true,
                    cx,
                ))
            })
            .child(sessions_block)
            .into_any_element()
    }

    /// The rail's last row, opening raw activity in the context panel.
    fn render_raw_capture_row(&self, model: &DayModel, cx: &mut Context<Self>) -> AnyElement {
        let lb = cx.lb().clone();
        let selected = self.selection == Some(Selection::RawCapture);
        let (fill, stroke) = if selected {
            (lb.accent_text.opacity(0.11), lb.accent_text.opacity(0.8))
        } else {
            (lb.text.opacity(0.035), lb.text.opacity(0.10))
        };
        h_flex()
            .id("timeline-raw-capture")
            .mt(px(4.))
            .w_full()
            .p(px(10.))
            .gap(px(10.))
            .items_center()
            .rounded(metric::GROUP_RADIUS)
            .bg(fill)
            .border_1()
            .border_color(stroke)
            .cursor_pointer()
            .hover(move |s| s.bg(fill.opacity(1.6)))
            .child(div().w(px(24.)).flex().justify_center().child(ui::icon(
                Ico::Activity,
                px(16.),
                lb.text_secondary,
            )))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(ty::BODY)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(lb.text)
                            .child("Browse Raw Capture"),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(ty::CAPTION)
                            .text_color(lb.text_secondary)
                            .child(context::capture_counts(model)),
                    ),
            )
            .child(ui::icon(Ico::ChevronRight, px(13.), lb.text_tertiary))
            .on_click(cx.listener(|this, _, _, cx| this.select(Some(Selection::RawCapture), cx)))
            .into_any_element()
    }

    fn render_session_row(
        &self,
        ix: usize,
        session: &WorkSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lb = cx.lb().clone();
        let meeting = session.meeting_id;
        let selected = match (&self.selection, meeting) {
            (Some(Selection::Meeting(id)), Some(mid)) => *id == mid,
            (Some(Selection::Session(id)), None) => *id == session.id,
            _ => false,
        };
        let live = meeting.is_some_and(|id| {
            self.data
                .read(cx)
                .recording
                .meeting()
                .is_some_and(|m| m.id == id)
        });
        let tint: Hsla = if meeting.is_some() {
            lb.accent_text
        } else {
            lb.app_color(&session.primary_app)
        };
        let (fill, stroke) = match (meeting.is_some(), selected) {
            (false, false) => (lb.text.opacity(0.035), lb.text.opacity(0.10)),
            (false, true) => (lb.accent_text.opacity(0.11), lb.accent_text.opacity(0.8)),
            (true, false) => (lb.accent_text.opacity(0.045), lb.accent_text.opacity(0.20)),
            (true, true) => (lb.accent_text.opacity(0.13), lb.accent_text.opacity(0.85)),
        };
        let apps = {
            let visible: Vec<&str> = session.apps.iter().take(3).map(String::as_str).collect();
            let rest = session.apps.len().saturating_sub(3);
            if rest > 0 {
                format!("{} · +{rest}", visible.join(" · "))
            } else {
                visible.join(" · ")
            }
        };
        let detail = if meeting.is_some() {
            if live {
                "Meeting · Recording".to_string()
            } else {
                "Meeting".to_string()
            }
        } else {
            apps
        };
        let duration = if meeting.is_some() {
            (session.end - session.start).num_seconds().max(0) as f64
        } else {
            session.active_seconds
        };
        let selection = match meeting {
            Some(id) => Selection::Meeting(id),
            None => Selection::Session(session.id.clone()),
        };
        h_flex()
            .id(("timeline-session", ix))
            .w_full()
            .p(px(10.))
            .gap(px(10.))
            .items_center()
            .rounded(metric::GROUP_RADIUS)
            .bg(fill)
            .border_1()
            .border_color(stroke)
            .cursor_pointer()
            .hover(move |s| s.bg(fill.opacity(1.6)))
            .child(
                v_flex()
                    .w(px(64.))
                    .flex_none()
                    .items_end()
                    .gap(px(2.))
                    .text_size(ty::CAPTION)
                    .child(
                        div()
                            .text_color(lb.text)
                            .child(lokalbot_core::time::clock_time(session.start)),
                    )
                    .child(
                        div()
                            .text_color(lb.supporting)
                            .child(lokalbot_core::time::clock_time(session.end)),
                    ),
            )
            .child(ui::icon_tile(
                if meeting.is_some() {
                    Ico::AudioLines
                } else {
                    Ico::Briefcase
                },
                tint,
                px(24.),
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        div()
                            .truncate()
                            .text_size(ty::BODY)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(lb.text)
                            .child(session.title.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .text_size(ty::CAPTION)
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(if live {
                                        lb.recording_text
                                    } else {
                                        lb.supporting
                                    })
                                    .child(detail),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(lb.text)
                                    .child(crate::pages::today::digest::hm(duration)),
                            ),
                    ),
            )
            .child(ui::icon(Ico::ChevronRight, px(13.), lb.text_tertiary))
            .on_click(cx.listener(move |this, _, _, cx| this.select(Some(selection.clone()), cx)))
            .into_any_element()
    }

    fn ask_day(&self, question: String, meeting_ids: Vec<Uuid>, cx: &mut Context<Self>) {
        self.data.update(cx, |data, cx| {
            data.ask(
                crate::state::AskRequest {
                    question,
                    meeting_ids,
                    submit: false,
                },
                cx,
            )
        });
    }
}

impl Page for TimelinePage {
    fn title(&self, _cx: &App) -> SharedString {
        "Timeline".into()
    }

    fn toolbar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let paused = self.data.read(cx).day_memory_paused;
        let data = self.data.clone();
        Some(
            h_flex()
                .gap_1()
                .child(
                    ui::controls::toolbar_icon(
                        "timeline-pause",
                        if paused { Ico::Play } else { Ico::Pause },
                        if paused {
                            "Resume day memory"
                        } else {
                            "Pause day memory"
                        },
                    )
                    .on_click(move |_, _, cx| {
                        data.update(cx, |data, cx| {
                            data.request(AppEvent::SetDayMemoryPaused(!paused), cx)
                        })
                    }),
                )
                .child(
                    ui::controls::toolbar_icon(
                        "timeline-refresh",
                        Ico::RefreshCw,
                        "Reload the selected day's local activity",
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                )
                .into_any_element(),
        )
    }
}

impl Render for TimelinePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lb = cx.lb().clone();
        let bounds = self.bounds.clone();
        let measure = canvas(
            move |b, window, _| {
                if bounds.get().map(|old| old.size) != Some(b.size) {
                    bounds.set(Some(b));
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let header = self.render_header(cx);
        let body: AnyElement = match self.model.clone() {
            None => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(gpui_kit::component::spinner::Spinner::new())
                .into_any_element(),
            Some(model) if self.uses_drawer() => {
                let day = self.render_day_column(&model, true, cx);
                let drawer = (self.drawer_open && self.selection.is_some()).then(|| {
                    let width = (self.page_width() - px(72.)).max(px(320.)).min(px(520.));
                    let panel = self.render_context(&model, true, window, cx);
                    div()
                        .absolute()
                        .inset_0()
                        .child(
                            div()
                                .id("timeline-backdrop")
                                .absolute()
                                .inset_0()
                                .bg(gpui_kit::black().opacity(0.16))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.drawer_open = false;
                                        this.selection = None;
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right_0()
                                .w(width)
                                .bg(lb.elevated)
                                .border_l_1()
                                .border_color(lb.separator)
                                .shadow_lg()
                                .occlude()
                                .child(panel),
                        )
                });
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(day)
                    .children(drawer)
                    .into_any_element()
            }
            Some(model) => {
                let left = if self.selection.is_some() {
                    self.render_context(&model, false, window, cx)
                } else {
                    self.render_day_column(&model, false, cx)
                };
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_1().min_w(px(DAY_MIN_WIDTH)).h_full().child(left))
                    .child(
                        div()
                            .id("timeline-rail")
                            .w(self.rail_width())
                            .flex_none()
                            .h_full()
                            .bg(lb.sidebar.opacity(if lb.dark { 0.6 } else { 0.55 }))
                            .border_l_1()
                            .border_color(lb.separator)
                            .overflow_y_scrollbar()
                            .child(div().p(px(16.)).child(self.render_rail_content(&model, cx))),
                    )
                    .into_any_element()
            }
        };
        v_flex()
            .relative()
            .size_full()
            .child(measure)
            .child(header)
            .child(body)
    }
}
