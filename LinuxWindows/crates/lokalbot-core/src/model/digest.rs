//! Day Digest: `journal/YYYY-MM-DD.md` plus its `.meta.json` sidecar
//! (`DayDigestJournalWriter.swift`, `DayDigestPresentation.swift`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::time::iso_seconds;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DigestQuality {
    #[default]
    Complete,
    Partial,
    Fallback,
}

/// `journal/YYYY-MM-DD.meta.json` (v2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestMeta {
    #[serde(default = "default_meta_version")]
    pub version: u32,
    #[serde(default)]
    pub quality: DigestQuality,
    #[serde(with = "iso_seconds")]
    pub generated_at: DateTime<Utc>,
    #[serde(
        default,
        with = "iso_seconds::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub evidence_latest_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meeting_evidence_signature: Option<String>,
    /// SHA-256 of the journal Markdown. A journal whose digest no longer
    /// matches was edited by the user and is never overwritten.
    #[serde(default)]
    pub journal_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

fn default_meta_version() -> u32 {
    2
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Completed,
    InProgress,
    Blocked,
    Unknown,
}

/// One task in the digest (`### Tasks` bullets).
#[derive(Clone, Debug, PartialEq)]
pub struct DigestTask {
    pub title: String,
    pub status: TaskStatus,
    pub summary: String,
    pub next_step: Option<String>,
    pub time_range: Option<String>,
    /// `[screen:N]` source IDs, when present.
    pub screen_sources: Vec<i64>,
}

/// The digest parsed back from Markdown for display.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayDigest {
    pub day: Option<chrono::NaiveDate>,
    pub markdown: String,
    /// "At a glance" text, shown as Highlights (at most 3 bullets on Today).
    pub highlights: Vec<String>,
    pub tasks: Vec<DigestTask>,
    pub decisions: Vec<String>,
    pub blockers: Vec<String>,
    pub follow_ups: Vec<String>,
    pub meetings_markdown: Option<String>,
    pub meta: Option<DigestMeta>,
    /// Newer evidence exists than what the digest covers.
    pub is_stale: bool,
}
