//! Dictation: capture while the shortcut is held, transcribe on this PC,
//! optionally clean up or compose with Think, then insert at the cursor
//! (`DictationCoordinator.swift`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use parking_lot::Mutex;

use lokalbot_audio::MicCapture;
use lokalbot_core::settings::{DictationIntent, DictationOutputMode};
use lokalbot_platform::{InsertMethod, InsertOutcome};

use crate::models::Models;
use crate::{DictationCommand, DictationUpdate, EngineEvent, Shared};

pub struct Dictation {
    shared: Arc<Shared>,
    models: Models,
    state: Arc<Mutex<State>>,
}

enum State {
    Idle,
    Listening {
        capture: MicCapture,
        started: Instant,
        ticker_stop: Arc<AtomicBool>,
    },
    Working,
}

impl Dictation {
    pub fn new(shared: Arc<Shared>, models: Models) -> Self {
        Self {
            shared,
            models,
            state: Arc::new(Mutex::new(State::Idle)),
        }
    }

    pub fn handle(&self, command: DictationCommand) {
        let listening = matches!(*self.state.lock(), State::Listening { .. });
        match command {
            DictationCommand::Start if !listening => self.start(),
            DictationCommand::Stop if listening => self.finish(),
            DictationCommand::Toggle => {
                if listening {
                    self.finish()
                } else {
                    self.start()
                }
            }
            DictationCommand::Cancel if listening => self.cancel(),
            _ => {}
        }
    }

    fn start(&self) {
        if !matches!(*self.state.lock(), State::Idle) {
            return;
        }
        if !self
            .models
            .missing(&self.models.dictation_requirements())
            .is_empty()
        {
            self.shared
                .emit(EngineEvent::Dictation(DictationUpdate::Failed {
                    message:
                        "Dictation needs the transcription model. Download it in Settings → Models."
                            .into(),
                }));
            return;
        }
        self.shared
            .emit(EngineEvent::Dictation(DictationUpdate::Starting));
        let settings = self.shared.settings();
        let device = Some(settings.dictation_microphone_id.clone()).filter(|d| !d.is_empty());
        let capture = match MicCapture::start(device.as_deref()) {
            Ok(capture) => capture,
            Err(err) => {
                self.shared
                    .emit(EngineEvent::Dictation(DictationUpdate::Failed {
                        message: format!("The microphone could not start: {err:#}"),
                    }));
                return;
            }
        };
        self.shared.capture_active.store(true, Ordering::Relaxed);
        let ticker_stop = Arc::new(AtomicBool::new(false));
        {
            let state = self.state.clone();
            let shared = self.shared.clone();
            let stop = ticker_stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let State::Listening {
                        capture, started, ..
                    } = &*state.lock()
                    {
                        shared.emit(EngineEvent::Dictation(DictationUpdate::Listening {
                            level: capture.level(),
                            elapsed_seconds: started.elapsed().as_secs_f64(),
                            preview: String::new(),
                        }));
                    }
                    std::thread::sleep(Duration::from_millis(80));
                }
            });
        }
        *self.state.lock() = State::Listening {
            capture,
            started: Instant::now(),
            ticker_stop,
        };
    }

    fn cancel(&self) {
        let previous = std::mem::replace(&mut *self.state.lock(), State::Idle);
        if let State::Listening {
            capture,
            ticker_stop,
            ..
        } = previous
        {
            ticker_stop.store(true, Ordering::Relaxed);
            drop(capture.stop());
        }
        self.shared.capture_active.store(
            self.shared.recording_active.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.shared
            .emit(EngineEvent::Dictation(DictationUpdate::Cancelled));
    }

    fn finish(&self) {
        let previous = std::mem::replace(&mut *self.state.lock(), State::Working);
        let State::Listening {
            capture,
            ticker_stop,
            ..
        } = previous
        else {
            *self.state.lock() = State::Idle;
            return;
        };
        ticker_stop.store(true, Ordering::Relaxed);
        let samples = capture.stop();
        self.shared.capture_active.store(
            self.shared.recording_active.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        let shared = self.shared.clone();
        let state = self.state.clone();
        std::thread::spawn(move || {
            let update = match process(&shared, &samples) {
                Ok(update) => update,
                Err(err) => DictationUpdate::Failed {
                    message: format!("{err:#}"),
                },
            };
            *state.lock() = State::Idle;
            shared.emit(EngineEvent::Dictation(update));
        });
    }
}

fn process(shared: &Shared, samples: &[f32]) -> Result<DictationUpdate> {
    let settings = shared.settings();
    shared.emit(EngineEvent::Dictation(DictationUpdate::Transcribing));
    let language =
        Some(settings.transcription_language.clone()).filter(|l| l != "auto" && !l.is_empty());
    let mut text = shared
        .speech
        .with(shared, &settings.transcription_model, |t| {
            t.transcribe_dictation(samples, language.as_deref())
        })?;
    if text.trim().is_empty() {
        return Err(anyhow!("No speech detected."));
    }
    if settings.dictation_intent == DictationIntent::Compose {
        shared.emit(EngineEvent::Dictation(DictationUpdate::Composing));
        let think = shared.think(&settings)?;
        text = crate::compat::prepare_dictation(&think, &text, &settings)?;
    }
    let (inserted, notice) = match settings.dictation_output_mode {
        DictationOutputMode::Paste => {
            match lokalbot_platform::insert_text(&text, InsertMethod::Paste) {
                Ok(InsertOutcome::Inserted) => (true, None),
                Ok(InsertOutcome::CopiedToClipboard) => {
                    (false, Some("Copied to the clipboard".to_string()))
                }
                Err(err) => {
                    lokalbot_platform::copy_to_clipboard(&text)?;
                    (false, Some(format!("Copied to the clipboard ({err})")))
                }
            }
        }
        DictationOutputMode::Clipboard => {
            lokalbot_platform::copy_to_clipboard(&text)?;
            (false, Some("Copied to the clipboard".to_string()))
        }
        DictationOutputMode::ShowInApp => (false, None),
    };
    Ok(DictationUpdate::Finished {
        text,
        inserted,
        notice,
    })
}
