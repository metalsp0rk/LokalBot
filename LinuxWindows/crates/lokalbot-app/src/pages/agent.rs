//! Agent (`AgentView.swift`). Agent Mode is not available on Linux and
//! Windows yet; this page explains it and sets up the other direction:
//! coding agents using LokalBot through `lokalbot-cli` and MCP.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Div, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    div, px,
};

use crate::pages::Page;
use crate::pages::settings::advanced::cli_install_path;
use crate::state::{AppData, AppEvent, SettingsCategory};
use crate::theme::{LbThemeExt as _, metric, ty};
use crate::ui::{self, Ico};

pub struct AgentPage {
    data: Entity<AppData>,
    on_page: bool,
    meetings_access: Option<bool>,
    cli_installed: Option<bool>,
    copied: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

impl AgentPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let data = AppData::global(cx);
        // Re-check the marker and the installed command each time the page opens.
        let subscriptions = vec![cx.observe(&data, |this, data, cx| {
            let on_page = data.read(cx).section == crate::state::Section::Agent;
            if on_page && !this.on_page {
                this.refresh(cx);
            }
            this.on_page = on_page;
        })];
        let on_page = data.read(cx).section == crate::state::Section::Agent;
        let mut page =
            Self { data, on_page, meetings_access: None, cli_installed: None, copied: None, _subscriptions: subscriptions };
        page.refresh(cx);
        page
    }

    /// Reads the access marker and the installed command off the main thread.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let paths = self.data.read(cx).library.paths().clone();
        cx.spawn(async move |this, cx| {
            let (access, installed) = cx
                .background_spawn(async move {
                    (lokalbot_core::access::meetings_access_enabled(&paths), cli_install_path().exists())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.meetings_access != Some(access) || this.cli_installed != Some(installed) {
                    this.meetings_access = Some(access);
                    this.cli_installed = Some(installed);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn set_access(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.meetings_access = Some(enabled);
        let paths = self.data.read(cx).library.paths().clone();
        let data = self.data.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { lokalbot_core::access::set_meetings_access(&paths, enabled) }).await;
            if let Err(err) = result {
                let _ = data.update(cx, |data, cx| data.show_error(format!("Could not change agent access: {err}"), cx));
            }
            let _ = this.update(cx, |this, cx| this.refresh(cx));
        })
        .detach();
        cx.notify();
    }

    fn copy(&mut self, key: &'static str, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = Some(key);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(1500)).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied == Some(key) {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A command in a monospaced well with a Copy button.
    fn command(&self, key: &'static str, label: &'static str, command: String, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let copied = self.copied == Some(key);
        let text = command.clone();
        v_flex()
            .gap(px(6.))
            .child(div().text_size(ty::SECONDARY).font_weight(FontWeight::SEMIBOLD).text_color(lb.text_secondary).child(label))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .bg(lb.chip_fill)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(cx.theme_mono_font())
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text)
                            .child(command),
                    )
                    .child(
                        Button::new(SharedString::from(format!("agent-copy-{key}")))
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(if copied { Ico::Check } else { Ico::Copy }))
                            .label(if copied { "Copied" } else { "Copy" })
                            .on_click(cx.listener(move |this, _, _, cx| this.copy(key, text.clone(), cx))),
                    ),
            )
    }

    fn step(number: usize, title: &'static str, content: impl IntoElement, cx: &App) -> Div {
        let lb = cx.lb();
        h_flex()
            .gap(px(12.))
            .items_start()
            .child(
                div()
                    .size(px(22.))
                    .flex_none()
                    .rounded_full()
                    .bg(lb.accent_wash)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(ty::SECONDARY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(lb.accent_text)
                    .child(number.to_string()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.))
                    .child(div().pt(px(1.)).text_size(ty::BODY).font_weight(FontWeight::SEMIBOLD).text_color(lb.text).child(title))
                    .child(content),
            )
    }
}

/// The mono font family from the GPUI Kit theme.
trait MonoFont {
    fn theme_mono_font(&self) -> SharedString;
}

impl MonoFont for App {
    fn theme_mono_font(&self) -> SharedString {
        use gpui_kit::component::ActiveTheme as _;
        self.theme().mono_font_family.clone()
    }
}

impl Page for AgentPage {
    fn title(&self, _cx: &App) -> SharedString {
        "Agent".into()
    }
}

impl Render for AgentPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lb = cx.lb().clone();
        let path = cli_install_path();
        let path_text = path.display().to_string();
        let installed = self.cli_installed;
        let access = self.meetings_access.unwrap_or(false);
        let install_data = self.data.clone();
        let settings_data = self.data.clone();
        let page = cx.entity();

        let install = v_flex()
            .gap(px(8.))
            .child(ui::settings_row::help(
                format!("Adds lokalbot-cli at {path_text} and the LokalBot skill to ~/.agents/skills and ~/.claude/skills."),
                cx,
            ))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(
                        Button::new("agent-install-cli")
                            .outline()
                            .small()
                            .icon(Icon::new(Ico::Terminal))
                            .label(if installed == Some(true) { "Reinstall…" } else { "Install for your coding agent…" })
                            .on_click(move |_, _, cx| install_data.update(cx, |data, cx| data.request(AppEvent::InstallCli, cx))),
                    )
                    .when_some(installed, |this, installed| {
                        let (glyph, text, color) = if installed {
                            (Ico::CircleCheck, "Installed", lb.success_text)
                        } else {
                            (Ico::CircleAlert, "Not installed yet", lb.text_secondary)
                        };
                        this.child(
                            h_flex()
                                .gap(px(5.))
                                .items_center()
                                .child(ui::icon(glyph, px(13.), color))
                                .child(div().text_size(ty::SECONDARY).text_color(color).child(text)),
                        )
                    }),
            );

        let access_row = h_flex()
            .gap(px(12.))
            .items_center()
            .child(ui::settings_row::help(
                "Lets MCP clients and the lokalbot-cli skill list, read, search, and ask about your meetings — read-only and on this PC only. Screen memory has its own switch in Settings → Privacy & Data.",
                cx,
            ).flex_1().min_w_0())
            .child(Switch::new("agent-meetings-access").checked(access).on_click(move |on, _, cx| {
                let on = *on;
                page.update(cx, |page, cx| page.set_access(on, cx))
            }));

        let command = if cfg!(target_os = "windows") { format!("\"{path_text}\"") } else { path_text.clone() };
        let connect = v_flex()
            .gap(px(12.))
            .child(self.command("claude", "Claude Code", format!("claude mcp add lokalbot -- {command} mcp"), cx))
            .child(self.command("codex", "Codex", format!("codex mcp add lokalbot -- {command} mcp"), cx))
            .child(self.command(
                "json",
                "Other MCP clients",
                format!("{{\"mcpServers\": {{\"lokalbot\": {{\"command\": \"{}\", \"args\": [\"mcp\"]}}}}}}", path_text.replace('\\', "\\\\")),
                cx,
            ))
            .child(self.command("brief", "Try it in a terminal", "lokalbot-cli brief".into(), cx));

        div().id("agent-page").size_full().overflow_y_scroll().child(
            v_flex().w_full().items_center().child(
                v_flex()
                    .w_full()
                    .max_w(metric::READING_MAX_WIDTH + metric::DETAIL_PADDING * 2.)
                    .p(metric::DETAIL_PADDING)
                    .gap(metric::SECTION_SPACING)
                    .child(
                        v_flex()
                            .items_center()
                            .gap(px(10.))
                            .pt(px(12.))
                            .pb(px(4.))
                            .child(ui::icon_tile(Ico::WandSparkles, lb.accent_text, px(56.)))
                            .child(div().text_size(ty::PAGE_TITLE).font_weight(FontWeight::BOLD).text_color(lb.text).child("Agent Mode"))
                            .child(
                                div()
                                    .max_w(px(520.))
                                    .text_center()
                                    .text_size(ty::BODY)
                                    .line_height(px(21.))
                                    .text_color(lb.text_secondary)
                                    .child("A local coding and file agent powered by your selected Think model. Agent Mode is coming to Linux and Windows; until then, your own coding agent can use LokalBot's memory of your meetings."),
                            ),
                    )
                    .child(ui::section(
                        "Use LokalBot from your coding agent",
                        v_flex()
                            .gap(px(18.))
                            .child(Self::step(1, "Install the command", install, cx))
                            .child(Self::step(2, "Let agents read your meetings", access_row, cx))
                            .child(Self::step(3, "Connect your agent", connect, cx)),
                        cx,
                    ))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_start()
                            .child(ui::icon(Ico::ShieldCheck, px(14.), lb.text_secondary))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap(px(6.))
                                    .child(ui::settings_row::help(
                                        "Read-only by design: agents can read what you allow and nothing else. LokalBot never sends your library anywhere; your agent decides what to do with what it reads.",
                                        cx,
                                    ))
                                    .child(div().child(ui::settings_row::link_button(
                                        "agent-privacy-settings",
                                        "Manage agent access in Settings…",
                                        move |_, cx| settings_data.update(cx, |data, cx| data.open_settings(SettingsCategory::Privacy, cx)),
                                        cx,
                                    ))),
                            ),
                    ),
            ),
        )
    }
}
