//! Linux: one window probe for the session (X11 or the Wayland compositor)
//! plus one idle probe, with app names from `.desktop` files.

use chrono::Utc;
use lokalbot_core::ActivitySample;

use super::desktop_entries::AppNames;
use super::{ActivitySource, Unavailable, own_window_sample};
use crate::session::Desktop;

/// What a probe knows about the focused window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FocusedWindow {
    pub title: String,
    pub pid: Option<u32>,
    /// X11 `WM_CLASS` class, or the Wayland app id.
    pub class: Option<String>,
    /// X11 `WM_CLASS` instance.
    pub instance: Option<String>,
    /// A desktop-file id the compositor reported (KDE).
    pub desktop_file: Option<String>,
}

pub(super) trait WindowProbe: Send {
    fn focused(&mut self) -> anyhow::Result<Option<FocusedWindow>>;
    fn name(&self) -> &'static str;
    fn limitation(&self) -> Option<String> {
        None
    }
}

pub(super) trait IdleProbe: Send {
    fn idle_seconds(&mut self) -> Option<f64>;
}

/// A session-bus connection whose calls give up after half a second, so a
/// stuck desktop service never stalls sampling.
pub(super) fn session_bus() -> anyhow::Result<zbus::blocking::Connection> {
    Ok(zbus::blocking::connection::Builder::session()?
        .method_timeout(std::time::Duration::from_millis(500))
        .build()?)
}

const NO_IDLE: &str = "LokalBot cannot read idle time on this desktop, so time away from the computer may count as activity.";

pub(super) fn source() -> Box<dyn ActivitySource> {
    let desktop = crate::session::desktop();
    let probe: anyhow::Result<Box<dyn WindowProbe>> = match &desktop {
        Desktop::X11 => {
            super::x11::X11Probe::connect().map(|p| Box::new(p) as Box<dyn WindowProbe>)
        }
        Desktop::Hyprland => super::wayland::Hyprland::new().map(|p| Box::new(p) as _),
        Desktop::Sway => super::wayland::Sway::new().map(|p| Box::new(p) as _),
        Desktop::Kde => super::wayland::Kwin::new().map(|p| Box::new(p) as _),
        Desktop::Gnome => super::wayland::Gnome::new().map(|p| Box::new(p) as _),
        Desktop::OtherWayland(_) => Err(anyhow::anyhow!(
            "LokalBot cannot see which window is in front on {}. Day memory needs X11, GNOME, KDE Plasma, Hyprland or Sway.",
            desktop.label()
        )),
        Desktop::Headless | Desktop::Windows => Err(anyhow::anyhow!(
            "No desktop session was found, so LokalBot cannot see which app is in front."
        )),
    };
    let probe = match probe {
        Ok(probe) => probe,
        Err(err) => {
            log::warn!("activity: {err:#}");
            return Box::new(Unavailable::new(format!("{err:#}")));
        }
    };
    let idle = super::idle::probe_for(&desktop);
    log::info!(
        "activity: {} window probe, idle {}",
        probe.name(),
        if idle.is_some() {
            "available"
        } else {
            "unavailable"
        }
    );
    Box::new(LinuxSource {
        probe,
        idle,
        names: AppNames::new(),
        own_pid: std::process::id(),
    })
}

struct LinuxSource {
    probe: Box<dyn WindowProbe>,
    idle: Option<Box<dyn IdleProbe>>,
    names: AppNames,
    own_pid: u32,
}

impl ActivitySource for LinuxSource {
    fn sample(&mut self) -> anyhow::Result<Option<ActivitySample>> {
        let idle = self.idle_seconds().unwrap_or(0.0);
        let Some(window) = self.probe.focused()? else {
            return Ok(None);
        };
        if window.pid == Some(self.own_pid) {
            return Ok(Some(own_window_sample(idle)));
        }
        let app = self.names.resolve(&window);
        Ok(Some(ActivitySample {
            at: Utc::now(),
            app: app.name,
            app_id: app.id,
            title: window.title,
            url: None,
            idle_seconds: idle,
        }))
    }

    fn idle_seconds(&mut self) -> Option<f64> {
        self.idle.as_mut()?.idle_seconds()
    }

    fn backend_name(&self) -> &'static str {
        self.probe.name()
    }

    fn limitation(&self) -> Option<String> {
        let mut parts: Vec<String> = self.probe.limitation().into_iter().collect();
        if self.idle.is_none() {
            parts.push(NO_IDLE.into());
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}
