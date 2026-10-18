//! Desktop integration: the tray icon, global shortcuts, commands from a
//! second `lokalbot` process, and notifications. Platform callbacks arrive on
//! their own threads and are forwarded to the UI thread through one channel.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, App, Entity, Global};
use parking_lot::Mutex;

use lokalbot_core::settings::DictationTriggerMode;
use lokalbot_engine::{Command, DictationCommand};
use lokalbot_platform::shortcuts::{GlobalShortcuts, Shortcut, ShortcutEvent};
use lokalbot_platform::tray::{self, Tray, TrayEvent, TrayMenuState, item_ids};
use lokalbot_platform::{IpcCommand, IpcServer};

use crate::state::{AppData, MeetingTab, RecordingState, SettingsCategory};

const DICTATION_ID: &str = "dictation";
const QUICK_RECALL_ID: &str = "quick-recall";
/// A dictation press shorter than this keeps recording until the next tap
/// ("Tap or hold").
const TAP_THRESHOLD: Duration = Duration::from_millis(300);

enum DesktopEvent {
    Tray(TrayEvent),
    Shortcut(ShortcutEvent),
    Ipc(IpcCommand),
    /// The user chose an action on the "Meeting detected" notification.
    RecordDetected {
        title: String,
        app_name: String,
    },
}

/// Handles that must live as long as the app.
pub struct Desktop {
    tray: Option<Tray>,
    tray_state: Mutex<Option<TrayMenuState>>,
    shortcuts: Arc<Mutex<Option<GlobalShortcuts>>>,
    _ipc: Option<IpcServer>,
    sender: smol::channel::Sender<DesktopEvent>,
    dictation_pressed_at: Mutex<Option<Instant>>,
    pub main_window: Mutex<Option<AnyWindowHandle>>,
}

impl Desktop {
    pub fn has_tray(&self) -> bool {
        self.tray.is_some()
    }
}

pub struct DesktopHandle(pub Arc<Desktop>);

impl Global for DesktopHandle {}

pub fn desktop(cx: &App) -> Option<Arc<Desktop>> {
    cx.try_global::<DesktopHandle>().map(|d| d.0.clone())
}

/// Starts the tray, the IPC listener and (in the background) global shortcuts.
pub fn start(data: &Entity<AppData>, root: &Path, cx: &mut App) {
    let (sender, receiver) = smol::channel::unbounded::<DesktopEvent>();

    let tray = {
        let tx = sender.clone();
        match tray_icon_rgba() {
            Some(icon) => match Tray::new(
                icon,
                "LokalBot",
                tray::standard_items(&tray_state(data.read(cx))),
                Box::new(move |event| {
                    let _ = tx.send_blocking(DesktopEvent::Tray(event));
                }),
            ) {
                Ok(tray) => Some(tray),
                Err(err) => {
                    log::warn!("no tray icon: {err:#}");
                    None
                }
            },
            None => None,
        }
    };

    let ipc = {
        let tx = sender.clone();
        match lokalbot_platform::listen(root, move |command| {
            let _ = tx.send_blocking(DesktopEvent::Ipc(command));
        }) {
            Ok(server) => Some(server),
            Err(err) => {
                log::warn!("commands from other lokalbot processes are unavailable: {err}");
                None
            }
        }
    };

    let desktop = Arc::new(Desktop {
        tray,
        tray_state: Mutex::new(None),
        shortcuts: Arc::new(Mutex::new(None)),
        _ipc: ipc,
        sender: sender.clone(),
        dictation_pressed_at: Mutex::new(None),
        main_window: Mutex::new(None),
    });
    cx.set_global(DesktopHandle(desktop.clone()));

    // Shortcuts can block for seconds on Wayland while the desktop asks the
    // user, so they are created off the UI thread.
    {
        let shortcuts = desktop.shortcuts.clone();
        let tx = sender;
        std::thread::spawn(move || {
            let handler_tx = tx.clone();
            match GlobalShortcuts::new(move |event| {
                let _ = handler_tx.send_blocking(DesktopEvent::Shortcut(event));
            }) {
                Ok(manager) => *shortcuts.lock() = Some(manager),
                Err(err) => log::warn!("global shortcuts are unavailable: {err}"),
            }
        });
    }
    // Register once the manager exists, and again whenever settings change.
    sync_shortcuts_later(cx);

    let data = data.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = receiver.recv().await {
            cx.update(|cx| handle(&data, event, cx));
        }
    })
    .detach();

    // Keep the tray's status line and recording timer current.
    let data_for_tray = AppData::global(cx);
    cx.spawn(async move |cx| {
        loop {
            smol::Timer::after(Duration::from_secs(1)).await;
            cx.update(|cx| refresh_tray(&data_for_tray, cx));
        }
    })
    .detach();
}

fn tray_icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let png = include_bytes!("../../../packaging/icons/lokalbot-32.png");
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Some((image.into_raw(), width, height))
}

fn tray_state(data: &AppData) -> TrayMenuState {
    let (recording, timer, subtitle) = match &data.recording {
        RecordingState::Recording {
            meeting, started, ..
        } => (
            true,
            Some(lokalbot_core::time::timer(started.elapsed().as_secs_f64())),
            meeting.display_title(),
        ),
        _ => (false, None, mode_status(&data.settings).to_string()),
    };
    TrayMenuState {
        recording,
        timer,
        subtitle,
        day_memory_paused: data.day_memory_paused,
    }
}

/// The menu's second line when nothing is recording (`MenuBarView.swift`).
fn mode_status(settings: &lokalbot_core::Settings) -> &'static str {
    use lokalbot_core::settings::AutoRecordMode;
    match settings.auto_record_mode {
        AutoRecordMode::Automatic => "Auto-records detected meetings",
        AutoRecordMode::Ask => "Asks before recording meetings",
        AutoRecordMode::Manual => "Manual recording only",
    }
}

fn refresh_tray(data: &Entity<AppData>, cx: &mut App) {
    let Some(desktop) = desktop(cx) else { return };
    let Some(tray) = desktop.tray.as_ref() else {
        return;
    };
    let state = tray_state(data.read(cx));
    let mut last = desktop.tray_state.lock();
    if last.as_ref() == Some(&state) {
        return;
    }
    let menu_changed = last.as_ref().is_none_or(|l| {
        l.recording != state.recording
            || l.subtitle != state.subtitle
            || l.day_memory_paused != state.day_memory_paused
    });
    if menu_changed {
        let _ = tray.set_items(tray::standard_items(&state));
    }
    let _ = tray.set_recording(state.recording, state.timer.clone());
    *last = Some(state);
}

/// Re-registers global shortcuts from the current settings, off the UI thread.
pub fn sync_shortcuts_later(cx: &mut App) {
    let Some(desktop) = desktop(cx) else { return };
    let data = AppData::global(cx);
    let data = data.read(cx);
    let paused = data.shortcut_recording;
    let dictation = (data.settings.dictation_enabled && !paused)
        .then(|| Shortcut::parse(&data.settings.dictation_shortcut).ok())
        .flatten();
    let quick_recall = (data.settings.quick_recall_enabled && !paused)
        .then(|| Shortcut::parse(&data.settings.quick_recall_shortcut).ok())
        .flatten();
    let shortcuts = desktop.shortcuts.clone();
    std::thread::spawn(move || {
        // The manager may still be starting (Wayland asks the user first).
        for _ in 0..50 {
            if shortcuts.lock().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let mut guard = shortcuts.lock();
        let Some(manager) = guard.as_mut() else {
            return;
        };
        for (id, shortcut) in [(DICTATION_ID, dictation), (QUICK_RECALL_ID, quick_recall)] {
            let _ = manager.unregister(id);
            if let Some(shortcut) = shortcut {
                if let Err(err) = manager.register(id, &shortcut) {
                    log::warn!(
                        "could not register the {id} shortcut {}: {err}",
                        shortcut.label()
                    );
                }
            }
        }
    });
}

fn handle(data: &Entity<AppData>, event: DesktopEvent, cx: &mut App) {
    match event {
        DesktopEvent::Tray(TrayEvent::Activated) => show_main_window(cx),
        DesktopEvent::Tray(TrayEvent::Item(id)) => match id.as_str() {
            item_ids::RECORD => crate::bridge::send(
                Command::StartRecording {
                    title: None,
                    app_name: None,
                },
                cx,
            ),
            item_ids::STOP_RECORDING => crate::bridge::send(Command::StopRecording, cx),
            item_ids::ASK => crate::windows::quick_recall::open(cx),
            item_ids::OPEN => show_main_window(cx),
            item_ids::SETTINGS => {
                show_main_window(cx);
                data.update(cx, |data, cx| {
                    data.open_settings(SettingsCategory::General, cx)
                });
            }
            item_ids::PAUSE_DAY_MEMORY | item_ids::RESUME_DAY_MEMORY => {
                let paused = id == item_ids::PAUSE_DAY_MEMORY;
                data.update(cx, |data, cx| {
                    data.day_memory_paused = paused;
                    cx.notify();
                });
                crate::bridge::send(Command::SetDayMemoryPaused(paused), cx);
            }
            item_ids::QUIT => cx.quit(),
            other => log::debug!("unknown tray item {other}"),
        },
        DesktopEvent::Shortcut(event) => match event.id.as_str() {
            QUICK_RECALL_ID if event.pressed => crate::windows::quick_recall::open(cx),
            DICTATION_ID => dictation_shortcut(data, event.pressed, cx),
            _ => {}
        },
        DesktopEvent::Ipc(command) => match command {
            IpcCommand::Show => show_main_window(cx),
            IpcCommand::QuickRecall => crate::windows::quick_recall::open(cx),
            IpcCommand::ToggleRecording => data.update(cx, |data, cx| data.toggle_recording(cx)),
            IpcCommand::StartDictation => {
                crate::bridge::send(Command::Dictation(DictationCommand::Start), cx)
            }
            IpcCommand::StopDictation => {
                crate::bridge::send(Command::Dictation(DictationCommand::Stop), cx)
            }
            IpcCommand::ToggleDictation => {
                crate::bridge::send(Command::Dictation(DictationCommand::Toggle), cx)
            }
            IpcCommand::OpenMeeting(id) => {
                show_main_window(cx);
                if let Ok(id) = uuid::Uuid::parse_str(&id) {
                    data.update(cx, |data, cx| {
                        data.open_meeting(id, MeetingTab::Summary, None, cx)
                    });
                }
            }
        },
        DesktopEvent::RecordDetected { title, app_name } => crate::bridge::send(
            Command::StartRecording {
                title: Some(title),
                app_name: Some(app_name),
            },
            cx,
        ),
    }
}

/// Push to talk records while held; Toggle starts and stops on presses;
/// Tap or hold keeps recording after a short tap until the next press.
fn dictation_shortcut(data: &Entity<AppData>, pressed: bool, cx: &mut App) {
    let Some(desktop) = desktop(cx) else { return };
    let mode = data.read(cx).settings.dictation_trigger_mode;
    let command = match (mode, pressed) {
        (DictationTriggerMode::PushToTalk, true) => Some(DictationCommand::Start),
        (DictationTriggerMode::PushToTalk, false) => Some(DictationCommand::Stop),
        (DictationTriggerMode::Toggle, true) => Some(DictationCommand::Toggle),
        (DictationTriggerMode::Toggle, false) => None,
        (DictationTriggerMode::TapOrHold, true) => {
            *desktop.dictation_pressed_at.lock() = Some(Instant::now());
            Some(DictationCommand::Toggle)
        }
        (DictationTriggerMode::TapOrHold, false) => {
            let held = desktop
                .dictation_pressed_at
                .lock()
                .take()
                .map(|at| at.elapsed());
            held.filter(|held| *held >= TAP_THRESHOLD)
                .map(|_| DictationCommand::Stop)
        }
    };
    if let Some(command) = command {
        crate::bridge::send(Command::Dictation(command), cx);
    }
}

/// Activates the main window, reopening it if it was closed to the tray.
pub fn show_main_window(cx: &mut App) {
    let existing = desktop(cx).and_then(|d| *d.main_window.lock());
    if let Some(handle) = existing {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }
    crate::open_main_window(AppData::global(cx), cx);
}

/// Shows "Meeting detected — Record <title>?" with Record and Ignore.
pub fn notify_meeting_detected(app_name: String, title: String, cx: &mut App) {
    let Some(desktop) = desktop(cx) else { return };
    let tx = desktop.sender.clone();
    std::thread::spawn(move || {
        let body = format!("Record {title}?");
        let callback_title = title.clone();
        let result = lokalbot_platform::notify::notify_with_timeout(
            "Meeting detected",
            &body,
            &[("record", "Record"), ("ignore", "Ignore")],
            Some(Box::new(move |action| {
                if action == "record" || action == "default" {
                    let _ = tx.send_blocking(DesktopEvent::RecordDetected {
                        title: callback_title.clone(),
                        app_name: app_name.clone(),
                    });
                }
            })),
            Some(lokalbot_platform::notify::MEETING_PROMPT_LIFETIME),
        );
        if let Err(err) = result {
            log::warn!(
                "{}: {err:#}",
                lokalbot_platform::notify::notifications_unavailable_message(&title)
            );
        }
    });
}

/// "Recording saved · <title> · N min", like the macOS notification.
pub fn notify_recording_saved(title: String, minutes: u64) {
    std::thread::spawn(move || {
        let body = format!("{title} · {minutes} min · transcribing…");
        if let Err(err) = lokalbot_platform::notify("Recording saved", &body, &[], None) {
            log::debug!("recording-saved notification failed: {err:#}");
        }
    });
}
