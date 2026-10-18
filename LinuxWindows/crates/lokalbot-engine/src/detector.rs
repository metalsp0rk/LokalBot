//! Meeting detection and the recording mode it follows (`MeetingDetector.swift`):
//! record automatically, ask through a notification, or never auto-record.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::Sender;
use parking_lot::Mutex;

use lokalbot_core::settings::AutoRecordMode;
use lokalbot_platform::{DetectorEvent, MeetingDetector};

use crate::{Command, EngineEvent, Shared};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub struct Detector {
    stop: Arc<AtomicBool>,
}

impl Detector {
    pub fn start(
        shared: Arc<Shared>,
        commands: Sender<Command>,
        detector: Arc<Mutex<MeetingDetector>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::Builder::new()
            .name("lokalbot-meeting-detector".into())
            .spawn(move || run(shared, commands, flag, detector))
            .expect("spawning the meeting detector");
        Self { stop }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(
    shared: Arc<Shared>,
    commands: Sender<Command>,
    stop: Arc<AtomicBool>,
    detector: Arc<Mutex<MeetingDetector>>,
) {
    // The process whose call LokalBot started recording automatically.
    let mut auto_recording: Option<u32> = None;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(POLL_INTERVAL);
        let settings = shared.settings();
        let events = {
            let mut detector = detector.lock();
            detector.set_stop_debounce(Duration::from_secs_f64(
                settings.stop_debounce_seconds.clamp(5.0, 120.0),
            ));
            detector.poll()
        };
        for event in events {
            match event {
                DetectorEvent::Started(meeting) => {
                    if shared.recording_active.load(Ordering::Relaxed) {
                        continue;
                    }
                    let title = meeting
                        .title_hint
                        .clone()
                        .unwrap_or_else(|| format!("{} meeting", meeting.app_name));
                    match settings.auto_record_mode {
                        AutoRecordMode::Automatic => {
                            auto_recording = Some(meeting.pid);
                            let _ = commands.send(Command::StartRecording {
                                title: Some(title),
                                app_name: Some(meeting.app_name),
                            });
                        }
                        AutoRecordMode::Ask => {
                            shared.emit(EngineEvent::MeetingDetected {
                                app_name: meeting.app_name,
                                title,
                            });
                        }
                        AutoRecordMode::Manual => {}
                    }
                }
                DetectorEvent::Ended(meeting) => {
                    if auto_recording == Some(meeting.pid) {
                        auto_recording = None;
                        let _ = commands.send(Command::StopRecording);
                    }
                }
            }
        }
    }
}
