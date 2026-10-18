//! Welcome to LokalBot (`OnboardingView.swift`): four steps. Choices stay a
//! draft until "Apply Choices and Start"; model downloads happen only when
//! asked for. macOS permission panes have no equivalent here, so step two
//! checks the microphone, system audio and activity tracking instead.
//! Autocomplete is macOS-only, so it is not offered here.

use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, AppContext as _, Bounds, Context, Div, Entity,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, WindowBounds, WindowKind,
    WindowOptions, div, img, px, relative, size,
};
use lokalbot_core::ScreenContextMode;
use lokalbot_core::settings::{AutoRecordMode, SummarizerBackend};

use crate::pages::ask::composer_inference;
use crate::pages::settings::models::{model_bytes, model_name, models_in_use, size_label};
use crate::state::{AppData, AppEvent};
use crate::theme::{FONT_FAMILY, LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

const WIDTH: f32 = 650.;
const HEIGHT: f32 = 640.;

/// Opens the welcome window, or brings it forward.
pub fn open(cx: &mut App) {
    let existing = super::secondary(cx).onboarding;
    if super::activate(existing, cx) {
        return;
    }
    let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        kind: WindowKind::Normal,
        is_resizable: false,
        is_minimizable: false,
        app_id: Some("com.lokalbot.LokalBot".into()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui_kit::WindowDecorations::Client),
        ..TitleBar::window_options()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title("Welcome to LokalBot");
        cx.new(|cx| Onboarding::new(window, cx))
    }) {
        Ok((handle, _)) => super::secondary(cx).onboarding = Some(handle),
        Err(err) => log::error!("could not open onboarding: {err:#}"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Capture,
    Access,
    Models,
    Review,
}

impl Step {
    const ALL: [Step; 4] = [Step::Capture, Step::Access, Step::Models, Step::Review];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    fn title(self) -> &'static str {
        match self {
            Step::Capture => "Choose What to Remember",
            Step::Access => "Enable the Access You Need",
            Step::Models => "Prepare Your Workflows",
            Step::Review => "Review and Start",
        }
    }
}

/// `CaptureSetupDraft`: applied only on the last step.
#[derive(Clone, Copy)]
struct Draft {
    meeting_mode: AutoRecordMode,
    day_memory: bool,
    context_mode: ScreenContextMode,
}

impl Draft {
    /// Screen context the draft would turn on.
    fn screen_context(&self) -> ScreenContextMode {
        if self.day_memory {
            self.context_mode
        } else {
            ScreenContextMode::ActivityOnly
        }
    }
}

pub struct Onboarding {
    data: Entity<AppData>,
    step: Step,
    forward: bool,
    draft: Draft,
    _subscriptions: Vec<Subscription>,
}

impl Onboarding {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let data = AppData::global(cx);
        let settings = data.read(cx).settings.clone();
        let subscriptions = vec![cx.observe_in(&data, window, |_, _, _, cx| cx.notify())];
        Self {
            data,
            step: Step::Capture,
            forward: true,
            draft: Draft {
                meeting_mode: settings.auto_record_mode,
                day_memory: settings.tracking_enabled,
                context_mode: settings.screen_context_capture_mode,
            },
            _subscriptions: subscriptions,
        }
    }

    fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        self.forward = step > self.step;
        self.step = step;
        cx.notify();
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.draft;
        self.data.update(cx, |data, cx| {
            data.update_settings(
                |s| {
                    s.auto_record_mode = draft.meeting_mode;
                    s.tracking_enabled = draft.day_memory;
                    s.screen_context_capture_mode = draft.screen_context();
                    s.onboarding_completed = true;
                },
                cx,
            )
        });
        super::secondary(cx).onboarding = None;
        window.remove_window();
    }

    fn prepare_models(&mut self, cx: &mut Context<Self>) {
        // The draft is not applied yet, so its screen text model is asked for here.
        let screen_text = self.draft.screen_context() != ScreenContextMode::ActivityOnly;
        self.data.update(cx, |data, cx| {
            data.request(AppEvent::PrepareModels, cx);
            if screen_text && !data.models.screen_text_ready {
                data.request(
                    AppEvent::DownloadModel(lokalbot_ai::ocr::OCR_MODEL_ID.to_string()),
                    cx,
                );
            }
        });
    }

    fn render_capture(&self, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let draft = self.draft;
        let page = cx.entity();
        let mode_page = page.clone();
        let detail_page = page.clone();
        let memory_page = page.clone();
        let retention = match self.data.read(cx).settings.retention_days {
            1 => "1 day".to_string(),
            days => format!("{days} days"),
        };
        let meeting_picker = Button::new("onboarding-meeting-mode")
            .outline()
            .small()
            .label(draft.meeting_mode.label())
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for mode in AutoRecordMode::ALL {
                    let page = mode_page.clone();
                    menu = menu.item(
                        PopupMenuItem::new(mode.label())
                            .checked(mode == draft.meeting_mode)
                            .on_click(move |_, _, cx| {
                                page.update(cx, |this, cx| {
                                    this.draft.meeting_mode = mode;
                                    cx.notify();
                                })
                            }),
                    );
                }
                menu
            });
        let detail_picker = Button::new("onboarding-context-mode")
            .outline()
            .small()
            .label(draft.context_mode.label())
            .dropdown_caret(true)
            .disabled(!draft.day_memory)
            .dropdown_menu(move |mut menu, _, _| {
                for mode in ScreenContextMode::ALL {
                    let page = detail_page.clone();
                    menu = menu.item(
                        PopupMenuItem::new(mode.label())
                            .checked(mode == draft.context_mode)
                            .on_click(move |_, _, cx| {
                                page.update(cx, |this, cx| {
                                    this.draft.context_mode = mode;
                                    cx.notify();
                                })
                            }),
                    );
                }
                menu
            });
        let screen_context = draft.screen_context() != ScreenContextMode::ActivityOnly;
        v_flex()
            .gap(px(16.))
            .child(text(
                "Keep meeting evidence and a useful memory of your day. You can change each choice later in Settings.",
                lb.text_secondary,
            ))
            .child(
                ui::panel(cx)
                    .child(
                        v_flex()
                            .gap(px(12.))
                            .child(
                                v_flex()
                                    .gap(px(6.))
                                    .child(
                                        h_flex()
                                            .gap(px(12.))
                                            .items_center()
                                            .child(div().flex_1().min_w_0().text_size(ty::BODY).text_color(lb.text).child("Detected Meetings"))
                                            .child(meeting_picker),
                                    )
                                    .child(text(
                                        "Ask via notification waits for you to start recording. Manual mode keeps Record Now available.",
                                        lb.text_secondary,
                                    )),
                            )
                            .child(ui::divider(cx))
                            .child(
                                h_flex()
                                    .gap(px(12.))
                                    .items_center()
                                    .child(
                                        v_flex()
                                            .flex_1().min_w_0()
                                            .gap(px(4.))
                                            .child(div().text_size(ty::BODY).text_color(lb.text).child("Day Memory"))
                                            .child(text("Remember app and window activity.", lb.text_secondary)),
                                    )
                                    .child(Switch::new("onboarding-day-memory").checked(draft.day_memory).on_click(
                                        move |on, _, cx| {
                                            let on = *on;
                                            memory_page.update(cx, |this, cx| {
                                                this.draft.day_memory = on;
                                                cx.notify();
                                            })
                                        },
                                    )),
                            )
                            .child(ui::divider(cx))
                            .child(
                                v_flex()
                                    .gap(px(6.))
                                    .child(
                                        h_flex()
                                            .gap(px(12.))
                                            .items_center()
                                            .child(div().flex_1().min_w_0().text_size(ty::BODY).text_color(lb.text).child("Day-Memory Detail"))
                                            .child(detail_picker),
                                    )
                                    .child(text(
                                        crate::pages::settings::day_memory_mode_detail(draft.context_mode),
                                        lb.text_secondary,
                                    )),
                            ),
                    ),
            )
            .child(text(
                if screen_context {
                    format!(
                        "Text and images build on app activity. Images are encrypted on disk; captured text remains searchable under your retention policy ({retention}), and passwords, excluded apps and private windows are never captured."
                    )
                } else {
                    format!(
                        "Day Memory notes the app in front and its window title, never what you type or what is on screen. Window titles are cleared after {retention}; you can change this in Settings → Privacy & Data."
                    )
                },
                lb.text_secondary,
            ))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(ui::icon(Ico::ListChecks, px(14.), lb.text_secondary))
                    .child(text("These choices are applied on the final Review step.", lb.text_secondary)),
            )
    }

    fn render_access(&self, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let (microphones, system_audio) = {
            let data = self.data.read(cx);
            (data.audio_inputs.clone(), data.settings.record_system_audio)
        };
        let microphone_status = match microphones.first() {
            Some(name) => (format!("Found: {name}"), true),
            None => ("Checked when you first record".to_string(), false),
        };
        let wayland = cfg!(target_os = "linux")
            && std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t.eq_ignore_ascii_case("wayland"));
        let activity_detail = if cfg!(target_os = "windows") {
            "Reads the focused app and window title. Windows needs no extra permission for this."
        } else if wayland {
            "Wayland keeps window titles private on many desktops; LokalBot records the app names it can see and fills in titles where the desktop shares them."
        } else {
            "Reads the focused app and window title from the X11 session. No extra permission is needed."
        };
        let system_detail = if cfg!(target_os = "windows") {
            "Records the other side of calls from your speakers or headset through Windows loopback capture."
        } else {
            "Records the other side of calls from your speakers or headset through a PipeWire or PulseAudio monitor."
        };
        v_flex()
            .gap(px(14.))
            .child(text(
                "LokalBot checks what it needs on this PC. Nothing here changes system settings, and you can finish setup and turn features on later.",
                lb.text_secondary,
            ))
            .child(access_row(
                Ico::Mic,
                "Microphone",
                "Records your side of meetings and dictation.",
                microphone_status.0,
                microphone_status.1,
                cx,
            ))
            .child(access_row(
                Ico::Volume2,
                "System audio",
                system_detail,
                if system_audio { "On".into() } else { "Off — microphone only".into() },
                system_audio,
                cx,
            ))
            .child(access_row(
                Ico::Activity,
                "App and window activity",
                activity_detail,
                if self.draft.day_memory { "Used by Day Memory".into() } else { "Off".into() },
                self.draft.day_memory,
                cx,
            ))
            .child(text(
                "The dictation shortcut can be turned on later in Settings → Dictation.",
                lb.text_secondary,
            ))
    }

    fn render_models(&self, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let (settings, status, downloads) = {
            let data = self.data.read(cx);
            (
                data.settings.clone(),
                data.models.clone(),
                data.downloads.clone(),
            )
        };
        let model_status = |id: &str, ready: bool| -> (String, bool) {
            match downloads.get(id) {
                Some(progress) if progress.error.is_some() => ("Download failed".into(), false),
                Some(progress) => (
                    format!("Downloading · {}%", (progress.fraction() * 100.).round()),
                    false,
                ),
                None if ready => ("Ready".into(), true),
                None => ("Download required".into(), false),
            }
        };
        let with_size = |id: &str| format!("{} · {}", model_name(id), size_label(model_bytes(id)));
        let built_in = settings.summarizer_backend == SummarizerBackend::BuiltIn;
        let think_local = settings.think_is_local();
        let think_name = if built_in {
            with_size(&settings.built_in_model_id)
        } else {
            "Your model server".into()
        };
        let speech = model_status(&settings.transcription_model, status.transcription_ready);
        let think = model_status(&settings.built_in_model_id, status.think_ready || !built_in);
        let embedding_id = lokalbot_ai::catalog::EMBEDDING_MODEL_ID;
        let search = settings
            .semantic_search_enabled
            .then(|| model_status(embedding_id, status.embeddings_ready));
        // Everything "Prepare" downloads: Transcribe with voice detection and
        // speaker separation, Think, search by meaning, and the llama.cpp runtime.
        let ocr_id = lokalbot_ai::ocr::OCR_MODEL_ID;
        let screen_text = (self.draft.screen_context() != ScreenContextMode::ActivityOnly)
            .then(|| model_status(ocr_id, status.screen_text_ready));
        let mut needed = models_in_use(&settings);
        if screen_text.is_some() && !needed.iter().any(|id| id == ocr_id) {
            needed.push(ocr_id.to_string());
        }
        let download: u64 = needed
            .iter()
            .filter(|id| !status.installed.contains(id))
            .map(|id| model_bytes(id))
            .sum();
        let page = cx.entity();
        let preparing = !downloads.is_empty();
        v_flex()
            .gap(px(14.))
            .child(text(
                "Meeting processing needs a Transcribe model and a Think model, and search by meaning adds a small embedding model. Downloads can be deferred; captured meetings wait for their selected models.",
                lb.text_secondary,
            ))
            .child(model_row("Speech recognition", with_size(&settings.transcription_model), speech, downloads.get(&settings.transcription_model).map(|d| d.fraction()), cx))
            .child(model_row("Summaries and Ask", think_name, think, downloads.get(&settings.built_in_model_id).map(|d| d.fraction()), cx))
            .when_some(search, |this, search| {
                this.child(model_row("Search by meaning", with_size(embedding_id), search, downloads.get(embedding_id).map(|d| d.fraction()), cx))
            })
            .when_some(screen_text, |this, screen_text| {
                this.child(model_row("Screen text", with_size(ocr_id), screen_text, downloads.get(ocr_id).map(|d| d.fraction()), cx))
            })
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_start()
                    .child(ui::icon(if think_local { Ico::ShieldCheck } else { Ico::Network }, px(14.), lb.text_secondary))
                    .child(text(
                        if think_local {
                            "The configured Think model processes selected context on this PC."
                        } else {
                            "Selected transcript and work context is sent to the approved model server."
                        },
                        lb.text_secondary,
                    )),
            )
            .child(
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(
                        Button::new("onboarding-prepare")
                            .outline()
                            .icon(Icon::new(Ico::CircleArrowDown))
                            .label(if preparing { "Preparing…" } else { "Prepare Selected Models" })
                            .disabled(preparing || download == 0)
                            .on_click(move |_, _, cx| page.update(cx, |this, cx| this.prepare_models(cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(if download == 0 {
                                "Everything is already on this PC.".to_string()
                            } else {
                                format!("About {} to download", size_label(download))
                            }),
                    ),
            )
            .child(text(
                "Downloads use the model provider. They do not send your meeting content. Existing downloaded models are kept.",
                lb.text_secondary,
            ))
    }

    fn render_review(&self, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let status = self.data.read(cx).models.clone();
        let draft = self.draft;
        let day_memory: SharedString = if draft.day_memory {
            draft.context_mode.label().into()
        } else {
            "Off".into()
        };
        let meeting_models = if status.transcription_ready && status.think_ready {
            "Ready to use · not a test result"
        } else {
            "Preparation can continue later"
        };
        let row = |label: &'static str, value: SharedString| {
            h_flex()
                .gap(px(12.))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(ty::BODY)
                        .text_color(lb.text)
                        .child(label),
                )
                .child(
                    div()
                        .text_size(ty::BODY)
                        .text_color(lb.text_secondary)
                        .child(value),
                )
        };
        v_flex()
            .gap(px(16.))
            .child(
                ui::panel(cx).child(
                    v_flex()
                        .gap(px(12.))
                        .child(row("Meeting recording", draft.meeting_mode.label().into()))
                        .child(ui::divider(cx))
                        .child(row("Day memory", day_memory))
                        .child(ui::divider(cx))
                        .child(row("Think model", composer_inference(cx).label))
                        .child(ui::divider(cx))
                        .child(row("Meeting models", meeting_models.into())),
                ),
            )
            .child(text(
                "You can browse, search and edit your existing local library immediately. Setup does not delete any content or change existing remote-server approvals.",
                lb.text_secondary,
            ))
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lb = cx.lb().clone();
        let step = self.step;
        let body = match step {
            Step::Capture => self.render_capture(cx),
            Step::Access => self.render_access(cx),
            Step::Models => self.render_models(cx),
            Step::Review => self.render_review(cx),
        };
        let travel = if self.forward { 24. } else { -24. };
        let dots = Step::ALL.iter().map(|item| {
            div().size(px(6.)).rounded_full().bg(if *item == step {
                lb.accent_text
            } else {
                lb.text_secondary.opacity(0.25)
            })
        });
        let primary_label = match step {
            Step::Access => "Continue with Current Access",
            Step::Review => "Apply Choices and Start",
            _ => "Continue",
        };
        v_flex()
            .size_full()
            .font_family(FONT_FAMILY)
            .bg(lb.window)
            .text_color(lb.text)
            .child(
                TitleBar::new()
                    .bg(lb.title_bar)
                    .border_color(lb.separator)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .justify_center()
                            .text_size(ty::SECONDARY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(lb.text_secondary)
                            .child("Welcome to LokalBot"),
                    ),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap(px(16.))
                    .px(metric::DETAIL_PADDING)
                    .pt(px(24.))
                    .pb(px(8.))
                    .child(img("brand/lokalbot.svg").size(px(64.)))
                    .child(h_flex().gap(px(8.)).children(dots))
                    .child(
                        div()
                            .text_center()
                            .text_size(ty::PAGE_TITLE)
                            .font_weight(FontWeight::BOLD)
                            .text_color(lb.text)
                            .child(step.title()),
                    ),
            )
            .child(
                div()
                    .id("onboarding-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(div().p(px(24.)).w_full().child(body).with_animation(
                        SharedString::from(format!("onboarding-step-{}", step.index())),
                        Animation::new(Duration::from_millis(250)),
                        move |element, delta| {
                            element.left(px(travel * (1. - delta))).opacity(delta)
                        },
                    )),
            )
            .child(ui::divider(cx))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .p(px(24.))
                    .child(
                        div()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(format!("Step {} of 4", step.index() + 1)),
                    )
                    .child(div().flex_1().min_w_0())
                    .child(
                        Button::new("onboarding-back")
                            .outline()
                            .label("Back")
                            .disabled(step == Step::Capture)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let previous = Step::ALL[step.index().saturating_sub(1)];
                                this.go(previous, cx);
                            })),
                    )
                    .child(
                        Button::new("onboarding-continue")
                            .primary()
                            .label(primary_label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if step == Step::Review {
                                    this.finish(window, cx);
                                } else {
                                    this.go(Step::ALL[step.index() + 1], cx);
                                }
                            })),
                    ),
            )
    }
}

fn text(content: impl Into<SharedString>, color: gpui_kit::Hsla) -> Div {
    div()
        .text_size(ty::BODY)
        .line_height(px(20.))
        .text_color(color)
        .child(content.into())
}

fn access_row(
    icon: Ico,
    title: &'static str,
    detail: &'static str,
    status: String,
    good: bool,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let tint = if good {
        lb.success_text
    } else {
        lb.attention_text
    };
    ui::panel(cx)
        .child(
            h_flex()
                .gap(px(12.))
                .items_start()
                .child(ui::icon_tile(icon, lb.accent_text, px(32.)))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(ty::BODY)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(lb.text)
                                .child(title),
                        )
                        .child(text(detail, lb.text_secondary))
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .items_center()
                                .child(ui::icon(
                                    if good {
                                        Ico::CircleCheck
                                    } else {
                                        Ico::CircleAlert
                                    },
                                    px(13.),
                                    tint,
                                ))
                                .child(
                                    div()
                                        .text_size(ty::SECONDARY)
                                        .text_color(tint)
                                        .child(status),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

fn model_row(
    title: &'static str,
    model: String,
    status: (String, bool),
    progress: Option<f32>,
    cx: &App,
) -> AnyElement {
    let lb = cx.lb();
    let (label, good) = status;
    ui::panel(cx)
        .child(
            h_flex()
                .gap(px(12.))
                .items_center()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(ty::BODY)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(lb.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(ty::SECONDARY)
                                .text_color(lb.text_secondary)
                                .child(model),
                        ),
                )
                .child(
                    div()
                        .text_size(ty::SECONDARY)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if good {
                            lb.success_text
                        } else {
                            lb.text_secondary
                        })
                        .child(label),
                ),
        )
        .when_some(progress, |this, fraction| {
            this.child(
                div()
                    .mt(px(10.))
                    .h(px(4.))
                    .w_full()
                    .rounded_full()
                    .bg(lb.chip_fill)
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(lb.accent_fill)
                            .w(relative(fraction.clamp(0.0, 1.0))),
                    ),
            )
        })
        .into_any_element()
}
