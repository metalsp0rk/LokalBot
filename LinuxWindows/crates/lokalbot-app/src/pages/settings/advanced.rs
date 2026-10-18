//! Advanced: the Agent CLI, logs and build details.

use std::path::PathBuf;

use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{Context, ParentElement as _, SharedString, Styled as _, px};

use super::{Section, SettingsPage};
use crate::state::AppEvent;
use crate::ui::{Ico, settings_row};

/// Where `InstallCli` puts the command for coding agents.
pub fn cli_install_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        directories::BaseDirs::new()
            .map(|dirs| {
                dirs.data_local_dir()
                    .join("LokalBot")
                    .join("bin")
                    .join("lokalbot-cli.exe")
            })
            .unwrap_or_else(|| PathBuf::from("lokalbot-cli.exe"))
    } else {
        directories::BaseDirs::new()
            .map(|dirs| {
                dirs.home_dir()
                    .join(".local")
                    .join("bin")
                    .join("lokalbot-cli")
            })
            .unwrap_or_else(|| PathBuf::from("lokalbot-cli"))
    }
}

/// "Linux · x86_64" / "Windows · x86_64".
pub fn platform_line() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        other => other,
    };
    format!("{os} · {}", std::env::consts::ARCH)
}

impl SettingsPage {
    pub(super) fn advanced_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let (paths, data) = (
            self.data.read(cx).library.paths().clone(),
            self.data.clone(),
        );
        let path = cli_install_path();
        let installed = self.cli_installed;
        let status = match installed {
            Some(true) => settings_row::value("Installed", cx),
            Some(false) => settings_row::value("Not installed", cx),
            None => settings_row::value("Checking…", cx),
        };
        let install_data = data.clone();
        let cli = Section::new("Agent CLI")
            .row("settings.agentCLI", settings_row::row("settings.agentCLI", "Status", None, status, self.is_focused("settings.agentCLI"), cx))
            .row(
                "settings.agentCLIInstall",
                settings_row::content_row(
                    "settings.agentCLIInstall",
                    v_flex()
                        .gap(px(8.))
                        .child(
                            h_flex().gap(px(8.)).child(
                                Button::new("settings-install-cli")
                                    .outline()
                                    .small()
                                    .icon(Icon::new(Ico::Terminal))
                                    .label(if installed == Some(true) { "Reinstall…" } else { "Install for your coding agent…" })
                                    .on_click(move |_, _, cx| {
                                        install_data.update(cx, |data, cx| data.request(AppEvent::InstallCli, cx))
                                    }),
                            ),
                        )
                        .child(settings_row::help(
                            format!(
                                "Installs lokalbot-cli at {} and the LokalBot skill into ~/.agents/skills and ~/.claude/skills. Read-only by design.",
                                path.display()
                            ),
                            cx,
                        )),
                    false,
                    cx,
                ),
            );

        let logs = paths.logs_dir();
        let open_logs = logs.clone();
        let root = paths.root().to_path_buf();
        let diagnostics = Section::new("Logs").row(
            "settings.logs",
            settings_row::row(
                "settings.logs",
                "Logs folder",
                Some(SharedString::from(logs.display().to_string())),
                Button::new("settings-open-logs")
                    .outline()
                    .small()
                    .label("Open Folder")
                    .on_click(move |_, _, cx| {
                        let target = if open_logs.is_dir() {
                            open_logs.clone()
                        } else {
                            root.clone()
                        };
                        cx.open_with_system(&target);
                    }),
                self.is_focused("settings.logs"),
                cx,
            ),
        );

        let build = if cfg!(debug_assertions) {
            "Debug build"
        } else {
            "Release build"
        };
        let system = Section::new("System")
            .row(
                "settings.version",
                settings_row::row(
                    "settings.version",
                    "Version",
                    None,
                    settings_row::value(
                        format!("LokalBot {} · {build}", env!("CARGO_PKG_VERSION")),
                        cx,
                    ),
                    self.is_focused("settings.version"),
                    cx,
                ),
            )
            .row(
                "settings.thisPC",
                settings_row::row(
                    "settings.thisPC",
                    "This PC",
                    None,
                    settings_row::value(platform_line(), cx),
                    false,
                    cx,
                ),
            )
            .row(
                "settings.processors",
                settings_row::row(
                    "settings.processors",
                    "Processor threads",
                    None,
                    settings_row::value(
                        std::thread::available_parallelism()
                            .map(|n| n.get().to_string())
                            .unwrap_or_else(|_| "Unknown".into()),
                        cx,
                    ),
                    false,
                    cx,
                ),
            );

        vec![cli, diagnostics, system]
    }
}
