//! `transcript.json` (`LokalBot/Models/Transcript.swift`, `SpeakerAttribution.swift`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub engine: String,
    /// Canonical speaker key (trimmed, lowercase) to the name the user gave it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub speaker_aliases: BTreeMap<String, String>,
    /// Canonical speaker key to an opaque calendar participant ID.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        rename = "speakerCalendarIdentityIDs"
    )]
    pub speaker_calendar_identity_ids: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo_report: Option<EchoReport>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// Seconds from the start of the recording.
    pub start: f64,
    pub end: f64,
    /// `me`/`them` (legacy), `local`/`them`, or diarized `local N`/`them N`.
    pub speaker: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing_precision: Option<TimingPrecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<SpeakerAttribution>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimingPrecision {
    Coarse,
    Span,
    Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerAttribution {
    pub source: AttributionSource,
    pub identity: SpeakerIdentity,
    pub method: AttributionMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttributionSource {
    Microphone,
    System,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeakerIdentity {
    User,
    Other,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttributionMethod {
    Track,
    Diarization,
    OverlappingSpeech,
    SuspectedEcho,
    Confirmation,
    Profile,
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EchoReport {
    pub status: EchoStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "reductionDB"
    )]
    pub reduction_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_seconds: Option<f64>,
    #[serde(default)]
    pub alignment_verified: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EchoStatus {
    Disabled,
    NoReference,
    Applied,
    Uncertain,
    Failed,
}

impl Transcript {
    /// Trimmed, lowercased key used for aliases and colors.
    pub fn canonical_speaker_key(speaker: &str) -> String {
        speaker.trim().to_lowercase()
    }

    /// Default names when no alias exists: Me, Them, Local speaker, Speaker unclear.
    pub fn default_speaker_name(speaker: &str) -> String {
        let key = Self::canonical_speaker_key(speaker);
        let base = key.rsplit(':').next().unwrap_or(&key).to_string();
        match base.as_str() {
            "me" | "local" => "Me".into(),
            "them" => "Them".into(),
            "local unclear" | "them unclear" | "unclear" => "Speaker unclear".into(),
            other if other.starts_with("local ") => "Local speaker".into(),
            other if other.starts_with("them ") => {
                let n = other.trim_start_matches("them ").trim();
                format!("Speaker {n}")
            }
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => "Speaker unclear".into(),
                }
            }
        }
    }

    /// Display name for a segment's speaker, honouring user aliases.
    pub fn speaker_name(&self, speaker: &str) -> String {
        let key = Self::canonical_speaker_key(speaker);
        self.speaker_aliases
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Self::default_speaker_name(speaker))
    }

    /// `segment-%04d-%010lld-%010lld`: index, start ms, end ms.
    pub fn segment_id(index: usize, segment: &Segment) -> String {
        format!(
            "segment-{:04}-{:010}-{:010}",
            index,
            (segment.start * 1000.0).round() as i64,
            (segment.end * 1000.0).round() as i64
        )
    }

    /// Text with Whisper-style `<|…|>` control tokens removed and whitespace collapsed.
    pub fn display_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("<|") {
            out.push_str(&rest[..start]);
            match rest[start..].find("|>") {
                Some(end) => rest = &rest[start + end + 2..],
                None => {
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// `transcript.md`: `**[HH:MM:SS] Name:** text`, blank-line separated.
    pub fn markdown(&self) -> String {
        self.segments
            .iter()
            .filter_map(|segment| {
                let text = Self::display_text(&segment.text);
                (!text.is_empty()).then(|| {
                    format!(
                        "**[{}] {}:** {}",
                        crate::time::stamp(segment.start),
                        self.speaker_name(&segment.speaker),
                        text
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn word_count(&self) -> usize {
        self.segments
            .iter()
            .map(|s| s.text.split_whitespace().count())
            .sum()
    }

    pub fn duration(&self) -> f64 {
        self.segments.iter().map(|s| s.end).fold(0.0, f64::max)
    }
}

impl SpeakerAttribution {
    pub fn microphone_user() -> Self {
        Self {
            source: AttributionSource::Microphone,
            identity: SpeakerIdentity::User,
            method: AttributionMethod::Track,
        }
    }

    pub fn system_other() -> Self {
        Self {
            source: AttributionSource::System,
            identity: SpeakerIdentity::Other,
            method: AttributionMethod::Track,
        }
    }
}
