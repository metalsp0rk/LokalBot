//! Searchable settings (`SettingDescriptor.swift`): title, category, extra
//! words to match, and the current value shown in results.

use lokalbot_core::Settings;
use lokalbot_core::settings::{AppTheme, TextSize};

use crate::state::SettingsCategory;

pub struct Entry {
    pub id: &'static str,
    pub title: &'static str,
    pub category: SettingsCategory,
    pub aliases: &'static str,
    pub value: fn(&Settings) -> String,
}

fn on_off(value: bool) -> String {
    if value { "On".into() } else { "Off".into() }
}

fn text(value: &str) -> String {
    if value.trim().is_empty() {
        "Not set".into()
    } else {
        value.chars().take(70).collect()
    }
}

fn details(_: &Settings) -> String {
    "Open details".into()
}

pub fn theme_label(theme: AppTheme) -> &'static str {
    match theme {
        AppTheme::System => "Match System",
        AppTheme::Light => "Light",
        AppTheme::Dark => "Dark",
    }
}

pub fn text_size_label(size: TextSize) -> &'static str {
    match size {
        TextSize::Small => "Small",
        TextSize::Standard => "Default",
        TextSize::Large => "Large",
        TextSize::Larger => "Larger",
        TextSize::Largest => "Largest",
    }
}

use SettingsCategory::*;

pub const ENTRIES: &[Entry] = &[
    Entry {
        id: "settings.launchAtLogin",
        title: "Launch LokalBot at login",
        category: General,
        aliases: "startup start automatically boot autostart",
        value: |s| on_off(s.launch_at_login),
    },
    Entry {
        id: "settings.runInTray",
        title: "Keep LokalBot running in the system tray",
        category: General,
        aliases: "tray background close window menu bar",
        value: |s| on_off(s.run_in_tray),
    },
    Entry {
        id: "settings.appTheme",
        title: "Theme",
        category: General,
        aliases: "appearance dark light mode",
        value: |s| theme_label(s.app_theme).into(),
    },
    Entry {
        id: "settings.textSize",
        title: "Text size",
        category: General,
        aliases: "font scale bigger smaller zoom",
        value: |s| text_size_label(s.text_size).into(),
    },
    Entry {
        id: "settings.quickRecallEnabled",
        title: "Enable the system-wide Ask shortcut",
        category: General,
        aliases: "quick recall hotkey global search",
        value: |s| on_off(s.quick_recall_enabled),
    },
    Entry {
        id: "settings.quickRecallShortcut",
        title: "Ask shortcut",
        category: General,
        aliases: "quick recall hotkey keyboard",
        value: |s| s.quick_recall_shortcut.clone(),
    },
    Entry {
        id: "settings.currentVersion",
        title: "Current version",
        category: General,
        aliases: "version about release update updates",
        value: |_| env!("CARGO_PKG_VERSION").into(),
    },
    Entry {
        id: "settings.autoRecordMode",
        title: "When a meeting is detected",
        category: Meetings,
        aliases: "auto record detect notification consent",
        value: |s| s.auto_record_mode.label().into(),
    },
    Entry {
        id: "settings.stopDebounceSeconds",
        title: "Wait before stopping",
        category: Meetings,
        aliases: "debounce stop delay",
        value: |s| format!("{} s after audio stops", s.stop_debounce_seconds.round()),
    },
    Entry {
        id: "settings.recordSystemAudio",
        title: "Record system audio",
        category: Meetings,
        aliases: "speakers other side loopback",
        value: |s| on_off(s.record_system_audio),
    },
    Entry {
        id: "settings.microphoneDevice",
        title: "Microphone",
        category: Meetings,
        aliases: "input device mic",
        value: |s| {
            if s.microphone_device.is_empty() {
                "System default".into()
            } else {
                s.microphone_device.clone()
            }
        },
    },
    Entry {
        id: "settings.autoTranscribe",
        title: "Transcribe automatically after each meeting",
        category: Meetings,
        aliases: "transcription processing",
        value: |s| on_off(s.auto_transcribe),
    },
    Entry {
        id: "settings.autoSummarize",
        title: "Summarize automatically after transcription",
        category: Meetings,
        aliases: "summary notes processing",
        value: |s| on_off(s.auto_summarize),
    },
    Entry {
        id: "settings.noteTemplate",
        title: "Notes template",
        category: Meetings,
        aliases: "summary lecture study podcast",
        value: |s| s.note_template.label().into(),
    },
    Entry {
        id: "settings.summaryLanguage",
        title: "Notes language",
        category: Meetings,
        aliases: "summary translate language",
        value: |s| super::meetings::summary_language_label(&s.summary_language),
    },
    Entry {
        id: "settings.multiSpeakerDiarization",
        title: "Separate voices by speaker",
        category: Meetings,
        aliases: "diarization speakers",
        value: |s| on_off(s.multi_speaker_diarization),
    },
    Entry {
        id: "settings.microphoneIsUser",
        title: "My microphone is me",
        category: Meetings,
        aliases: "speaker you owner",
        value: |s| on_off(s.microphone_is_user),
    },
    Entry {
        id: "settings.trackingEnabled",
        title: "Track app & window activity",
        category: DayMemory,
        aliases: "activity timeline tracking",
        value: |s| on_off(s.tracking_enabled),
    },
    Entry {
        id: "settings.screenContextCaptureMode",
        title: "Screen context",
        category: DayMemory,
        aliases: "screenshots ocr text capture visual images",
        value: |s| s.screen_context_capture_mode.label().into(),
    },
    Entry {
        id: "settings.screenTextRecognition",
        title: "Screen text recognition",
        category: DayMemory,
        aliases: "ocr model download screenshots text",
        value: details,
    },
    Entry {
        id: "settings.screenshotIntervalMinutes",
        title: "Idle fallback",
        category: DayMemory,
        aliases: "screen capture interval minutes screenshots",
        value: |s| {
            format!(
                "at least every {} min",
                s.screenshot_interval_minutes.round()
            )
        },
    },
    Entry {
        id: "settings.dayDigestAutoEnabled",
        title: "Generate the day digest automatically",
        category: DayMemory,
        aliases: "digest journal schedule",
        value: |s| on_off(s.day_digest_auto_enabled),
    },
    Entry {
        id: "settings.dayDigestCustomPrompt",
        title: "Digest instructions (optional)",
        category: DayMemory,
        aliases: "digest prompt instructions",
        value: |s| text(&s.day_digest_custom_prompt),
    },
    Entry {
        id: "settings.autocompleteAvailability",
        title: "Autocomplete",
        category: Writing,
        aliases: "cotyping suggestions autocomplete typing macos",
        value: |_| "macOS only for now".into(),
    },
    Entry {
        id: "settings.cotypingUserName",
        title: "Your name",
        category: Writing,
        aliases: "writing profile name compose",
        value: |s| text(&s.cotyping_user_name),
    },
    Entry {
        id: "settings.cotypingStyleNote",
        title: "Writing style",
        category: Writing,
        aliases: "writing profile tone compose",
        value: |s| text(&s.cotyping_style_note),
    },
    Entry {
        id: "settings.dictationEnabled",
        title: "Enable dictation shortcut",
        category: Dictation,
        aliases: "dictation voice typing speech",
        value: |s| on_off(s.dictation_enabled),
    },
    Entry {
        id: "settings.dictationShortcut",
        title: "Shortcut",
        category: Dictation,
        aliases: "dictation hotkey keyboard",
        value: |s| s.dictation_shortcut.clone(),
    },
    Entry {
        id: "settings.dictationTriggerMode",
        title: "Trigger",
        category: Dictation,
        aliases: "push to talk toggle hold",
        value: |s| s.dictation_trigger_mode.label().into(),
    },
    Entry {
        id: "settings.dictationIntent",
        title: "Intent",
        category: Dictation,
        aliases: "dictation transcribe compose rewrite draft reply",
        value: |s| match s.dictation_intent {
            lokalbot_core::settings::DictationIntent::Transcribe => "Transcribe".into(),
            lokalbot_core::settings::DictationIntent::Compose => "Compose".into(),
        },
    },
    Entry {
        id: "settings.dictationOutputMode",
        title: "After a shortcut recording",
        category: Dictation,
        aliases: "paste clipboard output",
        value: |s| s.dictation_output_mode.label().into(),
    },
    Entry {
        id: "settings.dictationShowOverlay",
        title: "Show floating dictation status",
        category: Dictation,
        aliases: "overlay hud",
        value: |s| on_off(s.dictation_show_overlay),
    },
    Entry {
        id: "settings.models",
        title: "Models",
        category: Models,
        aliases: "transcribe think model llm whisper qwen download",
        value: details,
    },
    Entry {
        id: "settings.summarizerBackend",
        title: "Connections",
        category: Models,
        aliases: "ollama openai server remote api key url",
        value: |s| s.summarizer_backend.label().into(),
    },
    Entry {
        id: "settings.semanticSearchEnabled",
        title: "Search by meaning",
        category: Models,
        aliases: "semantic embeddings",
        value: |s| on_off(s.semantic_search_enabled),
    },
    Entry {
        id: "settings.modelStorage",
        title: "Model Storage",
        category: Models,
        aliases: "disk space remove delete unused downloaded models folder",
        value: details,
    },
    Entry {
        id: "settings.libraryLocation",
        title: "Library location",
        category: Privacy,
        aliases: "storage folder data path",
        value: details,
    },
    Entry {
        id: "settings.retentionDays",
        title: "Keep screen context",
        category: Privacy,
        aliases: "retention delete days cleanup window titles history activity screenshots",
        value: |s| format!("{} days", s.retention_days),
    },
    Entry {
        id: "settings.keepOCRTextForever",
        title: "Keep screen text forever",
        category: Privacy,
        aliases: "retention ocr text",
        value: |s| on_off(s.keep_ocr_text_forever),
    },
    Entry {
        id: "settings.agentAccess",
        title: "Allow external agents to read your meeting library",
        category: Privacy,
        aliases: "mcp claude cursor cli agents",
        value: details,
    },
    Entry {
        id: "settings.screenMemoryAccess",
        title: "Allow external agents to read screen memory",
        category: Privacy,
        aliases: "mcp agents screen activity timeline",
        value: details,
    },
    Entry {
        id: "settings.excludedApps",
        title: "Never capture these apps",
        category: Privacy,
        aliases: "exclusions private password activity",
        value: |s| format!("{} selected", s.excluded_app_list().len()),
    },
    Entry {
        id: "settings.excludedScreenDomains",
        title: "Never capture these sites",
        category: Privacy,
        aliases: "exclusions domains websites private",
        value: |s| format!("{} selected", s.excluded_domain_list().len()),
    },
    Entry {
        id: "settings.setupAgain",
        title: "Set Up LokalBot Again",
        category: Advanced,
        aliases: "setup onboarding welcome assistant wizard first run",
        value: details,
    },
    Entry {
        id: "settings.agentCLI",
        title: "Agent CLI",
        category: Advanced,
        aliases: "cli terminal claude codex install path mcp",
        value: details,
    },
    Entry {
        id: "settings.logs",
        title: "Logs",
        category: Advanced,
        aliases: "diagnostics logs folder support",
        value: details,
    },
    Entry {
        id: "settings.version",
        title: "Version",
        category: Advanced,
        aliases: "build about version",
        value: |_| env!("CARGO_PKG_VERSION").into(),
    },
];

/// Entries whose title, aliases or category contain every query word.
pub fn search(query: &str) -> Vec<&'static Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    ENTRIES
        .iter()
        .filter(|entry| {
            let haystack = format!(
                "{} {} {}",
                entry.title,
                entry.aliases,
                category_title(entry.category)
            )
            .to_lowercase();
            words.iter().all(|word| haystack.contains(word.as_str()))
        })
        .collect()
}

pub fn category_title(category: SettingsCategory) -> &'static str {
    match category {
        General => "General",
        Meetings => "Meetings",
        DayMemory => "Day Memory",
        Writing => "Writing",
        Dictation => "Dictation",
        Models => "Models",
        Privacy => "Privacy & Data",
        Advanced => "Advanced",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_every_word() {
        let hits: Vec<&str> = search("dark").iter().map(|e| e.id).collect();
        assert_eq!(hits, ["settings.appTheme"]);
        assert!(
            search("shortcut ask")
                .iter()
                .any(|e| e.id == "settings.quickRecallShortcut")
        );
        assert!(search("   ").is_empty());
        let ids: std::collections::HashSet<_> = ENTRIES.iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), ENTRIES.len(), "ids are unique");
    }

    #[test]
    fn features_missing_on_this_platform_are_not_offered() {
        for word in ["obsidian", "chime", "sessions"] {
            assert!(search(word).is_empty(), "{word}");
        }
        let autocomplete: Vec<&str> = search("autocomplete").iter().map(|e| e.id).collect();
        assert_eq!(autocomplete, ["settings.autocompleteAvailability"]);
        let updates: Vec<&str> = search("updates").iter().map(|e| e.id).collect();
        assert_eq!(updates, ["settings.currentVersion"]);
    }

    #[test]
    fn screen_context_settings_are_searchable() {
        let ocr: Vec<&str> = search("ocr").iter().map(|e| e.id).collect();
        assert!(ocr.contains(&"settings.screenContextCaptureMode"));
        assert!(ocr.contains(&"settings.screenTextRecognition"));
        assert!(ocr.contains(&"settings.keepOCRTextForever"));
        assert!(!search("screenshot").is_empty());
    }
}
