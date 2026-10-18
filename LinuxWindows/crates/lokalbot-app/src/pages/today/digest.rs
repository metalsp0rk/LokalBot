//! The Day Digest and the day's activity overview, shared by Today and
//! Timeline (`DayDigestCard.swift`, `DayDigestView.swift`,
//! `DayActivityOverview.swift`).

use std::collections::HashSet;
use std::rc::Rc;

use chrono::NaiveDate;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Disableable as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Entity, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use lokalbot_core::{DayActivity, DayDigest, DigestTask, TaskStatus};

use crate::state::{AppData, AppEvent};
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DigestMode {
    Today,
    Timeline,
}

/// Toggles a task's details by index.
pub(crate) type TaskToggle = Rc<dyn Fn(usize, &mut Window, &mut App)>;

// --------------------------------------------------------------- overview

/// `CaptureStyle.hm`: "3h 50m", "48m", "<1m" (and "0m" for nothing).
pub(crate) fn hm(seconds: f64) -> String {
    if seconds <= 0. {
        "0m".into()
    } else {
        lokalbot_core::time::compact_duration(seconds)
    }
}

/// One proportion-bar segment: label, seconds, whether it is "Other".
pub(crate) struct AppShare {
    pub label: String,
    pub seconds: f64,
    pub is_other: bool,
}

/// Top five apps with at least a minute, then "Other" (`ProportionBarMath`).
pub(crate) fn app_shares(activity: &DayActivity) -> Vec<AppShare> {
    let mut shares = Vec::new();
    let mut other = 0.;
    for usage in activity
        .app_usage
        .iter()
        .filter(|u| !lokalbot_core::timeline::is_system_app(&u.app))
    {
        if shares.len() < 5 && usage.seconds >= 60. {
            shares.push(AppShare {
                label: usage.app.clone(),
                seconds: usage.seconds,
                is_other: false,
            });
        } else {
            other += usage.seconds;
        }
    }
    if other > 0. {
        shares.push(AppShare {
            label: "Other".into(),
            seconds: other,
            is_other: true,
        });
    }
    shares
}

pub(crate) fn app_count(activity: &DayActivity) -> usize {
    activity
        .app_usage
        .iter()
        .filter(|u| !lokalbot_core::timeline::is_system_app(&u.app))
        .count()
}

fn share_color(share: &AppShare, cx: &App) -> Hsla {
    let lb = cx.lb();
    if share.is_other {
        lb.text_tertiary.opacity(0.8)
    } else {
        lb.app_color(&share.label)
    }
}

/// "Day So Far" / "Day Overview": stat chips, proportion bar, optional legend.
pub(crate) fn day_overview(
    title: &'static str,
    activity: &DayActivity,
    legend: bool,
    narrow: bool,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let apps = app_count(activity);
    let mut chips = vec![
        ui::stat_chip(
            Ico::Clock,
            crate::pages::today::digest::hm(activity.tracked_seconds),
            "tracked",
            cx,
        )
        .into_any_element(),
        ui::stat_chip(
            Ico::LayoutGrid,
            apps.to_string(),
            if apps == 1 { "app" } else { "apps" },
            cx,
        )
        .into_any_element(),
        ui::stat_chip(
            Ico::ScanText,
            activity.moment_count.to_string(),
            "moments",
            cx,
        )
        .into_any_element(),
    ];
    if activity.meeting_count > 0 || narrow {
        chips.push(
            ui::stat_chip(
                Ico::AudioLines,
                activity.meeting_count.to_string(),
                if activity.meeting_count == 1 {
                    "meeting"
                } else {
                    "meetings"
                },
                cx,
            )
            .into_any_element(),
        );
    }
    let shares = app_shares(activity);
    let segments: Vec<(Hsla, f32)> = shares
        .iter()
        .map(|s| (share_color(s, cx), s.seconds as f32))
        .collect();
    v_flex()
        .gap(px(10.))
        .child(
            div()
                .text_size(ty::SECTION)
                .font_weight(FontWeight::BOLD)
                .text_color(lb.text)
                .child(title),
        )
        .child(h_flex().flex_wrap().gap(px(8.)).children(chips))
        .when(!shares.is_empty(), |this| {
            this.child(ui::proportion_bar(&segments, cx))
                .when(legend, |this| {
                    this.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_y(px(8.))
                            .children(shares.iter().map(|share| {
                                h_flex()
                                    .w(px(150.))
                                    .gap(px(6.))
                                    .items_center()
                                    .text_size(ty::SECONDARY)
                                    .child(ui::status_dot(share_color(share, cx), px(6.)))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_color(lb.text)
                                            .child(share.label.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_color(lb.text_secondary)
                                            .child(crate::pages::today::digest::hm(share.seconds)),
                                    )
                            })),
                    )
                })
        })
        .into_any_element()
}

// --------------------------------------------------------------- controls

/// Generate button and the "Digest actions" menu (and Ask on Timeline).
pub(crate) fn digest_controls(
    day: NaiveDate,
    digest: Option<&DayDigest>,
    show_ask: bool,
    data: &Entity<AppData>,
    cx: &App,
) -> AnyElement {
    let busy = data.read(cx).digest_running == Some(day);
    let label = if busy {
        "Writing Digest…"
    } else if digest.is_none() {
        "Write Digest"
    } else {
        "Regenerate"
    };
    let generate = {
        let data = data.clone();
        Button::new("digest-generate")
            .outline()
            .icon(Icon::new(if busy {
                Ico::Hourglass
            } else {
                Ico::RefreshCw
            }))
            .label(label)
            .disabled(busy)
            .on_click(move |_, _, cx| {
                data.update(cx, |data, cx| {
                    data.request(AppEvent::RegenerateDigest(day), cx)
                })
            })
    };
    let actions = digest.map(|digest| {
        let markdown = digest.markdown.clone();
        let data = data.clone();
        Button::new("digest-actions")
            .ghost()
            .icon(Icon::new(Ico::CircleEllipsis))
            .label("Digest actions")
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                let copy = markdown.clone();
                let export = markdown.clone();
                let copy_data = data.clone();
                let export_data = data.clone();
                menu.item(
                    PopupMenuItem::new("Copy Digest")
                        .icon(Icon::new(Ico::Copy))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                            copy_data
                                .update(cx, |data, cx| data.show_message("Digest copied.", cx));
                        }),
                )
                .item(
                    PopupMenuItem::new("Export Markdown")
                        .icon(Icon::new(Ico::Share))
                        .on_click(move |_, _, cx| {
                            export_markdown(export_data.clone(), day, export.clone(), cx)
                        }),
                )
            })
    });
    let ask = show_ask.then(|| {
        let data = data.clone();
        ui::controls::bordered_icon("digest-ask", Ico::TextSearch, "Ask About This Day").on_click(
            move |_, _, cx| {
                let question = format!("What happened on {}?", day.format("%A, %B %-d"));
                data.update(cx, |data, cx| {
                    data.ask(
                        crate::state::AskRequest {
                            question,
                            meeting_ids: Vec::new(),
                            submit: false,
                        },
                        cx,
                    )
                })
            },
        )
    });
    h_flex()
        .gap(px(8.))
        .items_center()
        .children(ask)
        .child(generate)
        .children(actions)
        .into_any_element()
}

fn export_markdown(data: Entity<AppData>, day: NaiveDate, markdown: String, cx: &mut App) {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let name = format!("{}.md", day.format("%Y-%m-%d"));
    let receiver = cx.prompt_for_new_path(&home, Some(&name));
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(path))) = receiver.await else {
            return;
        };
        let result = cx
            .background_spawn(async move { std::fs::write(&path, markdown) })
            .await;
        data.update(cx, |data, cx| match result {
            Ok(()) => data.show_message("Digest exported as Markdown.", cx),
            Err(err) => data.show_error(format!("Digest export failed: {err}"), cx),
        });
    })
    .detach();
}

// ------------------------------------------------------------------- body

/// The digest, its freshness line, or the empty text.
pub(crate) fn digest_card(
    digest: Option<&DayDigest>,
    mode: DigestMode,
    width: gpui_kit::Pixels,
    expanded: &HashSet<usize>,
    on_toggle: TaskToggle,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let Some(digest) = digest else {
        return div()
            .text_size(ty::BODY)
            .text_color(lb.text_secondary)
            .child("Your digest brings together this day's meetings and captured activity.")
            .into_any_element();
    };
    let updated = digest
        .meta
        .as_ref()
        .map(|m| m.generated_at)
        .or(digest.modified_at);
    v_flex()
        .gap(px(14.))
        .child(digest_view(digest, mode, width, expanded, on_toggle, cx))
        .when(updated.is_some() || digest.is_stale, |this| {
            this.child(
                h_flex()
                    .flex_wrap()
                    .gap_x(px(8.))
                    .text_size(ty::SECONDARY)
                    .text_color(lb.text_secondary)
                    .when_some(updated, |this, at| {
                        this.child(format!("Updated {}", lokalbot_core::time::clock_time(at)))
                    })
                    .when(digest.is_stale, |this| {
                        this.child("New activity available · Update digest to include it")
                    }),
            )
        })
        .into_any_element()
}

fn digest_section(icon: Ico, title: &str, content: impl IntoElement, cx: &App) -> AnyElement {
    let lb = cx.lb();
    v_flex()
        .gap(px(8.))
        .child(
            h_flex()
                .gap(px(8.))
                .items_center()
                .text_size(ty::SECONDARY)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(lb.text)
                .child(ui::icon(icon, px(15.), lb.text))
                .child(title.to_string()),
        )
        .child(content)
        .into_any_element()
}

fn bullet_list(items: &[String], cx: &App) -> AnyElement {
    let lb = cx.lb();
    v_flex()
        .gap(px(4.))
        .children(items.iter().map(|item| {
            h_flex()
                .gap(px(8.))
                .items_start()
                .text_size(ty::BODY)
                .line_height(px(21.))
                .text_color(lb.text)
                .child(div().flex_none().text_color(lb.text_tertiary).child("•"))
                .child(div().flex_1().min_w_0().child(item.clone()))
        }))
        .into_any_element()
}

/// `DayDigestView` in its Today or Timeline mode.
fn digest_view(
    digest: &DayDigest,
    mode: DigestMode,
    width: gpui_kit::Pixels,
    expanded: &HashSet<usize>,
    on_toggle: TaskToggle,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let mut parts: Vec<AnyElement> = Vec::new();
    if !digest.blockers.is_empty() {
        let title = if digest.blockers.len() == 1 {
            "Blocker"
        } else {
            "Blockers"
        };
        parts.push(
            ui::status_surface(lb.attention, cx)
                .px(px(12.))
                .py(px(10.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .text_size(ty::SECONDARY)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(lb.attention_text)
                        .child(ui::icon(Ico::TriangleAlert, px(14.), lb.attention_text))
                        .child(title),
                )
                .child(bullet_list(&digest.blockers, cx))
                .into_any_element(),
        );
    }
    if !digest.at_a_glance_markdown.trim().is_empty() {
        parts.push(digest_section(
            Ico::Sparkles,
            "Highlights",
            div()
                .max_w(metric::READING_MAX_WIDTH)
                .text_size(ty::BODY)
                .text_color(lb.text)
                .child(
                    crate::ui::markdown_view::markdown(
                        "digest-highlights",
                        digest.at_a_glance_markdown.clone(),
                    )
                    .selectable(true),
                ),
            cx,
        ));
    }
    if !digest.decisions.is_empty() {
        parts.push(digest_section(
            Ico::BadgeCheck,
            "Decisions",
            div()
                .max_w(metric::READING_MAX_WIDTH)
                .child(bullet_list(&digest.decisions, cx)),
            cx,
        ));
    }
    if !digest.tasks.is_empty() {
        let title = if mode == DigestMode::Today {
            "Tasks"
        } else {
            "Work Summary"
        };
        let body = match mode {
            DigestMode::Today => task_grid(&digest.tasks, width, expanded, on_toggle.clone(), cx),
            DigestMode::Timeline => task_groups(&digest.tasks, expanded, on_toggle.clone(), cx),
        };
        parts.push(
            v_flex()
                .gap(px(12.))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .text_size(ty::SECTION)
                        .font_weight(FontWeight::BOLD)
                        .text_color(lb.text)
                        .child(ui::icon(Ico::Briefcase, px(16.), lb.text))
                        .child(title),
                )
                .child(body)
                .into_any_element(),
        );
    }
    if !digest.follow_up_tasks.is_empty() {
        let offset = 10_000;
        parts.push(digest_section(
            Ico::CornerDownRight,
            "Follow-ups",
            task_rows(&digest.follow_up_tasks, offset, expanded, on_toggle, cx),
            cx,
        ));
    } else if !digest.follow_ups.is_empty() {
        parts.push(digest_section(
            Ico::CornerDownRight,
            "Follow-ups",
            bullet_list(&digest.follow_ups, cx),
            cx,
        ));
    }
    v_flex().gap(px(20.)).children(parts).into_any_element()
}

/// Card grid of 1/2/3 columns at widths below 560 / from 560 / from 900.
fn task_grid(
    tasks: &[DigestTask],
    width: gpui_kit::Pixels,
    expanded: &HashSet<usize>,
    on_toggle: TaskToggle,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let columns = if width >= px(900.) {
        3
    } else if width >= px(560.) {
        2
    } else {
        1
    };
    let rows = tasks.chunks(columns).enumerate().map(|(row_ix, row)| {
        h_flex()
            .gap(px(12.))
            .items_stretch()
            .children(row.iter().enumerate().map(|(col, task)| {
                let ix = row_ix * columns + col;
                div()
                    .flex_1()
                    .min_w_0()
                    .p(metric::PANEL_PADDING)
                    .rounded(metric::GROUP_RADIUS)
                    .bg(lb.chip_fill.opacity(0.5))
                    .border_1()
                    .border_color(lb.separator.opacity(0.9))
                    .child(task_row(
                        task,
                        ix,
                        expanded.contains(&ix),
                        on_toggle.clone(),
                        cx,
                    ))
                    .into_any_element()
            }))
            .children((row.len()..columns).map(|_| div().flex_1().into_any_element()))
    });
    v_flex().gap(px(12.)).children(rows).into_any_element()
}

/// Timeline's grouped list: "Blocked · N", "In progress · N", …
fn task_groups(
    tasks: &[DigestTask],
    expanded: &HashSet<usize>,
    on_toggle: TaskToggle,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let order = [
        TaskStatus::Blocked,
        TaskStatus::InProgress,
        TaskStatus::Unknown,
        TaskStatus::Completed,
    ];
    let groups: Vec<(TaskStatus, Vec<(usize, &DigestTask)>)> = order
        .iter()
        .map(|status| {
            (
                *status,
                tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.status == *status)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, items)| !items.is_empty())
        .collect();
    let show_titles = groups.len() > 1;
    v_flex()
        .gap(px(14.))
        .children(groups.into_iter().map(|(status, items)| {
            let count = items.len();
            v_flex()
                .gap(px(2.))
                .when(show_titles, |this| {
                    this.child(
                        div()
                            .pb(px(2.))
                            .text_size(ty::SECONDARY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(lb.text_secondary)
                            .child(format!("{} · {count}", status.group_title())),
                    )
                })
                .children(items.into_iter().enumerate().map(|(pos, (ix, task))| {
                    v_flex()
                        .child(div().py(px(7.)).child(task_row(
                            task,
                            ix,
                            expanded.contains(&ix),
                            on_toggle.clone(),
                            cx,
                        )))
                        .when(pos + 1 < count, |this| {
                            this.child(div().ml(px(26.)).h(px(1.)).bg(lb.separator))
                        })
                }))
        }))
        .into_any_element()
}

fn task_rows(
    tasks: &[DigestTask],
    offset: usize,
    expanded: &HashSet<usize>,
    on_toggle: TaskToggle,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let count = tasks.len();
    v_flex()
        .children(tasks.iter().enumerate().map(|(pos, task)| {
            let ix = offset + pos;
            v_flex()
                .child(div().py(px(7.)).child(task_row(
                    task,
                    ix,
                    expanded.contains(&ix),
                    on_toggle.clone(),
                    cx,
                )))
                .when(pos + 1 < count, |this| {
                    this.child(div().ml(px(26.)).h(px(1.)).bg(lb.separator))
                })
        }))
        .into_any_element()
}

/// Status mark, title, "Next" step and collapsible details.
fn task_row(
    task: &DigestTask,
    ix: usize,
    expanded: bool,
    on_toggle: TaskToggle,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let mark: AnyElement = match task.status {
        TaskStatus::Completed => {
            ui::icon(Ico::CircleCheck, px(16.), lb.success_text).into_any_element()
        }
        TaskStatus::InProgress => {
            ui::icon(Ico::Contrast, px(16.), lb.accent_text).into_any_element()
        }
        TaskStatus::Blocked => {
            ui::icon(Ico::CircleAlert, px(16.), lb.attention_text).into_any_element()
        }
        TaskStatus::Unknown => div()
            .text_size(ty::BODY)
            .text_color(lb.text_tertiary)
            .child("•")
            .into_any_element(),
    };
    let collapsible = task.has_heading && task.summary.chars().count() > 160;
    let show_summary =
        task.has_heading && !task.summary.trim().is_empty() && (!collapsible || expanded);
    let title: AnyElement = if task.has_heading {
        h_flex()
            .flex_wrap()
            .gap_x(px(8.))
            .items_baseline()
            .when_some(task.time_range.clone(), |this, range| {
                this.child(
                    div()
                        .text_size(ty::CAPTION)
                        .text_color(lb.text_secondary)
                        .child(range),
                )
            })
            .child(
                div()
                    .text_size(ty::BODY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(20.))
                    .text_color(lb.text)
                    .child(task.title.clone()),
            )
            .into_any_element()
    } else {
        div()
            .text_size(ty::BODY)
            .line_height(px(20.))
            .text_color(lb.text)
            .child(task.title.clone())
            .into_any_element()
    };
    let toggle = on_toggle.clone();
    h_flex()
        .w_full()
        .items_start()
        .gap(px(8.))
        .child(
            div()
                .w(px(18.))
                .flex_none()
                .pt(px(2.))
                .flex()
                .justify_center()
                .child(mark),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.))
                .child(title)
                .when_some(task.next_step.clone(), |this, next| {
                    this.child(
                        h_flex()
                            .gap(px(5.))
                            .items_start()
                            .text_size(ty::SECONDARY)
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(lb.accent_text)
                                    .child("Next"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(lb.text_secondary)
                                    .child(next),
                            ),
                    )
                })
                .when(show_summary, |this| {
                    this.child(
                        div()
                            .text_size(ty::SECONDARY)
                            .line_height(px(19.))
                            .text_color(lb.text)
                            .child(task.summary.clone()),
                    )
                })
                .when(collapsible, |this| {
                    this.child(
                        div()
                            .id(("task-more", ix))
                            .text_size(ty::SECONDARY)
                            .text_color(lb.accent_text)
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .child(if expanded { "Show less" } else { "Show more" })
                            .on_click(move |_, window, cx| toggle(ix, window, cx)),
                    )
                }),
        )
        .into_any_element()
}
