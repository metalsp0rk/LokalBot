//! The composer dock (`AskView.composerDock`): filter chips, the evidence
//! boundary, answer sources, the input box and the inference status.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, Focusable as _, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, linear_color_stop, linear_gradient, px,
};
use lokalbot_core::settings::SummarizerBackend;

use super::AskPage;
use super::scope::{DateScope, Source};
use super::search::Facet;
use crate::state::SettingsCategory;
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

/// Where answers are computed (`InferencePresentation`).
pub struct Inference {
    pub icon: Ico,
    pub label: SharedString,
    pub remote: bool,
    pub blocked: bool,
}

pub fn inference(cx: &App) -> Inference {
    let settings = &crate::state::AppData::global(cx).read(cx).settings;
    let url = match settings.summarizer_backend {
        SummarizerBackend::BuiltIn => None,
        SummarizerBackend::Ollama => Some(settings.ollama_base_url.clone()),
        SummarizerBackend::OpenAICompatible => Some(settings.openai_base_url.clone()),
    };
    let Some(url) = url.filter(|url| !lokalbot_core::access::is_loopback_url(url)) else {
        return Inference {
            icon: Ico::Monitor,
            label: "On this PC".into(),
            remote: false,
            blocked: false,
        };
    };
    if lokalbot_core::access::remote_block_reason(&url, &settings.approved_remote_inference_origins)
        .is_some()
    {
        return Inference {
            icon: Ico::TriangleAlert,
            label: "Model connection blocked".into(),
            remote: false,
            blocked: true,
        };
    }
    let host = lokalbot_core::access::origin_of(&url)
        .and_then(|origin| origin.split_once("://").map(|(_, host)| host.to_string()))
        .unwrap_or(url);
    Inference {
        icon: Ico::Network,
        label: format!("Remote · {host}").into(),
        remote: true,
        blocked: false,
    }
}

impl AskPage {
    pub(super) fn render_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let lb = cx.lb().clone();
        let searching = !self.query.trim().is_empty();
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(filters) = self.render_filter_chips(cx) {
            rows.push(filters.into_any_element());
        }
        if let Some(scope) = &self.meeting_scope {
            let label = format!(
                "{} · All screens",
                super::history::count_label(scope.len(), "meeting")
            );
            rows.push(
                h_flex()
                    .child(
                        Button::new("ask-evidence-scope")
                            .outline()
                            .small()
                            .icon(Icon::new(Ico::CircleX))
                            .label(label)
                            .tooltip("Clear the selected evidence boundary")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.meeting_scope = None;
                                this.scope_changed(window, cx);
                            })),
                    )
                    .into_any_element(),
            );
        }
        if searching {
            rows.push(self.render_answer_sources(cx).into_any_element());
        }
        rows.push(self.render_composer(window, cx).into_any_element());
        rows.push(self.render_inference_status(cx).into_any_element());

        div()
            .relative()
            .w_full()
            .flex_none()
            .child(
                div()
                    .absolute()
                    .top(px(-14.))
                    .left_0()
                    .right_0()
                    .h(px(14.))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(lb.window.opacity(0.), 0.),
                        linear_color_stop(lb.window.opacity(0.7), 1.),
                    )),
            )
            .child(
                v_flex().w_full().items_center().bg(lb.window).child(
                    v_flex()
                        .w_full()
                        .max_w(metric::READING_MAX_WIDTH + metric::DETAIL_PADDING * 2.)
                        .px(metric::DETAIL_PADDING)
                        .pt(px(10.))
                        .pb(px(14.))
                        .gap(px(8.))
                        .children(rows),
                ),
            )
    }

    fn render_filter_chips(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.facet == Facet::All {
            return None;
        }
        Some(
            h_flex().gap(px(8.)).child(
                Button::new("ask-filter-result-type")
                    .outline()
                    .small()
                    .icon(Icon::new(Ico::CircleX))
                    .label(format!("Results: {}", self.facet.label()))
                    .tooltip("Clear the result-type filter")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.facet = Facet::All;
                        this.scope_changed(window, cx);
                    })),
            ),
        )
    }

    /// "Answer sources: 3 meetings · 0 screen moments" with the list on demand.
    fn render_answer_sources(&self, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let count = self.results.groups.len();
        let label = if self.results.running {
            "Finding sources…".to_string()
        } else if count > 0 {
            format!(
                "Answer sources: {} · 0 screen moments",
                super::history::count_label(count, "meeting")
            )
        } else {
            format!(
                "Answer sources: {} · {}",
                self.sources.summary(),
                self.date_scope.label()
            )
        };
        let expanded = self.sources_expanded;
        let mut details: Vec<AnyElement> = Vec::new();
        if expanded {
            if count > 0 {
                details.push(
                    div()
                        .text_color(lb.supporting)
                        .child("The answer can retrieve passages from these meetings and screen moments, including collapsed matches. It may use only the passages relevant to your question.")
                        .into_any_element(),
                );
                for group in &self.results.groups {
                    let date = group
                        .meeting
                        .started_at
                        .with_timezone(&chrono::Local)
                        .format("%b %-d, %Y");
                    details.push(
                        div()
                            .truncate()
                            .text_color(lb.text_secondary)
                            .child(format!(
                                "Meeting · {} · {date}",
                                group.meeting.display_title()
                            ))
                            .into_any_element(),
                    );
                }
            } else {
                let text = if self.meeting_scope.is_some() {
                    "Ask stays within the selected evidence shown above, even when this search has no matches.".to_string()
                } else {
                    format!(
                        "Ask can search the selected sources within {}. Result-type and screen-app filters apply to search results only.",
                        self.date_scope.label().to_lowercase()
                    )
                };
                details.push(
                    div()
                        .text_color(lb.supporting)
                        .child(text)
                        .into_any_element(),
                );
            }
        }
        v_flex()
            .gap(px(6.))
            .text_size(ty::SECONDARY)
            .child(
                h_flex()
                    .id("ask-answer-sources")
                    .gap(px(6.))
                    .items_center()
                    .cursor_pointer()
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
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sources_expanded = !this.sources_expanded;
                        cx.notify();
                    })),
            )
            .when(expanded, |this| {
                this.child(
                    v_flex()
                        .id("ask-answer-sources-list")
                        .pl(px(18.))
                        .gap(px(4.))
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .children(details),
                )
            })
    }

    fn render_composer(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let focused = self
            .composer
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        let responding = self.is_responding(cx);
        let can_submit = !self.query.trim().is_empty() && !responding;
        let waiting = self.pending_question.is_some();
        let label = if waiting {
            "Waiting for sources…"
        } else if !self.results.groups.is_empty() && !self.query.trim().is_empty() {
            "Ask about results"
        } else {
            "Ask"
        };

        v_flex()
            .w_full()
            .gap(px(12.))
            .px(px(10.))
            .pt(px(12.))
            .pb(px(12.))
            .rounded(metric::COMPOSER_RADIUS)
            .bg(lb.elevated)
            .border_1()
            .border_color(if focused {
                lb.accent_fill.opacity(0.45)
            } else {
                lb.separator
            })
            .shadow_sm()
            .child(
                Textarea::new(&self.composer)
                    .appearance(false)
                    .xsmall()
                    .text_size(ty::BODY)
                    .line_height(px(21.)),
            )
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .px(px(4.))
                    .child(self.render_sources_menu(cx))
                    .child(self.render_date_scope(cx))
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .w(px(196.))
                            .flex_none()
                            .gap(px(8.))
                            .justify_end()
                            .when(responding, |this| {
                                this.child(
                                    Button::new("ask-stop")
                                        .ghost()
                                        .icon(Icon::new(Ico::CircleStop))
                                        .tooltip("Stop answering (Esc)")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.stop_answer(cx)),
                                        ),
                                )
                            })
                            .child(
                                Button::new("ask-submit")
                                    .primary()
                                    .icon(Icon::new(Ico::Sparkles))
                                    .label(label)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .disabled(!can_submit || waiting)
                                    .tooltip("Ask using the displayed results (Ctrl+Return)")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit(true, window, cx)
                                    })),
                            ),
                    ),
            )
    }

    fn render_sources_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let page = cx.entity();
        let sources = self.sources;
        let facet = self.facet;
        Button::new("ask-sources")
            .ghost()
            .small()
            .label(sources.summary())
            .font_weight(FontWeight::SEMIBOLD)
            .dropdown_caret(true)
            .tooltip("Choose sources and result filters")
            .dropdown_menu(move |menu, window, cx| {
                let mut menu = menu.min_w(px(220.));
                for source in Source::ALL {
                    let page = page.clone();
                    let checked = sources.contains(source);
                    menu = menu.item(
                        PopupMenuItem::new(source.label())
                            .checked(checked)
                            .disabled(checked && sources.count() == 1)
                            .on_click(move |_, window, cx| {
                                page.update(cx, |page, cx| {
                                    page.sources.toggle(source);
                                    page.scope_changed(window, cx);
                                })
                            }),
                    );
                }
                let facet_page = page.clone();
                let facets = PopupMenu::build(window, cx, move |mut menu, _, _| {
                    for option in Facet::ALL {
                        let page = facet_page.clone();
                        menu = menu.item(
                            PopupMenuItem::new(option.label())
                                .checked(option == facet)
                                .on_click(move |_, window, cx| {
                                    page.update(cx, |page, cx| {
                                        page.facet = option;
                                        page.scope_changed(window, cx);
                                    })
                                }),
                        );
                    }
                    menu
                });
                let settings_page = page.clone();
                menu.separator()
                    .item(PopupMenuItem::submenu("Result type", facets))
                    .separator()
                    .item(PopupMenuItem::new("Manage source permissions…").on_click(
                        move |_, _, cx| {
                            let data = settings_page.read(cx).data.clone();
                            data.update(cx, |data, cx| {
                                data.open_settings(SettingsCategory::Privacy, cx)
                            });
                        },
                    ))
            })
    }

    fn render_date_scope(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let page = cx.entity();
        let current = self.date_scope;
        let picker = self.date_picker.clone();
        h_flex()
            .gap(px(4.))
            .items_center()
            .child(
                Popover::new("ask-date-scope")
                    .anchor(gpui_kit::Anchor::BottomLeft)
                    .trigger(
                        Button::new("ask-date-scope-button")
                            .outline()
                            .small()
                            .icon(Icon::new(Ico::Calendar))
                            .label(current.label())
                            .font_weight(FontWeight::SEMIBOLD)
                            .tooltip("One date scope for meeting, activity, and screen search and answers"),
                    )
                    .content(move |_, _, cx| {
                        let lb = cx.lb().clone();
                        let popover = cx.entity();
                        let preset = |label: &'static str, scope: DateScope, cx: &App| {
                            let page = page.clone();
                            let popover = popover.clone();
                            let selected = scope == current;
                            let _ = cx;
                            Button::new(SharedString::from(format!("ask-date-{label}")))
                                .outline()
                                .small()
                                .flex_1()
                                .icon(Icon::new(if selected { Ico::Check } else { Ico::Calendar }))
                                .label(label)
                                .on_click(move |_, window, cx| {
                                    page.update(cx, |page, cx| {
                                        page.date_scope = scope;
                                        page.scope_changed(window, cx);
                                    });
                                    popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                                })
                        };
                        let use_page = page.clone();
                        let use_popover = popover.clone();
                        v_flex()
                            .w(px(340.))
                            .gap(px(12.))
                            .p(px(4.))
                            .child(div().text_size(ty::SECTION).font_weight(FontWeight::BOLD).text_color(lb.text).child("Date scope"))
                            .child(
                                div()
                                    .text_size(ty::SECONDARY)
                                    .text_color(lb.supporting)
                                    .child("Applies to search results and answers across all selected sources."),
                            )
                            .child(
                                h_flex()
                                    .gap(px(8.))
                                    .child(preset("Any time", DateScope::AnyTime, cx))
                                    .child(preset("Today", DateScope::Today, cx)),
                            )
                            .child(
                                h_flex()
                                    .gap(px(8.))
                                    .child(preset("Yesterday", DateScope::Yesterday, cx))
                                    .child(preset("Last 7 days", DateScope::LastSevenDays, cx)),
                            )
                            .child(ui::divider(cx))
                            .child(
                                h_flex()
                                    .gap(px(8.))
                                    .items_center()
                                    .child(div().flex_none().text_size(ty::SECONDARY).text_color(lb.text).child("Specific date"))
                                    .child(div().flex_1().min_w_0().child(DatePicker::new(&picker).small()))
                                    .child(Button::new("ask-date-use").outline().small().label("Use date").on_click(
                                        move |_, window, cx| {
                                            use_page.update(cx, |page, cx| {
                                                if let Some(day) = page.picked_day {
                                                    page.date_scope = DateScope::Day(day);
                                                    page.scope_changed(window, cx);
                                                }
                                            });
                                            use_popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                                        },
                                    )),
                            )
                    }),
            )
            .when(current != DateScope::AnyTime, |this| {
                this.child(
                    Button::new("ask-date-clear")
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(Ico::CircleX))
                        .tooltip("Clear date filter")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.date_scope = DateScope::AnyTime;
                            this.scope_changed(window, cx);
                        })),
                )
            })
    }

    fn render_inference_status(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let lb = cx.lb().clone();
        let inference = inference(cx);
        let tint = if inference.blocked {
            lb.attention_text
        } else if inference.remote {
            lb.accent_text
        } else {
            lb.text_secondary
        };
        let help = if inference.remote {
            "Answers send your question and selected evidence to the configured server. Open model settings."
        } else {
            "Answers and selected evidence are processed on this PC. Open model settings."
        };
        h_flex().child(
            h_flex()
                .id("ask-inference-status")
                .gap(px(5.))
                .items_center()
                .min_h(px(24.))
                .cursor_pointer()
                .text_size(ty::SECONDARY)
                .text_color(if inference.remote || inference.blocked {
                    lb.text
                } else {
                    lb.text_secondary
                })
                .hover(|s| s.underline())
                .child(ui::icon(inference.icon, px(14.), tint))
                .child(inference.label)
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(help).build(window, cx)
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.data.update(cx, |data, cx| {
                        data.open_settings(SettingsCategory::Models, cx)
                    });
                })),
        )
    }
}
