//! `outcomes.json` and `outcome-state.json` (`LokalBot/Models/MeetingOutcomes.swift`,
//! `MeetingOutcomeState.swift`).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::uuid_upper_serde;
use crate::time::iso_millis;

pub const OUTCOMES_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingOutcomes {
    #[serde(default = "default_outcomes_schema")]
    pub schema_version: u32,
    /// Matches `Transcript` evidence revision; a mismatch makes outcomes stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_revision: Option<String>,
    #[serde(default, alias = "action_items")]
    pub action_items: Vec<ActionItem>,
    #[serde(default, alias = "decisions", deserialize_with = "decisions_compat")]
    pub decision_records: Vec<Decision>,
    #[serde(default, alias = "open_questions")]
    pub open_questions: Vec<String>,
}

fn default_outcomes_schema() -> u32 {
    OUTCOMES_SCHEMA_VERSION
}

/// v1 stored decisions as plain strings.
fn decisions_compat<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Decision>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Text(String),
        Record(Decision),
    }
    let raw = Vec::<Either>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(index, item)| match item {
            Either::Record(record) => record,
            Either::Text(text) => Decision {
                id: format!("legacy-decision-{index}"),
                schema_version: 1,
                text,
                citations: Vec::new(),
                attribution: None,
            },
        })
        .collect())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub id: String,
    #[serde(default = "default_outcomes_schema")]
    pub schema_version: u32,
    pub text: String,
    /// As named; `Me` for the user's own items; `None` when unresolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The due phrase as spoken ("by Friday"), not normalized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default)]
    pub is_for_user: bool,
    #[serde(default = "default_importance")]
    pub importance: u8,
    #[serde(default)]
    pub citations: Vec<OutcomeSourceCitation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<ActionAttribution>,
}

fn default_importance() -> u8 {
    3
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: String,
    #[serde(default = "default_outcomes_schema")]
    pub schema_version: u32,
    pub text: String,
    #[serde(default)]
    pub citations: Vec<OutcomeSourceCitation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<DecisionAttribution>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeSourceCitation {
    #[serde(default, with = "uuid_upper_serde::option", skip_serializing_if = "Option::is_none", rename = "meetingID")]
    pub meeting_id: Option<Uuid>,
    #[serde(rename = "segmentID")]
    pub segment_id: String,
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionAttribution {
    pub resolution: OwnerResolution,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "speakerID")]
    pub speaker_id: Option<String>,
    pub basis: AttributionBasis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OwnerResolution {
    User,
    Other,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttributionBasis {
    Commitment,
    Assignment,
    Request,
    Unclear,
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionAttribution {
    #[serde(rename = "speakerID")]
    pub speaker_id: String,
    pub speaker_label: String,
    pub identity: crate::model::transcript::SpeakerIdentity,
    pub quote: String,
}

/// `outcome-state.json`: the user's corrections layered over extracted actions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeState {
    #[serde(default = "default_state_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub actions: BTreeMap<String, ActionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unmatched_actions: Option<BTreeMap<String, ActionState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unmatched_action_text: Option<BTreeMap<String, String>>,
}

fn default_state_schema() -> u32 {
    2
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionStatus {
    #[default]
    Open,
    Done,
    Deferred,
}

impl ActionStatus {
    pub fn label(self) -> &'static str {
        match self {
            ActionStatus::Open => "Open",
            ActionStatus::Done => "Done",
            ActionStatus::Deferred => "Deferred",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionState {
    pub status: ActionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_override: Option<String>,
    #[serde(default)]
    pub owner_was_cleared: bool,
    #[serde(default)]
    pub due_was_cleared: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_correction: Option<String>,
    #[serde(default, with = "iso_millis::option", skip_serializing_if = "Option::is_none")]
    pub text_corrected_at: Option<DateTime<Utc>>,
    #[serde(default, with = "iso_millis::option", skip_serializing_if = "Option::is_none")]
    pub owner_corrected_at: Option<DateTime<Utc>>,
    #[serde(default, with = "iso_millis::option", skip_serializing_if = "Option::is_none")]
    pub due_corrected_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub is_thread_excluded: bool,
    #[serde(with = "iso_millis")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub user_edited: bool,
}

impl ActionState {
    pub fn new(status: ActionStatus) -> Self {
        Self {
            status,
            owner_override: None,
            due_override: None,
            owner_was_cleared: false,
            due_was_cleared: false,
            text_correction: None,
            text_corrected_at: None,
            owner_corrected_at: None,
            due_corrected_at: None,
            is_thread_excluded: false,
            updated_at: Utc::now(),
            user_edited: true,
        }
    }
}

/// An action item with the user's corrections applied, joined to its meeting.
/// `id` is `"<meetingUUID>:<actionID>"` (`MeetingOutcomeProjection.swift`).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionReference {
    pub id: String,
    pub meeting_id: Uuid,
    pub meeting_title: String,
    pub meeting_started_at: DateTime<Utc>,
    pub action_id: String,
    pub text: String,
    pub original_text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub resolved_due_date: Option<chrono::NaiveDate>,
    pub is_for_user: bool,
    pub status: ActionStatus,
    pub importance: u8,
    pub citations: Vec<OutcomeSourceCitation>,
}

impl ActionReference {
    pub fn first_citation(&self) -> Option<&OutcomeSourceCitation> {
        self.citations.first()
    }

    pub fn is_overdue(&self, today: chrono::NaiveDate) -> bool {
        self.status != ActionStatus::Done && self.resolved_due_date.is_some_and(|due| due < today)
    }
}

/// The same commitment across meetings (`ActionThreads.swift`).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionThread {
    pub id: String,
    pub canonical_text: String,
    pub references: Vec<ActionReference>,
}

impl ActionThread {
    /// Open if any member is open, else deferred if any, else done.
    pub fn status(&self) -> ActionStatus {
        if self.references.iter().any(|r| r.status == ActionStatus::Open) {
            ActionStatus::Open
        } else if self.references.iter().any(|r| r.status == ActionStatus::Deferred) {
            ActionStatus::Deferred
        } else {
            ActionStatus::Done
        }
    }

    pub fn is_mixed(&self) -> bool {
        let first = self.references.first().map(|r| r.status);
        self.references.iter().any(|r| Some(r.status) != first)
    }

    /// The newest member: its text, owner and due are shown for the thread.
    pub fn latest(&self) -> &ActionReference {
        self.references
            .iter()
            .max_by_key(|r| r.meeting_started_at)
            .expect("a thread has at least one reference")
    }
}
