//! App-wide state shared by every page: the library, settings, navigation,
//! recording and background-work status.
//!
//! Pages read it with `AppData::global(cx).read(cx)` and change it only through
//! the methods below, so the engine wiring stays in one place.

use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use gpui_kit::{App, AppContext as _, Context, Entity, Global, SharedString};
use lokalbot_core::{ActionStatus, Library, Meeting, Settings};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Today,
    Meetings,
    Timeline,
    People,
    Projects,
    Ask,
    Agent,
    Settings,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Today => "Today",
            Section::Meetings => "Meetings",
            Section::Timeline => "Timeline",
            Section::People => "People",
            Section::Projects => "Projects",
            Section::Ask => "Ask",
            Section::Agent => "Agent",
            Section::Settings => "Settings",
        }
    }
}

/// Tabs of the meeting workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeetingTab {
    #[default]
    Summary,
    Transcript,
    Notes,
    Review,
}

/// Settings categories, in sidebar order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsCategory {
    #[default]
    General,
    Meetings,
    DayMemory,
    Writing,
    Dictation,
    Models,
    Privacy,
    Advanced,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RecordingState {
    Idle,
    Starting,
    Recording {
        meeting: Meeting,
        started: Instant,
        /// False when system audio could not be captured.
        has_system_audio: bool,
        mic_level: f32,
        system_level: f32,
    },
    Stopping,
}

impl RecordingState {
    pub fn is_active(&self) -> bool {
        !matches!(self, RecordingState::Idle)
    }

    pub fn meeting(&self) -> Option<&Meeting> {
        match self {
            RecordingState::Recording { meeting, .. } => Some(meeting),
            _ => None,
        }
    }
}

/// Where a meeting is in transcription and summarization.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcessingStage {
    Queued,
    WaitingForModels,
    PreparingModel(SharedString),
    Transcribing,
    IdentifyingSpeakers,
    Summarizing,
    Failed(SharedString),
}

impl ProcessingStage {
    pub fn label(&self) -> SharedString {
        match self {
            ProcessingStage::Queued => "Queued".into(),
            ProcessingStage::WaitingForModels => "Waiting for models".into(),
            ProcessingStage::PreparingModel(name) => format!("Preparing {name}").into(),
            ProcessingStage::Transcribing => "Transcribing".into(),
            ProcessingStage::IdentifyingSpeakers => "Identifying speakers".into(),
            ProcessingStage::Summarizing => "Summarizing".into(),
            ProcessingStage::Failed(_) => "Failed".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundKind {
    Meeting,
    Digest,
    Download,
    Reindex,
}

/// One row of the sidebar's "In progress" card.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundItem {
    pub id: SharedString,
    pub kind: BackgroundKind,
    pub title: SharedString,
    pub detail: SharedString,
    /// 0.0–1.0 when known.
    pub progress: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub message: SharedString,
    pub is_error: bool,
}

/// A question to put into Ask when navigating there.
#[derive(Clone, Debug, PartialEq)]
pub struct AskRequest {
    pub question: String,
    /// Limit sources to these meetings.
    pub meeting_ids: Vec<Uuid>,
    pub submit: bool,
}

/// A request to show a meeting, optionally at a moment of its transcript.
#[derive(Clone, Debug, PartialEq)]
pub struct MeetingFocus {
    pub meeting_id: Uuid,
    pub tab: MeetingTab,
    pub at: Option<f64>,
}

pub struct AppData {
    pub library: Library,
    pub settings: Settings,
    /// Visible meetings, newest first.
    pub meetings: Vec<Meeting>,
    pub meetings_loaded: bool,
    /// Bumped whenever library content changes, so pages can reload lazily.
    pub revision: u64,
    pub section: Section,
    pub settings_category: SettingsCategory,
    pub meeting_focus: Option<MeetingFocus>,
    pub ask_request: Option<AskRequest>,
    pub recording: RecordingState,
    pub processing: HashMap<Uuid, ProcessingStage>,
    pub background: Vec<BackgroundItem>,
    pub toast: Option<Toast>,
    pub day_memory_paused: bool,
    pub inference_label: SharedString,
    pub remote_hosts: Vec<SharedString>,
    /// The answer being written right now, if any. The engine appends text
    /// as it streams and saves the finished turn into the conversation file.
    pub ask_stream: Option<AskStream>,
    /// Model downloads by catalog id.
    pub downloads: HashMap<String, DownloadProgress>,
    /// Which models are present and which runtime the Think role uses.
    pub models: ModelStatus,
    /// Day whose digest is being written.
    pub digest_running: Option<chrono::NaiveDate>,
    /// Project ids whose "Where it stands" note is being written.
    pub project_notes_running: Vec<String>,
    /// Text of the last dictation shown in LokalBot (output mode "Show in LokalBot").
    pub last_dictation: Option<SharedString>,
}

/// Progress of an answer in Ask.
#[derive(Clone, Debug, PartialEq)]
pub struct AskStream {
    pub conversation_id: Uuid,
    pub question: String,
    /// Assistant text so far, with `[meeting:…]` citations inline.
    pub text: String,
    /// Steps shown as "worked: searched meetings · read “Design review”".
    pub activity: Vec<String>,
    /// "Preparing Qwen3.5 4B" while the model loads.
    pub status: Option<SharedString>,
    pub error: Option<SharedString>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DownloadProgress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<SharedString>,
}

impl DownloadProgress {
    pub fn fraction(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.done_bytes as f32 / self.total_bytes as f32
        }
    }
}

/// Readiness of each model role, refreshed by the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelStatus {
    /// Catalog ids that are fully downloaded.
    pub installed: Vec<String>,
    pub transcription_ready: bool,
    pub think_ready: bool,
    pub autocomplete_ready: bool,
    pub embeddings_ready: bool,
    /// "Vulkan" or "CPU" for the built-in llama.cpp runtime, once installed.
    pub runtime_backend: Option<SharedString>,
}

/// Sources and date range for a question in Ask.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AskScope {
    /// Empty means every meeting.
    pub meeting_ids: Vec<Uuid>,
    pub include_meetings: bool,
    pub include_activity: bool,
    pub include_screen: bool,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

pub struct AppHandle(pub Entity<AppData>);

impl Global for AppHandle {}

impl AppData {
    pub fn global(cx: &App) -> Entity<AppData> {
        cx.global::<AppHandle>().0.clone()
    }

    pub fn new(library: Library, settings: Settings) -> Self {
        let (inference_label, remote_hosts) = inference_presentation(&settings);
        Self {
            library,
            settings,
            meetings: Vec::new(),
            meetings_loaded: false,
            revision: 0,
            section: Section::Today,
            settings_category: SettingsCategory::General,
            meeting_focus: None,
            ask_request: None,
            recording: RecordingState::Idle,
            processing: HashMap::new(),
            background: Vec::new(),
            toast: None,
            day_memory_paused: false,
            inference_label,
            remote_hosts,
            ask_stream: None,
            downloads: HashMap::new(),
            models: ModelStatus::default(),
            digest_running: None,
            project_notes_running: Vec::new(),
            last_dictation: None,
        }
    }

    /// Asks the engine for work. Pages call this instead of emitting directly
    /// so every request goes through one place.
    pub fn request(&mut self, event: AppEvent, cx: &mut Context<Self>) {
        cx.emit(event);
    }

    // ------------------------------------------------------------ navigation

    pub fn navigate(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    pub fn open_meeting(
        &mut self,
        meeting_id: Uuid,
        tab: MeetingTab,
        at: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        self.meeting_focus = Some(MeetingFocus {
            meeting_id,
            tab,
            at,
        });
        self.section = Section::Meetings;
        cx.notify();
    }

    pub fn open_settings(&mut self, category: SettingsCategory, cx: &mut Context<Self>) {
        self.settings_category = category;
        self.section = Section::Settings;
        cx.notify();
    }

    pub fn ask(&mut self, request: AskRequest, cx: &mut Context<Self>) {
        self.ask_request = Some(request);
        self.section = Section::Ask;
        cx.notify();
    }

    // --------------------------------------------------------------- library

    /// Reloads the meeting list on a background thread.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move { library.list_meetings() })
                .await;
            let _ = this.update(cx, |this, cx| {
                match loaded {
                    Ok(meetings) => this.meetings = meetings,
                    Err(err) => this.show_error(format!("Could not read the library: {err:#}"), cx),
                }
                this.meetings_loaded = true;
                this.revision += 1;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn meeting(&self, id: Uuid) -> Option<&Meeting> {
        self.meetings.iter().find(|m| m.id == id)
    }

    /// A page wrote meeting files itself (corrections, speaker names): pages
    /// that show them reload on the next `revision`.
    pub fn mark_library_changed(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        cx.notify();
    }

    pub fn set_action_status(
        &mut self,
        meeting_id: Uuid,
        action_id: String,
        status: ActionStatus,
        cx: &mut Context<Self>,
    ) {
        let Some(meeting) = self.meeting(meeting_id).cloned() else {
            return;
        };
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { library.set_action_status(&meeting, &action_id, status) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.show_error(format!("Could not update the action: {err:#}"), cx);
                }
                this.revision += 1;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_meeting(&mut self, meeting_id: Uuid, cx: &mut Context<Self>) {
        let Some(meeting) = self.meeting(meeting_id).cloned() else {
            return;
        };
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { library.delete_meeting(&meeting) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.show_error(format!("Could not delete the meeting: {err:#}"), cx);
                }
                if this
                    .meeting_focus
                    .as_ref()
                    .is_some_and(|f| f.meeting_id == meeting_id)
                {
                    this.meeting_focus = None;
                }
                this.reload(cx);
            });
        })
        .detach();
    }

    // -------------------------------------------------------------- settings

    /// Applies a settings change and saves it in the background.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let before = self.settings.clone();
        change(&mut self.settings);
        if self.settings == before {
            return;
        }
        let (label, hosts) = inference_presentation(&self.settings);
        self.inference_label = label;
        self.remote_hosts = hosts;
        let path = self.library.paths().settings_file();
        let settings = self.settings.clone();
        cx.background_spawn(async move {
            if let Err(err) = settings.save(&path) {
                log::error!("saving settings failed: {err}");
            }
        })
        .detach();
        if before.app_theme != self.settings.app_theme
            || before.text_size != self.settings.text_size
        {
            cx.emit(AppEvent::AppearanceChanged);
        }
        cx.emit(AppEvent::SettingsChanged);
        cx.notify();
    }

    // ------------------------------------------------------------- recording

    pub fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording.is_active() {
            self.stop_recording(cx);
        } else {
            self.start_recording(cx);
        }
    }

    pub fn start_recording(&mut self, cx: &mut Context<Self>) {
        cx.emit(AppEvent::StartRecording);
    }

    pub fn stop_recording(&mut self, cx: &mut Context<Self>) {
        cx.emit(AppEvent::StopRecording);
    }

    // ---------------------------------------------------------------- toasts

    pub fn show_message(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(Toast {
            message: message.into(),
            is_error: false,
        });
        cx.notify();
    }

    pub fn show_error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(Toast {
            message: message.into(),
            is_error: true,
        });
        cx.notify();
    }

    pub fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.toast = None;
        cx.notify();
    }
}

/// Requests that the engine bridge (in `main.rs`) handles. Results come back
/// as changes to `AppData` fields (`processing`, `ask_stream`, `downloads`,
/// `digest_running`, …) followed by `revision` bumps when files changed.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    StartRecording,
    StopRecording,
    SettingsChanged,
    AppearanceChanged,
    /// Transcribe and/or summarize a meeting again (Retry, "Transcribe & Summarize", "Summarize again").
    ProcessMeeting {
        meeting_id: Uuid,
        transcribe: bool,
        summarize: bool,
    },
    /// Write or rewrite the day digest.
    RegenerateDigest(chrono::NaiveDate),
    /// Ask a question in a conversation (a new id starts a new conversation).
    AskQuestion {
        conversation_id: Uuid,
        question: String,
        scope: AskScope,
    },
    StopAnswer,
    /// Download a model by catalog id (Think, Autocomplete, embeddings or speech).
    DownloadModel(String),
    CancelDownload(String),
    DeleteModel(String),
    /// Download every model the current settings need.
    PrepareModels,
    SetDayMemoryPaused(bool),
    /// Write the "Where it stands" note for a project id.
    WriteProjectNote(String),
    OpenQuickRecall,
    OpenOnboarding,
    /// Install `lokalbot-cli` for coding agents (PATH + skill folders).
    InstallCli,
    /// Start or stop dictation from the UI ("Try here").
    ToggleDictation,
    /// Start the live transcript preview of the current recording, from the
    /// latest 12 seconds or from the beginning.
    LivePreview {
        from_beginning: bool,
    },
}

impl gpui_kit::EventEmitter<AppEvent> for AppData {}

/// The sidebar storage card's AI line and remote hosts.
pub fn inference_presentation(settings: &Settings) -> (SharedString, Vec<SharedString>) {
    use lokalbot_core::settings::SummarizerBackend;
    let url = match settings.summarizer_backend {
        SummarizerBackend::BuiltIn => None,
        SummarizerBackend::Ollama => Some(settings.ollama_base_url.as_str()),
        SummarizerBackend::OpenAICompatible => Some(settings.openai_base_url.as_str()),
    };
    match url {
        Some(url) if !lokalbot_core::access::is_loopback_url(url) => {
            let host = lokalbot_core::access::origin_of(url)
                .map(|o| o.split_once("://").map(|(_, h)| h.to_string()).unwrap_or(o))
                .unwrap_or_else(|| url.to_string());
            ("AI: local + remote".into(), vec![host.into()])
        }
        _ => ("AI: on this PC".into(), Vec::new()),
    }
}

/// Friendly day label for list groups: "Today", "Yesterday", "Wednesday, October 7".
pub fn day_label(at: DateTime<Utc>) -> String {
    let day = lokalbot_core::time::local_day(at);
    let today = chrono::Local::now().date_naive();
    if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    } else {
        day.format("%A, %B %-d").to_string()
    }
}
