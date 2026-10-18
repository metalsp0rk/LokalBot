//! Day Memory: activity tracking, screen context, coding agent sessions and
//! the day digest.

use gpui_kit::component::button::Button;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, ParentElement as _, SharedString, Styled as _, div, px};
use lokalbot_core::ScreenContextMode;

use super::{Section, SettingsPage};
use crate::state::SettingsCategory;
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::settings_row;

/// The first characters of the digest instructions that are used.
pub const DIGEST_PROMPT_LIMIT: usize = 500;

fn mode_detail(mode: ScreenContextMode) -> &'static str {
    match mode {
        ScreenContextMode::ActivityOnly => "Records the active app and window title only.",
        ScreenContextMode::Text => {
            "Reads visible interface text through accessibility; no screenshots are stored."
        }
        ScreenContextMode::TextAndVisual => {
            "Pairs accessible text with an encrypted screenshot, using local OCR only when needed."
        }
    }
}

fn hour_label(hour: u32) -> String {
    match hour % 24 {
        0 => "12 AM".into(),
        12 => "12 PM".into(),
        h if h < 12 => format!("{h} AM"),
        h => format!("{} PM", h - 12),
    }
}

impl SettingsPage {
    pub(super) fn day_memory_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let settings = self.data.read(cx).settings.clone();
        let data = self.data.clone();
        let lb = cx.lb().clone();
        let captures_text = settings.screen_context_capture_mode != ScreenContextMode::ActivityOnly;

        let tracking_data = data.clone();
        let mode_data = data.clone();
        let retention_data = data.clone();
        let exclusions_data = data.clone();
        let mode_items: Vec<(SharedString, bool)> = ScreenContextMode::ALL
            .iter()
            .map(|m| {
                (
                    SharedString::from(m.label()),
                    *m == settings.screen_context_capture_mode,
                )
            })
            .collect();
        let mut memory = Section::new("Day Memory")
            .row(
                "settings.trackingEnabled",
                settings_row::row(
                    "settings.trackingEnabled",
                    "Track app & window activity",
                    Some("Records which app and window you're using for the Timeline and day digest.".into()),
                    settings_row::switch("settings.trackingEnabled", settings.tracking_enabled, move |on, _, cx| {
                        tracking_data.update(cx, |data, cx| {
                            data.update_settings(
                                |s| {
                                    s.tracking_enabled = on;
                                    if !on {
                                        s.screen_context_capture_mode = ScreenContextMode::ActivityOnly;
                                    }
                                },
                                cx,
                            )
                        })
                    }),
                    self.is_focused("settings.trackingEnabled"),
                    cx,
                ),
            )
            .row(
                "settings.screenContextCaptureMode",
                settings_row::row(
                    "settings.screenContextCaptureMode",
                    "Screen context",
                    Some(mode_detail(settings.screen_context_capture_mode).into()),
                    settings_row::picker(
                        "settings-screen-context",
                        settings.screen_context_capture_mode.label(),
                        mode_items,
                        move |index, _, cx| {
                            let mode = ScreenContextMode::ALL[index];
                            mode_data.update(cx, |data, cx| {
                                data.update_settings(
                                    |s| {
                                        s.screen_context_capture_mode = mode;
                                        if mode != ScreenContextMode::ActivityOnly {
                                            s.tracking_enabled = true;
                                        }
                                    },
                                    cx,
                                )
                            })
                        },
                    ),
                    self.is_focused("settings.screenContextCaptureMode"),
                    cx,
                ),
            );
        if captures_text {
            let minutes = settings.screenshot_interval_minutes.round() as u32;
            memory = memory
                .row(
                    "settings.screenshotIntervalMinutes",
                    settings_row::stacked_row(
                        "settings.screenshotIntervalMinutes",
                        format!("Idle fallback: at least every {minutes} min"),
                        None,
                        Slider::new(&self.interval_slider).w_full(),
                        self.is_focused("settings.screenshotIntervalMinutes"),
                        cx,
                    ),
                )
                .row(
                    "settings.suggestActionCompletion",
                    self.toggle(
                        "settings.suggestActionCompletion",
                        "Suggest actions that look done",
                        Some("When later captured text shows an action's words with a completion such as “message sent” or “merged”, the action offers Mark Done. Nothing changes until you confirm."),
                        settings.suggest_action_completion,
                        |s, v| s.suggest_action_completion = v,
                        cx,
                    ),
                )
                .row(
                    "settings.captureManage",
                    settings_row::content_row(
                        "settings.captureManage",
                        h_flex()
                            .gap(px(8.))
                            .child(Button::new("settings-manage-retention").outline().small().label("Manage retention and cleanup…").on_click(
                                move |_, _, cx| retention_data.update(cx, |data, cx| data.open_settings(SettingsCategory::Privacy, cx)),
                            ))
                            .child(Button::new("settings-manage-exclusions").outline().small().label("Manage capture exclusions…").on_click(
                                move |_, _, cx| exclusions_data.update(cx, |data, cx| data.open_settings(SettingsCategory::Privacy, cx)),
                            )),
                        false,
                        cx,
                    ),
                )
                .row(
                    "settings.captureDetails",
                    settings_row::content_row(
                        "settings.captureDetails",
                        self.details(
                            "settings.captureDetails",
                            "How screen context is captured",
                            "Captures context after app or window changes, clicks, typing pauses, settled scrolls, or a clipboard change, without storing raw keys, pointer positions, or clipboard contents. Accessible text is preferred; local OCR fills gaps. Private windows, excluded domains, secure fields, and detected credentials are never captured. Visuals are encrypted on disk; extracted text follows the same retention (see Privacy & Data). Saved moments keep their encrypted frame and text until you unsave or delete them. Excluded apps log as “Private”.",
                            cx,
                        ),
                        false,
                        cx,
                    ),
                );
        }

        let agents_enabled = settings.coding_agent_evidence_enabled;
        let mut agents = Section::new("Coding agent sessions").row(
            "settings.codingAgentEvidenceEnabled",
            self.toggle(
                "settings.codingAgentEvidenceEnabled",
                "Read coding agent sessions",
                Some("Adds Claude Code and Codex sessions on this PC to the day digest: your requests, changed file names, commits, and pull requests. Tool output and file contents are never read."),
                agents_enabled,
                |s, v| s.coding_agent_evidence_enabled = v,
                cx,
            ),
        );
        if agents_enabled {
            agents = agents
                .row(
                    "settings.codingAgentReadsClaudeCode",
                    self.toggle(
                        "settings.codingAgentReadsClaudeCode",
                        "Claude Code",
                        None,
                        settings.coding_agent_reads_claude_code,
                        |s, v| s.coding_agent_reads_claude_code = v,
                        cx,
                    ),
                )
                .row(
                    "settings.codingAgentReadsCodex",
                    self.toggle(
                        "settings.codingAgentReadsCodex",
                        "Codex",
                        None,
                        settings.coding_agent_reads_codex,
                        |s, v| s.coding_agent_reads_codex = v,
                        cx,
                    ),
                );
        }
        agents = agents.row(
            "settings.codingAgentDetails",
            settings_row::content_row(
                "settings.codingAgentDetails",
                self.details(
                    "settings.codingAgentDetails",
                    "What is read",
                    "LokalBot reads the session files in ~/.claude/projects and ~/.codex/sessions, never the agents' credentials or settings. It keeps your requests, the agent's final report, changed file names, and actions such as commits, pull requests, pushes, and test runs. Tool output, command output, file contents, and reasoning are skipped, and detected credentials are redacted. Work in progress is added after ten quiet minutes. Saved sessions follow screen-text retention, and turning this off keeps them until you delete them. The agents' own files are never changed.",
                    cx,
                ),
                false,
                cx,
            ),
        );

        let hour = settings.day_digest_hour.min(23);
        let hour_data = data.clone();
        let used = settings.day_digest_custom_prompt.trim().chars().count();
        let counter = if used > DIGEST_PROMPT_LIMIT {
            Some((
                format!(
                    "{used} of {DIGEST_PROMPT_LIMIT} characters. Only the first {DIGEST_PROMPT_LIMIT} are used; shorten the rest."
                ),
                lb.attention_text,
            ))
        } else if used > 0 {
            Some((
                format!("{used} of {DIGEST_PROMPT_LIMIT} characters"),
                lb.text_secondary,
            ))
        } else {
            None
        };
        let mut digest = Section::new("Day digest").row(
            "settings.dayDigestAutoEnabled",
            self.toggle(
                "settings.dayDigestAutoEnabled",
                "Generate the day digest automatically",
                Some("Writes the Timeline digest to your local journal at the chosen time, then finalizes yesterday after midnight so late activity is included."),
                settings.day_digest_auto_enabled,
                |s, v| s.day_digest_auto_enabled = v,
                cx,
            ),
        );
        if settings.day_digest_auto_enabled {
            digest = digest.row(
                "settings.dayDigestHour",
                settings_row::row(
                    "settings.dayDigestHour",
                    format!("Generate at {}", hour_label(hour)),
                    None,
                    settings_row::stepper(
                        "settings-digest-hour",
                        "",
                        hour > 0,
                        hour < 23,
                        move |step, _, cx| {
                            let next = (hour as i32 + step).clamp(0, 23) as u32;
                            hour_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.day_digest_hour = next, cx)
                            });
                        },
                        cx,
                    ),
                    self.is_focused("settings.dayDigestHour"),
                    cx,
                ),
            );
        }
        digest = digest.row(
            "settings.dayDigestCustomPrompt",
            settings_row::stacked_row(
                "settings.dayDigestCustomPrompt",
                "Digest instructions (optional)",
                Some(format!("Shapes both scheduled and manual digests. The first {DIGEST_PROMPT_LIMIT} characters are used.").into()),
                v_flex()
                    .gap(px(6.))
                    .child(Textarea::new(&self.digest_prompt).h(px(72.)).text_size(ty::BODY))
                    .when_some(counter, |this, (text, color)| {
                        this.child(div().text_size(ty::CAPTION).text_color(color).child(text))
                    }),
                self.is_focused("settings.dayDigestCustomPrompt"),
                cx,
            ),
        );

        vec![memory, agents, digest]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_read_like_the_mac() {
        assert_eq!(hour_label(18), "6 PM");
        assert_eq!(hour_label(0), "12 AM");
        assert_eq!(hour_label(12), "12 PM");
        assert_eq!(hour_label(9), "9 AM");
    }
}
