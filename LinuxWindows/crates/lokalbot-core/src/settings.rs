//! `settings.json` in the library root.
//!
//! Keys follow the macOS `AppSettings` blob where the setting exists on both
//! platforms. Decoding is tolerant: a missing or unreadable key takes its
//! default, so older or newer files always load.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::activity::ScreenContextMode;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // General
    pub launch_at_login: bool,
    /// Closing the window keeps LokalBot running in the system tray.
    #[serde(rename = "menuBarOnly")]
    pub run_in_tray: bool,
    pub app_theme: AppTheme,
    pub text_size: TextSize,
    pub quick_recall_enabled: bool,
    pub quick_recall_shortcut: String,
    pub check_for_updates: bool,
    pub onboarding_completed: bool,
    pub getting_started_dismissed: bool,

    // Meetings
    pub auto_record_mode: AutoRecordMode,
    pub stop_debounce_seconds: f64,
    pub record_system_audio: bool,
    /// Empty means the system default input.
    pub microphone_device: String,
    pub auto_transcribe: bool,
    pub auto_summarize: bool,
    pub note_template: NoteTemplate,
    /// `match`, a language code, or `custom:<name>`.
    pub summary_language: String,
    pub multi_speaker_diarization: bool,
    pub microphone_is_user: bool,
    pub suggest_action_completion: bool,
    pub generation_budget_preset: GenerationBudget,
    pub meeting_note_export_enabled: bool,
    pub meeting_note_export_folder: String,
    pub meeting_note_export_format: ExportFormat,
    pub meeting_note_export_includes_transcript: bool,

    // Day memory
    pub tracking_enabled: bool,
    pub screen_context_capture_mode: ScreenContextMode,
    pub screenshot_interval_minutes: f64,
    pub day_digest_auto_enabled: bool,
    pub day_digest_hour: u32,
    pub day_digest_custom_prompt: String,
    pub coding_agent_evidence_enabled: bool,
    pub coding_agent_reads_claude_code: bool,
    pub coding_agent_reads_codex: bool,

    // Privacy
    pub excluded_apps: String,
    pub excluded_screen_domains: String,
    pub retention_days: u32,
    #[serde(rename = "keepOCRTextForever")]
    pub keep_ocr_text_forever: bool,
    pub approved_remote_inference_origins: Vec<String>,
    pub approved_remote_transcription_origins: Vec<String>,
    pub approved_remote_autocomplete_origins: Vec<String>,

    // Models
    pub transcription_model: String,
    pub transcription_language: String,
    pub transcription_prompt: String,
    pub auto_transcription_vocabulary: bool,
    pub summarizer_backend: SummarizerBackend,
    #[serde(rename = "builtInModelID")]
    pub built_in_model_id: String,
    #[serde(rename = "ollamaBaseURL")]
    pub ollama_base_url: String,
    pub ollama_model: String,
    #[serde(rename = "openAIBaseURL")]
    pub openai_base_url: String,
    #[serde(rename = "openAIModel")]
    pub openai_model: String,
    pub think_reasoning_level: ReasoningLevel,
    pub semantic_search_enabled: bool,
    /// llama.cpp build for the built-in runtime: `auto`, `vulkan` or `cpu`.
    pub llama_backend: String,

    // Dictation
    pub dictation_enabled: bool,
    pub dictation_intent: DictationIntent,
    pub dictation_trigger_mode: DictationTriggerMode,
    pub dictation_shortcut: String,
    pub dictation_output_mode: DictationOutputMode,
    pub dictation_show_overlay: bool,
    pub dictation_plays_start_sound: bool,
    #[serde(rename = "dictationMicrophoneID")]
    pub dictation_microphone_id: String,
    pub dictation_retain_audio: bool,

    // Writing (autocomplete)
    pub cotyping_enabled: bool,
    pub cotyping_user_name: String,
    pub cotyping_style_note: String,
    pub cotyping_max_words: u32,
    pub cotyping_debounce_ms: u32,
    pub cotyping_multi_line: bool,
    pub cotyping_accept_granularity: AcceptGranularity,
    #[serde(rename = "cotypingBuiltInModelID")]
    pub cotyping_built_in_model_id: String,
    pub cotyping_excluded_apps: String,
    /// Sites where autocomplete never runs: a host (subdomains included) or a
    /// full URL, comma-separated (`cotypingExcludedDomains`).
    pub cotyping_excluded_domains: String,
    /// Use the text shown just above the field, such as the conversation
    /// being answered (`cotypingUseVisibleContext`, on as on macOS).
    pub cotyping_use_visible_context: bool,
    /// Let suggestions use relevant facts from saved meetings. A grant of
    /// its own, off by default, as on macOS.
    pub cotyping_use_meeting_memory: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            run_in_tray: true,
            app_theme: AppTheme::System,
            text_size: TextSize::Standard,
            quick_recall_enabled: false,
            quick_recall_shortcut: "Ctrl+Shift+Space".into(),
            check_for_updates: true,
            onboarding_completed: false,
            getting_started_dismissed: false,

            auto_record_mode: AutoRecordMode::Ask,
            stop_debounce_seconds: 15.0,
            record_system_audio: true,
            microphone_device: String::new(),
            auto_transcribe: true,
            auto_summarize: true,
            note_template: NoteTemplate::Meeting,
            summary_language: "match".into(),
            multi_speaker_diarization: true,
            microphone_is_user: true,
            suggest_action_completion: true,
            generation_budget_preset: GenerationBudget::Standard,
            meeting_note_export_enabled: false,
            meeting_note_export_folder: String::new(),
            meeting_note_export_format: ExportFormat::Markdown,
            meeting_note_export_includes_transcript: true,

            tracking_enabled: true,
            screen_context_capture_mode: ScreenContextMode::ActivityOnly,
            screenshot_interval_minutes: 3.0,
            day_digest_auto_enabled: true,
            day_digest_hour: 18,
            day_digest_custom_prompt: String::new(),
            coding_agent_evidence_enabled: true,
            coding_agent_reads_claude_code: true,
            coding_agent_reads_codex: true,

            excluded_apps: "1Password, Bitwarden, KeePassXC, KeePass".into(),
            excluded_screen_domains: String::new(),
            retention_days: 14,
            keep_ocr_text_forever: false,
            approved_remote_inference_origins: Vec::new(),
            approved_remote_transcription_origins: Vec::new(),
            approved_remote_autocomplete_origins: Vec::new(),

            transcription_model: "qwen3-asr-0.6b".into(),
            transcription_language: "auto".into(),
            transcription_prompt: String::new(),
            auto_transcription_vocabulary: true,
            summarizer_backend: SummarizerBackend::BuiltIn,
            built_in_model_id: "qwen3.5-4b".into(),
            ollama_base_url: "http://localhost:11434".into(),
            ollama_model: String::new(),
            openai_base_url: "http://localhost:1234/v1".into(),
            openai_model: String::new(),
            think_reasoning_level: ReasoningLevel::Automatic,
            semantic_search_enabled: true,
            llama_backend: "auto".into(),

            dictation_enabled: false,
            dictation_intent: DictationIntent::Transcribe,
            dictation_trigger_mode: DictationTriggerMode::PushToTalk,
            dictation_shortcut: "Alt+Space".into(),
            dictation_output_mode: DictationOutputMode::Paste,
            dictation_show_overlay: true,
            dictation_plays_start_sound: true,
            dictation_microphone_id: String::new(),
            dictation_retain_audio: false,

            cotyping_enabled: false,
            cotyping_user_name: String::new(),
            cotyping_style_note: String::new(),
            cotyping_max_words: 4,
            cotyping_debounce_ms: 160,
            cotyping_multi_line: false,
            cotyping_accept_granularity: AcceptGranularity::Word,
            cotyping_built_in_model_id: "lfm2.5-1.2b-instruct".into(),
            cotyping_excluded_apps: "1Password, Bitwarden, KeePassXC, KeePass".into(),
            cotyping_excluded_domains: String::new(),
            cotyping_use_visible_context: true,
            cotyping_use_meeting_memory: false,
        }
    }
}

impl Settings {
    /// Loads settings, falling back to defaults for anything missing or unreadable.
    /// A field with the wrong type falls back on its own; the rest still load.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => settings,
            Err(_) => Self::load_field_by_field(&bytes),
        }
    }

    fn load_field_by_field(bytes: &[u8]) -> Self {
        let Ok(serde_json::Value::Object(stored)) =
            serde_json::from_slice::<serde_json::Value>(bytes)
        else {
            return Self::default();
        };
        let serde_json::Value::Object(mut merged) =
            serde_json::to_value(Self::default()).expect("settings serialize")
        else {
            return Self::default();
        };
        for (key, value) in stored {
            if !merged.contains_key(&key) {
                continue;
            }
            let mut candidate = merged.clone();
            candidate.insert(key.clone(), value.clone());
            if serde_json::from_value::<Settings>(serde_json::Value::Object(candidate)).is_ok() {
                merged.insert(key, value);
            } else {
                log::warn!("settings.json: ignoring unreadable value for {key}");
            }
        }
        serde_json::from_value(serde_json::Value::Object(merged)).unwrap_or_default()
    }

    /// Writes atomically (temp file + rename), pretty-printed.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::library::write_atomic(path, &json)
    }

    /// Whether the selected Think backend runs on this machine without approval.
    pub fn think_is_local(&self) -> bool {
        match self.summarizer_backend {
            SummarizerBackend::BuiltIn => true,
            SummarizerBackend::Ollama => crate::access::is_loopback_url(&self.ollama_base_url),
            SummarizerBackend::OpenAICompatible => {
                crate::access::is_loopback_url(&self.openai_base_url)
            }
        }
    }

    pub fn excluded_app_list(&self) -> Vec<String> {
        split_list(&self.excluded_apps)
    }

    pub fn excluded_domain_list(&self) -> Vec<String> {
        split_list(&self.excluded_screen_domains)
    }

    pub fn cotyping_excluded_app_list(&self) -> Vec<String> {
        split_list(&self.cotyping_excluded_apps)
    }

    pub fn cotyping_excluded_domain_list(&self) -> Vec<String> {
        split_list(&self.cotyping_excluded_domains)
    }
}

pub fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    Small,
    #[default]
    Standard,
    Large,
    Larger,
    Largest,
}

impl TextSize {
    /// Multiplier on the base type scale (Small 1.0 … Largest 1.45).
    pub fn scale(self) -> f32 {
        match self {
            TextSize::Small => 0.93,
            TextSize::Standard => 1.0,
            TextSize::Large => 1.1,
            TextSize::Larger => 1.2,
            TextSize::Largest => 1.35,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoRecordMode {
    #[serde(rename = "Record automatically")]
    Automatic,
    #[default]
    #[serde(rename = "Ask via notification")]
    Ask,
    #[serde(rename = "Never auto-record")]
    Manual,
}

impl AutoRecordMode {
    pub const ALL: [AutoRecordMode; 3] = [Self::Automatic, Self::Ask, Self::Manual];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Record automatically",
            Self::Ask => "Ask via notification",
            Self::Manual => "Never auto-record",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteTemplate {
    #[default]
    Meeting,
    Lecture,
    StudyGuide,
    Podcast,
    Freeform,
}

impl NoteTemplate {
    pub const ALL: [NoteTemplate; 5] = [
        Self::Meeting,
        Self::Lecture,
        Self::StudyGuide,
        Self::Podcast,
        Self::Freeform,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Meeting => "Meeting",
            Self::Lecture => "Lecture",
            Self::StudyGuide => "Study guide",
            Self::Podcast => "Podcast / interview",
            Self::Freeform => "Free-form",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GenerationBudget {
    Conservative,
    #[default]
    Standard,
    Generous,
    Unlimited,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    #[default]
    Markdown,
    Obsidian,
    Logseq,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummarizerBackend {
    #[default]
    #[serde(rename = "Built-in (no setup)")]
    BuiltIn,
    #[serde(rename = "Ollama")]
    Ollama,
    #[serde(rename = "OpenAI-compatible server")]
    OpenAICompatible,
}

impl SummarizerBackend {
    pub const ALL: [SummarizerBackend; 3] = [Self::BuiltIn, Self::Ollama, Self::OpenAICompatible];

    pub fn label(self) -> &'static str {
        match self {
            Self::BuiltIn => "Built-in (no setup)",
            Self::Ollama => "Ollama",
            Self::OpenAICompatible => "OpenAI-compatible server",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    #[default]
    Automatic,
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningLevel {
    /// Thinking budget in tokens; `None` lets the task decide.
    pub fn budget_tokens(self) -> Option<u32> {
        match self {
            Self::Automatic => None,
            Self::Off => Some(0),
            Self::Minimal => Some(256),
            Self::Low => Some(512),
            Self::Medium => Some(4096),
            Self::High => Some(8192),
            Self::Xhigh => Some(16_384),
            Self::Max => Some(24_576),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationIntent {
    #[default]
    Transcribe,
    Compose,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationTriggerMode {
    #[default]
    #[serde(rename = "Push to talk")]
    PushToTalk,
    Toggle,
    #[serde(rename = "Tap or hold")]
    TapOrHold,
}

impl DictationTriggerMode {
    pub const ALL: [DictationTriggerMode; 3] = [Self::PushToTalk, Self::Toggle, Self::TapOrHold];

    pub fn label(self) -> &'static str {
        match self {
            Self::PushToTalk => "Push to talk",
            Self::Toggle => "Toggle",
            Self::TapOrHold => "Tap or hold",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationOutputMode {
    #[default]
    #[serde(rename = "Paste into focused app")]
    Paste,
    #[serde(rename = "Copy to clipboard")]
    Clipboard,
    #[serde(rename = "Show in LokalBot")]
    ShowInApp,
}

impl DictationOutputMode {
    pub const ALL: [DictationOutputMode; 3] = [Self::Paste, Self::Clipboard, Self::ShowInApp];

    pub fn label(self) -> &'static str {
        match self {
            Self::Paste => "Paste into focused app",
            Self::Clipboard => "Copy to clipboard",
            Self::ShowInApp => "Show in LokalBot",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AcceptGranularity {
    #[default]
    Word,
    Phrase,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Settings::default());
        assert!(json.contains("\"autoRecordMode\":\"Ask via notification\""));
        assert!(json.contains("\"builtInModelID\":\"qwen3.5-4b\""));
    }

    #[test]
    fn a_bad_field_does_not_reset_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"retentionDays": "fourteen", "trackingEnabled": false, "dayDigestHour": 9}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.retention_days, 14);
        assert!(!settings.tracking_enabled);
        assert_eq!(settings.day_digest_hour, 9);
    }
}
