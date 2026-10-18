//! Requests the app sends to the engine and the updates it gets back.

use chrono::{DateTime, NaiveDate, Utc};
use lokalbot_core::{Meeting, Settings};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum Command {
    StartRecording {
        title: Option<String>,
        app_name: Option<String>,
    },
    StopRecording,
    /// Retry or redo processing. User-initiated jobs may download missing models.
    ProcessMeeting {
        meeting_id: Uuid,
        transcribe: bool,
        summarize: bool,
    },
    RegenerateDigest(NaiveDate),
    Ask {
        conversation_id: Uuid,
        question: String,
        scope: AskScope,
    },
    StopAnswer,
    DownloadModel(String),
    CancelDownload(String),
    DeleteModel(String),
    /// Downloads every model the current settings need.
    PrepareModels,
    SetDayMemoryPaused(bool),
    WriteProjectNote(String),
    UpdateSettings(Box<Settings>),
    Dictation(DictationCommand),
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictationCommand {
    Start,
    Stop,
    Toggle,
    Cancel,
}

/// Sources and date range for a question in Ask.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AskScope {
    pub meeting_ids: Vec<Uuid>,
    pub include_meetings: bool,
    pub include_activity: bool,
    pub include_screen: bool,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EngineEvent {
    Recording(RecordingUpdate),
    /// `None` when the meeting is no longer being processed.
    Processing {
        meeting_id: Uuid,
        stage: Option<Stage>,
    },
    /// Files in the library changed; reload lists.
    LibraryChanged,
    Ask(AskUpdate),
    Download(DownloadUpdate),
    Models(ModelStatus),
    Digest {
        running: Option<NaiveDate>,
    },
    ProjectNote {
        project_id: String,
        running: bool,
    },
    Background(Vec<BackgroundItem>),
    Message {
        text: String,
        is_error: bool,
    },
    /// A meeting app started a call while recording mode is "Ask via notification".
    MeetingDetected {
        app_name: String,
        title: String,
    },
    Dictation(DictationUpdate),
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum RecordingUpdate {
    Started {
        meeting: Meeting,
        has_system_audio: bool,
    },
    Levels {
        mic: f32,
        system: f32,
        mic_status: String,
        system_status: String,
        elapsed_seconds: f64,
    },
    Stopped {
        meeting_id: Uuid,
        warnings: Vec<String>,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Queued,
    WaitingForModels,
    PreparingModel(String),
    Transcribing,
    IdentifyingSpeakers,
    Summarizing,
    Failed(String),
}

#[derive(Clone, Debug)]
pub enum AskUpdate {
    Progress {
        conversation_id: Uuid,
        question: String,
        text: String,
        activity: Vec<String>,
        status: Option<String>,
    },
    Finished {
        conversation_id: Uuid,
    },
    Failed {
        conversation_id: Uuid,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub struct DownloadUpdate {
    pub id: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub finished: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelStatus {
    pub installed: Vec<String>,
    pub transcription_ready: bool,
    pub think_ready: bool,
    pub autocomplete_ready: bool,
    pub embeddings_ready: bool,
    pub runtime_backend: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundKind {
    Meeting,
    Digest,
    Download,
    Reindex,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundItem {
    pub id: String,
    pub kind: BackgroundKind,
    pub title: String,
    pub detail: String,
    pub progress: Option<f32>,
}

#[derive(Clone, Debug)]
pub enum DictationUpdate {
    Starting,
    Listening {
        level: f32,
        elapsed_seconds: f64,
        preview: String,
    },
    Transcribing,
    Composing,
    /// `inserted` is false when the text went to the clipboard instead.
    Finished {
        text: String,
        inserted: bool,
        notice: Option<String>,
    },
    Failed {
        message: String,
    },
    Cancelled,
}
