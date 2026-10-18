//! Starting and stopping meeting recordings (`RecordingController.swift`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context as _, Result};
use parking_lot::Mutex;

use lokalbot_audio::{Recorder, RecorderOptions};
use lokalbot_core::Meeting;

use crate::pipeline::Pipeline;
use crate::{EngineEvent, RecordingUpdate, Shared};

pub struct Recording {
    shared: Arc<Shared>,
    pipeline: Pipeline,
    active: Arc<Mutex<Option<Active>>>,
}

struct Active {
    meeting: Meeting,
    recorder: Recorder,
    ticker_stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

impl Recording {
    pub fn new(shared: Arc<Shared>, pipeline: Pipeline) -> Self {
        Self {
            shared,
            pipeline,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self, title: Option<String>, app_name: Option<String>) {
        if self.active.lock().is_some() {
            self.shared.message("LokalBot is already recording.");
            return;
        }
        if let Err(err) = self.try_start(title, app_name) {
            log::error!("could not start recording: {err:#}");
            self.shared
                .emit(EngineEvent::Recording(RecordingUpdate::Failed {
                    message: format!("Recording could not start: {err:#}"),
                }));
        }
    }

    fn try_start(&self, title: Option<String>, app_name: Option<String>) -> Result<()> {
        let settings = self.shared.settings();
        let library = &self.shared.library;
        let title = title.unwrap_or_else(|| "Manual recording".to_string());
        let app_name = app_name.unwrap_or_else(|| "Recording".to_string());
        let meeting =
            library.create_meeting(&title, &app_name, lokalbot_core::time::now_seconds())?;
        let options = RecorderOptions {
            mic_device: Some(settings.microphone_device.clone()).filter(|d| !d.is_empty()),
            capture_system: settings.record_system_audio,
        };
        let recorder = match Recorder::start(&library.meeting_dir(&meeting), options) {
            Ok(recorder) => recorder,
            Err(err) => {
                let _ = library.delete_meeting(&meeting);
                return Err(err).context("opening the microphone");
            }
        };
        let has_system_audio = recorder.has_system_audio();
        let ticker_stop = Arc::new(AtomicBool::new(false));
        let ticker = {
            let active = self.active.clone();
            let shared = self.shared.clone();
            let stop = ticker_stop.clone();
            std::thread::Builder::new()
                .name("lokalbot-recording-levels".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if let Some(active) = active.lock().as_ref() {
                            let levels = active.recorder.levels();
                            let status = active.recorder.status();
                            shared.emit(EngineEvent::Recording(RecordingUpdate::Levels {
                                mic: levels.mic,
                                system: levels.system,
                                mic_status: status.mic.label().to_string(),
                                system_status: status.system.label().to_string(),
                                elapsed_seconds: active.recorder.elapsed().as_secs_f64(),
                            }));
                        }
                        std::thread::sleep(Duration::from_millis(200));
                    }
                })?
        };
        self.shared.capture_active.store(true, Ordering::Relaxed);
        self.shared.recording_active.store(true, Ordering::Relaxed);
        self.shared
            .emit(EngineEvent::Recording(RecordingUpdate::Started {
                meeting: meeting.clone(),
                has_system_audio,
            }));
        self.shared.emit(EngineEvent::LibraryChanged);
        *self.active.lock() = Some(Active {
            meeting,
            recorder,
            ticker_stop,
            ticker: Some(ticker),
        });
        Ok(())
    }

    pub fn stop(&self) {
        self.finish(true);
    }

    /// The folder of the meeting being recorded, if any.
    pub fn active_folder(&self) -> Option<std::path::PathBuf> {
        let active = self.active.lock();
        active
            .as_ref()
            .map(|active| self.shared.library.meeting_dir(&active.meeting))
    }

    /// On quit: save what was recorded and leave processing for the next launch.
    pub fn stop_for_shutdown(&self) {
        self.finish(false);
    }

    fn finish(&self, process_now: bool) {
        let Some(mut active) = self.active.lock().take() else {
            return;
        };
        active.ticker_stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = active.ticker.take() {
            let _ = ticker.join();
        }
        self.shared.recording_active.store(false, Ordering::Relaxed);
        self.shared.capture_active.store(false, Ordering::Relaxed);
        let mut meeting = active.meeting;
        let library = &self.shared.library;
        let warnings = match active.recorder.stop() {
            Ok(result) => {
                meeting.has_system_track = result.has_system_audio && result.system.is_some();
                meeting.recorded_duration = Some(result.mic_seconds.max(result.system_seconds));
                result.warnings
            }
            Err(err) => vec![format!("Saving the recording reported an error: {err:#}")],
        };
        meeting.ended_at = Some(lokalbot_core::time::now_seconds());
        if let Err(err) = library.save_meeting(&meeting) {
            self.shared
                .error(format!("Could not save the recording details: {err:#}"));
        }
        if let Err(err) = library.reindex_meeting(&meeting) {
            log::warn!("indexing the new meeting failed: {err:#}");
        }
        self.shared
            .emit(EngineEvent::Recording(RecordingUpdate::Stopped {
                meeting_id: meeting.id,
                warnings,
            }));
        self.shared.emit(EngineEvent::LibraryChanged);
        let settings = self.shared.settings();
        if settings.auto_transcribe || settings.auto_summarize {
            if process_now {
                self.pipeline.enqueue(
                    meeting.id,
                    settings.auto_transcribe,
                    settings.auto_summarize,
                    false,
                );
            } else if let Err(err) = library.enqueue_job(
                meeting.id,
                settings.auto_transcribe,
                settings.auto_summarize,
                false,
            ) {
                log::warn!("could not queue the recording for the next launch: {err:#}");
            }
        }
    }
}
