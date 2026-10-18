//! Keyword results while typing (`AskView.results`).

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

use super::scope::DateScope;
use super::search::{Facet, HitKind, MeetingGroup};
use super::{AskPage, history};
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico, result_row};

/// Rows above the first result in the scroll list (status line, count line).
pub const HEADER_ROWS: usize = 2;

impl AskPage {
    pub(super) fn render_results(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let lb = cx.lb().clone();
        let count = self.results.groups.len();
        let asks = !self.results.picked_with_keyboard && super::search::is_question(&self.query);
        let count_text = {
            let base = history::count_label(count, "result");
            let base = if self.results.truncated {
                format!("More than {base}")
            } else {
                base
            };
            if asks {
                format!("{base} · Return asks · ↓ to pick a result")
            } else {
                format!("{base} · Return opens · Ctrl+Return asks")
            }
        };
        let column = |element: AnyElement| {
            div()
                .w_full()
                .max_w(metric::READING_MAX_WIDTH + metric::DETAIL_PADDING * 2.)
                .px(metric::DETAIL_PADDING)
                .child(element)
                .into_any_element()
        };

        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(column(
            div()
                .pt(px(14.))
                .when(self.results.running, |this| {
                    this.child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .pb(px(4.))
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(Spinner::new().xsmall().color(lb.text_secondary))
                            .child("Searching local sources…"),
                    )
                })
                .into_any_element(),
        ));
        rows.push(column(
            div()
                .pb(px(6.))
                .text_size(ty::SECONDARY)
                .text_color(lb.supporting)
                .when(!self.results.running || count > 0, |this| {
                    this.child(count_text)
                })
                .into_any_element(),
        ));

        if count == 0 && !self.results.running {
            let message = if !self.sources.meetings {
                "Activity totals are available in Ask. Enable Meetings or Screen to search source text.".to_string()
            } else {
                format!(
                    "No results for “{}” in {} · {}.",
                    self.query.trim(),
                    self.sources.summary(),
                    self.date_scope.label()
                )
            };
            let filtered = self.facet != Facet::All || self.date_scope != DateScope::AnyTime;
            rows.push(column(
                v_flex()
                    .gap(px(6.))
                    .py(px(6.))
                    .child(
                        div()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(message),
                    )
                    .when(filtered, |this| {
                        let page = cx.entity();
                        this.child(div().text_size(ty::SECONDARY).child(ui::link(
                            "ask-clear-filters",
                            "Clear search filters",
                            move |window, cx| {
                                page.update(cx, |page, cx| page.clear_filters(window, cx))
                            },
                            cx,
                        )))
                    })
                    .into_any_element(),
            ));
        }

        let groups = self.results.groups.clone();
        for (index, group) in groups.iter().enumerate() {
            rows.push(column(
                self.render_group(index, group, cx).into_any_element(),
            ));
        }
        rows.push(div().h(px(12.)).into_any_element());

        // Rows are direct children so the scroll handle can reveal a selected row.
        v_flex()
            .id("ask-results")
            .size_full()
            .items_center()
            .overflow_y_scroll()
            .track_scroll(&self.results_scroll)
            .children(rows)
    }

    fn render_group(
        &self,
        index: usize,
        group: &MeetingGroup,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let lb = cx.lb().clone();
        let selected = index == self.results.selected;
        let meeting_id = group.meeting.id;
        let key = history::element_key(meeting_id);
        let title = group.meeting.display_title();
        let date = group
            .meeting
            .started_at
            .with_timezone(&chrono::Local)
            .format("%b %-d, %Y")
            .to_string();
        let expanded = self.results.expanded.contains(&meeting_id);
        let primary = group.hits[0].clone();
        let more = group.hits.len() - 1;

        let hit_row = |id: (&'static str, u64),
                       hit: &super::search::Hit,
                       cx: &mut Context<Self>| {
            let at = (hit.kind == HitKind::Segment).then_some(hit.start);
            div()
                .id(id)
                .w_full()
                .cursor_pointer()
                .child(result_row::result_body(
                    title.clone().into(),
                    Some(date.clone().into()),
                    hit.kind_label().into(),
                    None,
                    &hit.snippet,
                    cx,
                ))
                .on_click(cx.listener(move |this, _, _, cx| this.open_meeting(meeting_id, at, cx)))
        };

        v_flex()
            .w_full()
            .gap(px(4.))
            .px(px(10.))
            .py(px(8.))
            .rounded(metric::ROW_RADIUS)
            .when(selected, |this| this.bg(lb.accent_wash))
            .when(!selected, |this| this.hover(|s| s.bg(lb.hover)))
            .child(hit_row(("ask-result", key), &primary, cx))
            .when(more > 0, |this| {
                let label = if more == 1 {
                    "1 more match".to_string()
                } else {
                    format!("{more} more matches")
                };
                this.child(
                    h_flex()
                        .id(("ask-result-more", key))
                        .gap(px(4.))
                        .items_center()
                        .cursor_pointer()
                        .text_size(ty::SECONDARY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(lb.text_secondary)
                        .hover(|s| s.text_color(lb.text))
                        .child(ui::icon(
                            if expanded {
                                Ico::ChevronDown
                            } else {
                                Ico::ChevronRight
                            },
                            px(12.),
                            lb.text_secondary,
                        ))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.results.expanded.remove(&meeting_id) {
                                this.results.expanded.insert(meeting_id);
                            }
                            cx.notify();
                        })),
                )
            })
            .when(expanded, |this| {
                let hits: Vec<_> = group.hits.iter().skip(1).cloned().collect();
                this.child(
                    v_flex().gap(px(8.)).pl(px(16.)).pt(px(2.)).children(
                        hits.iter()
                            .enumerate()
                            .map(|(i, hit)| {
                                hit_row(("ask-result-hit", key.wrapping_add(i as u64 + 1)), hit, cx)
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
            })
    }
}
