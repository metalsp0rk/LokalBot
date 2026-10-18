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
    /// Writes a meeting's follow-up with the local Think model. Nothing is
    /// sent or saved; the draft comes back for review.
    WriteFollowUp(Uuid),
    UpdateSettings(Box<Settings>),
    Dictation(DictationCommand),
    Autocomplete(AutocompleteCommand),
    /// "Capture Screen Now": capture the window in front, or the next one
    /// to come to the front within a few seconds when LokalBot itself is.
    CaptureScreenNow,
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
    FollowUp(FollowUpUpdate),
    Autocomplete(AutocompleteUpdate),
    Screen(ScreenStatus),
}

/// Screen context capture as Settings shows it (`ScreenshotService`'s
/// published state and `MemoryHealthSection`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenStatus {
    /// Text and images are being captured (screen context on, tracking on,
    /// day memory not paused).
    pub active: bool,
    /// Why this desktop cannot capture window pixels (and so cannot OCR), if so.
    pub capture_limitation: Option<String>,
    /// Why visible text cannot be read through accessibility, if so.
    pub text_limitation: Option<String>,
    /// Why images cannot be kept (no system credential store), if so.
    pub images_unavailable: Option<String>,
    pub ocr_installed: bool,
    pub last_capture: Option<DateTime<Utc>>,
    /// `accessibility`, `ocr`, `hybrid` (with `_redacted`) of the last capture.
    pub last_text_source: Option<String>,
    pub last_error: Option<String>,
    /// "Capture Screen Now" is waiting for another window to come to the front.
    pub waiting_for_window: bool,
}

/// The text field the user is typing in, as the desktop integration reads
/// it (`CotypingSurfaceContext`). Sent on edits and caret moves while
/// autocomplete is on; never for secure fields or excluded apps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldContext {
    /// Changes when focus moves to another field; updates for an old id are stale.
    pub field_id: u64,
    pub app_name: String,
    /// Executable or desktop id: "firefox", "code.exe".
    pub app_id: String,
    pub window_title: String,
    /// Text before the caret (bounded by the reader).
    pub before: String,
    /// Text after the caret in the same paragraph.
    pub after: String,
    /// Text visible above the field, such as the conversation being answered.
    pub visible_text: Option<String>,
    /// The page address when the app is a browser and it could be read.
    pub url: Option<String>,
    /// Inputs where Enter sends (search boxes, chat inputs).
    pub single_line: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutocompleteCommand {
    /// The field changed: typing, a caret move, or new focus.
    Context(FieldContext),
    /// The user accepted `text`, the start of the shown suggestion.
    Accepted { field_id: u64, text: String },
    /// Escape, focus loss or a secure field: drop any suggestion.
    Dismiss,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutocompleteUpdate {
    /// Ghost text to show after the caret of `field_id`.
    Suggestion { field_id: u64, text: String },
    /// Hide any ghost text.
    Clear,
}

/// Progress of one follow-up written with Think (`FollowUpDraftSheet`).
#[derive(Clone, Debug)]
pub enum FollowUpUpdate {
    Writing {
        meeting_id: Uuid,
    },
    Written {
        meeting_id: Uuid,
        subject: String,
        body: String,
        /// "Qwen3.5 4B" for "Written on this PC by …".
        model: String,
    },
    Failed {
        meeting_id: Uuid,
        message: String,
    },
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
    /// The screen text recognition (OCR) models are downloaded.
    pub screen_text_ready: bool,
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
