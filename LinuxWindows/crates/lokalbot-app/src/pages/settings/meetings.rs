//! Meetings: detection, processing, summaries, speakers and note export.

use gpui_kit::component::button::Button;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{Context, ParentElement as _, PathPromptOptions, SharedString, Styled as _, px};
use lokalbot_core::settings::{AutoRecordMode, ExportFormat, NoteTemplate};

use super::{Section, SettingsPage};
use crate::state::SettingsCategory;
use crate::ui::settings_row;

/// `LanguageCode.summaryPresets` with their display names.
pub const SUMMARY_LANGUAGES: [(&str, &str); 24] = [
    ("en", "English"),
    ("zh", "Simplified Chinese (Mandarin)"),
    ("zh-Hant", "Traditional Chinese (Mandarin)"),
    ("yue", "Cantonese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("pt", "Portuguese (Brazil)"),
    ("pt-PT", "Portuguese (Portugal)"),
    ("ru", "Russian"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("tr", "Turkish"),
    ("pl", "Polish"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("no", "Norwegian (Bokmål)"),
    ("nn", "Norwegian (Nynorsk)"),
    ("th", "Thai"),
    ("vi", "Vietnamese"),
];

pub fn summary_language_label(value: &str) -> String {
    if value == "match" || value.is_empty() {
        return "Match transcript (auto)".into();
    }
    if let Some(name) = value.strip_prefix("custom:") {
        return if name.is_empty() {
            "Custom".into()
        } else {
            name.to_string()
        };
    }
    SUMMARY_LANGUAGES
        .iter()
        .find(|(code, _)| *code == value)
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| value.to_string())
}

fn template_description(template: NoteTemplate) -> &'static str {
    match template {
        NoteTemplate::Meeting => "TL;DR · Key points · Decisions · Action items · Open questions",
        NoteTemplate::Lecture => {
            "TL;DR · Concepts · Definitions · Examples · Questions to review · Action items"
        }
        NoteTemplate::StudyGuide => {
            "TL;DR · Key concepts · Flashcards · Practice questions · Action items"
        }
        NoteTemplate::Podcast => "TL;DR · Topics · Quotes · Insights · Action items",
        NoteTemplate::Freeform => "Topical bullets · Action items",
    }
}

/// Apps whose calls LokalBot notices on Linux and Windows.
const DETECTED_APPS: &str = "Zoom, Microsoft Teams, Slack, Webex, and Google Meet in a browser";

impl SettingsPage {
    pub(super) fn meetings_sections(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let data_entity = self.data.clone();
        let (settings, microphones) = {
            let data = self.data.read(cx);
            (data.settings.clone(), data.audio_inputs.clone())
        };

        let debounce = settings.stop_debounce_seconds.round().clamp(5., 120.);
        let debounce_data = data_entity.clone();
        let mut microphone_options: Vec<(SharedString, SharedString)> =
            vec![("".into(), "System default".into())];
        microphone_options.extend(microphones.iter().map(|name| (name.clone(), name.clone())));
        if !settings.microphone_device.is_empty()
            && !microphones
                .iter()
                .any(|m| m.as_ref() == settings.microphone_device)
        {
            let name: SharedString = settings.microphone_device.clone().into();
            microphone_options.push((name.clone(), name));
        }
        let microphone_current = microphone_options
            .iter()
            .find(|(value, _)| value.as_ref() == settings.microphone_device)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| "System default".into());
        let microphone_items: Vec<(SharedString, bool)> = microphone_options
            .iter()
            .map(|(value, label)| (label.clone(), value.as_ref() == settings.microphone_device))
            .collect();
        let microphone_values: Vec<String> = microphone_options
            .iter()
            .map(|(value, _)| value.to_string())
            .collect();
        let microphone_data = data_entity.clone();

        let meetings = Section::new("Meetings")
            .row(
                "settings.autoRecordMode",
                self.choice(
                    "settings.autoRecordMode",
                    "When a meeting is detected",
                    Some("Only record when everyone has been informed and any consent the meeting or location requires is in place.".into()),
                    AutoRecordMode::ALL.iter().map(|m| (*m, SharedString::from(m.label()))).collect(),
                    settings.auto_record_mode,
                    |s, v| s.auto_record_mode = v,
                    cx,
                ),
            )
            .row(
                "settings.detectedApps",
                settings_row::row(
                    "settings.detectedApps",
                    "Detected apps",
                    Some(DETECTED_APPS.into()),
                    gpui_kit::div(),
                    false,
                    cx,
                ),
            )
            .row(
                "settings.stopDebounceSeconds",
                settings_row::row(
                    "settings.stopDebounceSeconds",
                    "Wait before stopping",
                    None,
                    settings_row::stepper(
                        "settings-stop-debounce",
                        format!("{debounce} s after audio stops"),
                        debounce > 5.,
                        debounce < 120.,
                        move |step, _, cx| {
                            let next = (debounce + f64::from(step) * 5.).clamp(5., 120.);
                            debounce_data.update(cx, |data, cx| data.update_settings(|s| s.stop_debounce_seconds = next, cx));
                        },
                        cx,
                    ),
                    self.is_focused("settings.stopDebounceSeconds"),
                    cx,
                ),
            )
            .row(
                "settings.recordSystemAudio",
                self.toggle(
                    "settings.recordSystemAudio",
                    "Record system audio",
                    Some("Records the other side of calls from your speakers or headset. Without it, only your microphone is recorded."),
                    settings.record_system_audio,
                    |s, v| s.record_system_audio = v,
                    cx,
                ),
            )
            .row(
                "settings.microphoneDevice",
                settings_row::row(
                    "settings.microphoneDevice",
                    "Microphone",
                    None,
                    settings_row::picker("settings-microphone", microphone_current, microphone_items, move |index, _, cx| {
                        if let Some(value) = microphone_values.get(index).cloned() {
                            microphone_data.update(cx, |data, cx| data.update_settings(|s| s.microphone_device = value, cx));
                        }
                    }),
                    self.is_focused("settings.microphoneDevice"),
                    cx,
                ),
            );

        let models_data = data_entity.clone();
        let processing = Section::new("Processing")
            .row(
                "settings.autoTranscribe",
                self.toggle(
                    "settings.autoTranscribe",
                    "Transcribe automatically after each meeting",
                    None,
                    settings.auto_transcribe,
                    |s, v| s.auto_transcribe = v,
                    cx,
                ),
            )
            .row(
                "settings.autoSummarize",
                self.toggle(
                    "settings.autoSummarize",
                    "Summarize automatically after transcription",
                    None,
                    settings.auto_summarize,
                    |s, v| s.auto_summarize = v,
                    cx,
                ),
            )
            .row(
                "settings.processingModels",
                settings_row::row(
                    "settings.processingModels",
                    "Transcribe and Think models",
                    None,
                    Button::new("settings-manage-models")
                        .outline()
                        .small()
                        .label("Manage in Models…")
                        .on_click(move |_, _, cx| {
                            models_data.update(cx, |data, cx| {
                                data.open_settings(SettingsCategory::Models, cx)
                            });
                        }),
                    false,
                    cx,
                ),
            );

        let mut languages: Vec<(&'static str, SharedString)> =
            vec![("match", "Match transcript (auto)".into())];
        languages.extend(
            SUMMARY_LANGUAGES
                .iter()
                .map(|(code, name)| (*code, SharedString::from(*name))),
        );
        let current_language = languages
            .iter()
            .find(|(code, _)| *code == settings.summary_language)
            .map(|(code, _)| *code)
            .unwrap_or("match");
        let summarization = Section::new("Summarization")
            .row(
                "settings.noteTemplate",
                self.choice(
                    "settings.noteTemplate",
                    "Notes template",
                    Some(template_description(settings.note_template).into()),
                    NoteTemplate::ALL
                        .iter()
                        .map(|t| (*t, SharedString::from(t.label())))
                        .collect(),
                    settings.note_template,
                    |s, v| s.note_template = v,
                    cx,
                ),
            )
            .row(
                "settings.summaryLanguage",
                self.choice(
                    "settings.summaryLanguage",
                    "Notes language",
                    None,
                    languages,
                    current_language,
                    |s, v| s.summary_language = v.to_string(),
                    cx,
                ),
            );

        let speakers = Section::new("Speaker names")
            .row(
                "settings.multiSpeakerDiarization",
                self.toggle(
                    "settings.multiSpeakerDiarization",
                    "Separate voices by speaker",
                    None,
                    settings.multi_speaker_diarization,
                    |s, v| s.multi_speaker_diarization = v,
                    cx,
                ),
            )
            .row(
                "settings.microphoneIsUser",
                self.toggle(
                    "settings.microphoneIsUser",
                    "My microphone is me",
                    Some("Treats speech from this PC's microphone as yours, so your commitments lead each meeting's action items. Turn off when several people share this microphone, such as in a meeting room. Summarize a meeting again to update its action items."),
                    settings.microphone_is_user,
                    |s, v| s.microphone_is_user = v,
                    cx,
                ),
            );

        let folder = settings.meeting_note_export_folder.clone();
        let folder_label: SharedString = if folder.is_empty() {
            "Choose…".into()
        } else {
            std::path::Path::new(&folder)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(folder.clone())
                .into()
        };
        let export_enabled = settings.meeting_note_export_enabled;
        let export_data = data_entity.clone();
        let choose_data = data_entity.clone();
        let mut export = Section::new("Meeting note export").row(
            "settings.meetingNoteExportEnabled",
            settings_row::row(
                "settings.meetingNoteExportEnabled",
                "Export a note for each meeting",
                Some("Writes one unencrypted Markdown note per meeting, including meetings already recorded, to a folder you choose, such as an Obsidian vault. Each note has the summary, action items as you corrected them, and the people and tracked projects involved. A note you edit, move, or delete is yours: LokalBot leaves it alone.".into()),
                settings_row::switch("settings.meetingNoteExportEnabled", export_enabled, move |on, _, cx| {
                    export_data.update(cx, |data, cx| data.update_settings(|s| s.meeting_note_export_enabled = on, cx));
                    if on {
                        choose_export_folder(export_data.clone(), true, cx);
                    }
                }),
                self.is_focused("settings.meetingNoteExportEnabled"),
                cx,
            ),
        );
        if export_enabled {
            export = export
                .row(
                    "settings.meetingNoteExportFormat",
                    self.choice(
                        "settings.meetingNoteExportFormat",
                        "Format",
                        None,
                        vec![
                            (ExportFormat::Markdown, "Markdown".into()),
                            (ExportFormat::Obsidian, "Obsidian".into()),
                            (ExportFormat::Logseq, "Logseq".into()),
                        ],
                        settings.meeting_note_export_format,
                        |s, v| s.meeting_note_export_format = v,
                        cx,
                    ),
                )
                .row(
                    "settings.meetingNoteExportIncludesTranscript",
                    self.toggle(
                        "settings.meetingNoteExportIncludesTranscript",
                        "Include the transcript",
                        None,
                        settings.meeting_note_export_includes_transcript,
                        |s, v| s.meeting_note_export_includes_transcript = v,
                        cx,
                    ),
                )
                .row(
                    "settings.meetingNoteExportFolder",
                    settings_row::row(
                        "settings.meetingNoteExportFolder",
                        "Folder",
                        (!folder.is_empty()).then(|| SharedString::from(folder.clone())),
                        h_flex().gap(px(8.)).child(settings_row::link_button(
                            "settings-export-folder",
                            folder_label,
                            move |_, cx| choose_export_folder(choose_data.clone(), false, cx),
                            cx,
                        )),
                        false,
                        cx,
                    ),
                );
        }

        vec![meetings, processing, summarization, speakers, export]
    }
}

/// Asks for the export folder; turning export on without choosing one turns it off again.
fn choose_export_folder(
    data: gpui_kit::Entity<crate::state::AppData>,
    enabling: bool,
    cx: &mut gpui_kit::App,
) {
    if enabling && !data.read(cx).settings.meeting_note_export_folder.is_empty() {
        return;
    }
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Choose".into()),
    });
    cx.spawn(async move |cx| {
        let chosen = receiver
            .await
            .ok()
            .and_then(|result| result.ok())
            .flatten()
            .and_then(|paths| paths.into_iter().next());
        data.update(cx, |data, cx| match chosen {
            Some(path) => data.update_settings(
                |s| s.meeting_note_export_folder = path.to_string_lossy().to_string(),
                cx,
            ),
            None if data.settings.meeting_note_export_folder.is_empty() => {
                data.update_settings(|s| s.meeting_note_export_enabled = false, cx)
            }
            None => {}
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_labels() {
        assert_eq!(summary_language_label("match"), "Match transcript (auto)");
        assert_eq!(summary_language_label("pt-PT"), "Portuguese (Portugal)");
        assert_eq!(summary_language_label("custom:Klingon"), "Klingon");
    }
}
