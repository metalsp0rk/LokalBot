//! General: startup, appearance, the Ask shortcut, updates and about.

use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{Context, IntoElement as _, ParentElement as _, SharedString, Styled as _, div};
use lokalbot_core::settings::{AppTheme, TextSize};

use super::{Section, SettingsPage, catalog};
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{Ico, settings_row};

const THEMES: [AppTheme; 3] = [AppTheme::System, AppTheme::Light, AppTheme::Dark];
const SIZES: [TextSize; 5] = [
    TextSize::Small,
    TextSize::Standard,
    TextSize::Large,
    TextSize::Larger,
    TextSize::Largest,
];
pub const REPOSITORY: &str = "https://github.com/stevyhacker/LokalBot";

impl SettingsPage {
    pub(super) fn general_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let settings = self.data.read(cx).settings.clone();
        let data = self.data.clone();
        let theme_index = THEMES
            .iter()
            .position(|t| *t == settings.app_theme)
            .unwrap_or(0);
        let size_index = SIZES
            .iter()
            .position(|t| *t == settings.text_size)
            .unwrap_or(1);

        let general = Section::new("General")
            .row(
                "settings.launchAtLogin",
                self.toggle(
                    "settings.launchAtLogin",
                    "Launch LokalBot at login",
                    Some("Start automatically so it's ready to catch meetings."),
                    settings.launch_at_login,
                    |s, v| s.launch_at_login = v,
                    cx,
                ),
            )
            .row(
                "settings.runInTray",
                self.toggle(
                    "settings.runInTray",
                    "Keep LokalBot running in the system tray",
                    Some("Closing the window keeps LokalBot running in the system tray, ready to catch meetings. Quit from the tray menu."),
                    settings.run_in_tray,
                    |s, v| s.run_in_tray = v,
                    cx,
                ),
            );

        let theme_data = data.clone();
        let size_data = data.clone();
        let appearance = Section::new("Appearance")
            .row(
                "settings.appTheme",
                settings_row::row(
                    "settings.appTheme",
                    "Theme",
                    Some(
                        "Use light or dark windows regardless of the system setting, or follow it."
                            .into(),
                    ),
                    settings_row::segmented(
                        "settings-theme",
                        &THEMES.map(catalog::theme_label),
                        theme_index,
                        move |index, _, cx| {
                            let theme = THEMES[index];
                            theme_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.app_theme = theme, cx)
                            });
                        },
                        cx,
                    ),
                    self.is_focused("settings.appTheme"),
                    cx,
                ),
            )
            .row(
                "settings.textSize",
                settings_row::row(
                    "settings.textSize",
                    "Text size",
                    Some("Scales text across LokalBot's windows.".into()),
                    settings_row::segmented(
                        "settings-text-size",
                        &SIZES.map(catalog::text_size_label),
                        size_index,
                        move |index, _, cx| {
                            let size = SIZES[index];
                            size_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.text_size = size, cx)
                            });
                        },
                        cx,
                    ),
                    self.is_focused("settings.textSize"),
                    cx,
                ),
            )
            .row(
                "settings.textPreview",
                settings_row::content_row(
                    "settings.textPreview",
                    div()
                        .text_size(ty::BODY)
                        .text_color(cx.lb().text_secondary)
                        .child("Meeting notes, transcripts, and actions will look like this."),
                    false,
                    cx,
                ),
            );

        let shortcut_help: SharedString = format!(
            "Press {} in any app to search your work memory or ask a question. LokalBot registers only this shortcut and never reads other keystrokes.",
            settings.quick_recall_shortcut
        )
        .into();
        let shortcut_data = data.clone();
        let shortcut = Section::new("Shortcut")
            .row(
                "settings.quickRecallEnabled",
                settings_row::row(
                    "settings.quickRecallEnabled",
                    "Enable the system-wide Ask shortcut",
                    Some(shortcut_help),
                    settings_row::switch(
                        "settings.quickRecallEnabled",
                        settings.quick_recall_enabled,
                        move |on, _, cx| {
                            shortcut_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.quick_recall_enabled = on, cx)
                            })
                        },
                    ),
                    self.is_focused("settings.quickRecallEnabled"),
                    cx,
                ),
            )
            .row(
                "settings.quickRecallShortcut",
                settings_row::row(
                    "settings.quickRecallShortcut",
                    "Ask shortcut",
                    None,
                    self.quick_recall_shortcut.clone(),
                    self.is_focused("settings.quickRecallShortcut"),
                    cx,
                ),
            );

        let updates = Section::new("Updates")
            .row(
                "settings.checkForUpdates",
                self.toggle(
                    "settings.checkForUpdates",
                    "Check for updates automatically",
                    Some(
                        "Only the release feed and the update you choose to download are fetched.",
                    ),
                    settings.check_for_updates,
                    |s, v| s.check_for_updates = v,
                    cx,
                ),
            )
            .row(
                "settings.currentVersion",
                settings_row::row(
                    "settings.currentVersion",
                    "Current version",
                    None,
                    settings_row::value(env!("CARGO_PKG_VERSION"), cx),
                    false,
                    cx,
                ),
            );

        let about = Section::new("About").row(
            "settings.githubStar",
            settings_row::row(
                "settings.githubStar",
                "Open source",
                Some(
                    "LokalBot is free and built by one person. A GitHub star helps others find it."
                        .into(),
                ),
                Button::new("settings-github-star")
                    .outline()
                    .small()
                    .icon(Icon::new(Ico::Star))
                    .label("Star on GitHub")
                    .on_click(|_, _, cx| cx.open_url(REPOSITORY))
                    .into_any_element(),
                false,
                cx,
            ),
        );

        vec![general, appearance, shortcut, updates, about]
    }
}
